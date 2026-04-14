use crate::backup::parse_backup_timestamp;
use crate::config::ResolvedConfig;
use crate::http::{AdminClient, AdminUserSummary, ServerStatusResponse, UserStatsResponse};
use crate::output::{note, print_json};
use anyhow::Result;
use serde::Serialize;
use time::{Duration, OffsetDateTime};

const RECENT_SYNC_WINDOW_HOURS: i64 = 24;
const TLS_EXPIRY_WARNING_DAYS: i64 = 14;
const BACKUP_STALE_WARNING_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DoctorCheckStatus {
    Ok,
    Warn,
    Fail,
    Unsupported,
}

#[derive(Debug, Serialize)]
struct DoctorCheck {
    id: &'static str,
    title: &'static str,
    status: DoctorCheckStatus,
    summary: String,
    details: Vec<String>,
    guidance: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DoctorJson {
    healthy: bool,
    degraded: bool,
    checks: Vec<DoctorCheck>,
}

struct DoctorUserRow {
    user: AdminUserSummary,
    stats: Option<UserStatsResponse>,
}

struct DoctorReport {
    checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    fn healthy(&self) -> bool {
        !self
            .checks
            .iter()
            .any(|check| check.status == DoctorCheckStatus::Fail)
    }

    fn degraded(&self) -> bool {
        self.checks.iter().any(|check| {
            matches!(
                check.status,
                DoctorCheckStatus::Warn | DoctorCheckStatus::Unsupported
            )
        })
    }
}

pub(crate) fn run(cfg: &ResolvedConfig, json: bool) -> Result<bool> {
    let client = crate::require_client(cfg)?;
    let report = build_report(&client);
    let healthy = report.healthy();
    let degraded = report.degraded();

    if json {
        print_json(&DoctorJson {
            healthy,
            degraded,
            checks: report.checks,
        })?;
    } else {
        render_report(&report);
    }

    Ok(healthy)
}

fn build_report(client: &AdminClient) -> DoctorReport {
    let mut checks = Vec::new();
    let mut users_for_sync: Option<Vec<AdminUserSummary>> = None;

    match client.health() {
        Ok(health) => checks.push(DoctorCheck {
            id: "server_reachable",
            title: "Server Connection",
            status: DoctorCheckStatus::Ok,
            summary: format!("Server reachable at {}", client.base_url()),
            details: vec![
                format!("Health: {}", health.status),
                format!("Pending tasks: {}", health.pending_tasks),
            ],
            guidance: vec![],
        }),
        Err(err) => checks.push(DoctorCheck {
            id: "server_reachable",
            title: "Server Connection",
            status: DoctorCheckStatus::Fail,
            summary: err.to_string(),
            details: vec![format!("Target: {}", client.base_url())],
            guidance: vec![
                "Check the configured --server URL, DNS, firewall, and that cmdock/server is running."
                    .to_string(),
            ],
        }),
    }

    match client.tls_certificate_info() {
        Ok(info) => {
            let summary = format!(
                "TLS certificate valid until {} ({})",
                crate::format_offset_datetime(OffsetDateTime::from(info.not_after)),
                crate::describe_optional_duration(info.expires_in)
            );
            let status = match info.expires_in {
                Some(duration)
                    if duration.as_secs() <= (TLS_EXPIRY_WARNING_DAYS as u64) * 24 * 60 * 60 =>
                {
                    DoctorCheckStatus::Warn
                }
                Some(_) => DoctorCheckStatus::Ok,
                None => DoctorCheckStatus::Fail,
            };
            let guidance = if status == DoctorCheckStatus::Warn {
                vec![
                    "Renew the server certificate soon so new clients do not fail HTTPS validation."
                        .to_string(),
                ]
            } else if status == DoctorCheckStatus::Fail {
                vec!["Replace the expired certificate and rerun `cmdock-admin doctor`.".to_string()]
            } else {
                vec![]
            };
            checks.push(DoctorCheck {
                id: "tls",
                title: "TLS",
                status,
                summary,
                details: vec![],
                guidance,
            });
        }
        Err(err) => checks.push(DoctorCheck {
            id: "tls",
            title: "TLS",
            status: DoctorCheckStatus::Fail,
            summary: err.to_string(),
            details: vec![],
            guidance: vec![
                "Check that the server URL is HTTPS, the certificate is valid, and this machine trusts the issuing CA."
                    .to_string(),
            ],
        }),
    }

    let admin_ok = match client.admin_status() {
        Ok(status) => {
            checks.push(DoctorCheck {
                id: "admin_auth",
                title: "Admin Auth",
                status: DoctorCheckStatus::Ok,
                summary: "Admin API authenticated".to_string(),
                details: render_admin_status_details(&status),
                guidance: vec![],
            });
            if let Some(version) = status.version.as_ref() {
                checks.push(DoctorCheck {
                    id: "server_version",
                    title: "Server Version",
                    status: DoctorCheckStatus::Ok,
                    summary: format!("Server version: {version}"),
                    details: vec![],
                    guidance: vec![],
                });
            } else {
                checks.push(DoctorCheck {
                    id: "server_version",
                    title: "Server Version",
                    status: DoctorCheckStatus::Unsupported,
                    summary: "/admin/status does not currently report a server version".to_string(),
                    details: vec![],
                    guidance: vec![
                        "Use the server deployment metadata or logs to confirm the running version until /admin/status exposes it."
                            .to_string(),
                    ],
                });
            }
            if let Some(bytes) = status.data_dir_used_bytes {
                checks.push(DoctorCheck {
                    id: "disk_space",
                    title: "Disk Space",
                    status: DoctorCheckStatus::Warn,
                    summary: format!(
                        "/admin/status reports data size {}, but not free disk space",
                        crate::format_bytes(bytes)
                    ),
                    details: vec![],
                    guidance: vec![
                        "Check free disk space on the server host directly until the admin API exposes a free-space metric."
                            .to_string(),
                    ],
                });
            } else {
                checks.push(DoctorCheck {
                    id: "disk_space",
                    title: "Disk Space",
                    status: DoctorCheckStatus::Unsupported,
                    summary: "The current admin API does not report disk usage or free space"
                        .to_string(),
                    details: vec![],
                    guidance: vec![
                        "Check the server host filesystem directly until /admin/status exposes disk-space data."
                            .to_string(),
                    ],
                });
            }
            true
        }
        Err(err) => {
            checks.push(DoctorCheck {
                id: "admin_auth",
                title: "Admin Auth",
                status: DoctorCheckStatus::Fail,
                summary: err.to_string(),
                details: vec![],
                guidance: vec![
                    "Check that the operator bearer token matches CMDOCK_ADMIN_TOKEN on the server."
                        .to_string(),
                ],
            });
            checks.push(DoctorCheck {
                id: "server_version",
                title: "Server Version",
                status: DoctorCheckStatus::Unsupported,
                summary: "Skipped because the admin status check did not succeed".to_string(),
                details: vec![],
                guidance: vec!["Fix the admin-auth check first, then rerun doctor.".to_string()],
            });
            checks.push(DoctorCheck {
                id: "disk_space",
                title: "Disk Space",
                status: DoctorCheckStatus::Unsupported,
                summary: "Skipped because the admin status check did not succeed".to_string(),
                details: vec![],
                guidance: vec!["Fix the admin-auth check first, then rerun doctor.".to_string()],
            });
            false
        }
    };

    if admin_ok {
        match client.list_users() {
            Ok(users) => {
                let status = if users.is_empty() {
                    DoctorCheckStatus::Fail
                } else {
                    DoctorCheckStatus::Ok
                };
                let summary = if users.is_empty() {
                    "No users found".to_string()
                } else {
                    format!("{} user(s) found", users.len())
                };
                let guidance = if users.is_empty() {
                    vec![
                        "Run `cmdock-admin user create <name>` to create the first user."
                            .to_string(),
                    ]
                } else {
                    vec![]
                };
                let details = users
                    .iter()
                    .map(|user| {
                        format!(
                            "{} ({}) - devices: {}, last sync: {}",
                            user.username,
                            user.id,
                            user.device_count,
                            user.last_sync_at
                                .as_deref()
                                .map(crate::describe_sync_timestamp)
                                .unwrap_or_else(|| "never".to_string())
                        )
                    })
                    .collect::<Vec<_>>();
                users_for_sync = Some(users.clone());
                checks.push(DoctorCheck {
                    id: "users_exist",
                    title: "Users",
                    status,
                    summary,
                    details,
                    guidance,
                });
            }
            Err(err) if crate::endpoint_missing(&err) => {
                checks.push(DoctorCheck {
                    id: "users_exist",
                    title: "Users",
                    status: DoctorCheckStatus::Unsupported,
                    summary: "GET /admin/users is not available on this server".to_string(),
                    details: vec![err.to_string()],
                    guidance: vec![
                        "Upgrade cmdock/server to a build that includes cmdock/server#54."
                            .to_string(),
                    ],
                });
            }
            Err(err) => {
                checks.push(DoctorCheck {
                    id: "users_exist",
                    title: "Users",
                    status: DoctorCheckStatus::Fail,
                    summary: err.to_string(),
                    details: vec![],
                    guidance: vec![
                        "Check the admin API logs and rerun `cmdock-admin doctor` once user enumeration is working."
                            .to_string(),
                    ],
                });
            }
        }
    } else {
        checks.push(DoctorCheck {
            id: "users_exist",
            title: "Users",
            status: DoctorCheckStatus::Unsupported,
            summary: "Skipped because the admin-auth check did not succeed".to_string(),
            details: vec![],
            guidance: vec!["Fix the admin-auth check first, then rerun doctor.".to_string()],
        });
    }

    match users_for_sync {
        Some(users) if users.is_empty() => checks.push(DoctorCheck {
            id: "sync_active",
            title: "Sync Activity",
            status: DoctorCheckStatus::Fail,
            summary: "No users exist, so sync activity cannot be checked".to_string(),
            details: vec![],
            guidance: vec!["Run `cmdock-admin user create <name>` first.".to_string()],
        }),
        Some(users) => {
            let mut rows = Vec::new();
            let mut recent_syncs = 0usize;
            let mut stats_supported = true;
            let mut stats_errors = Vec::new();

            for user in users {
                if crate::synced_recently(user.last_sync_at.as_deref()) {
                    recent_syncs += 1;
                }
                match client.user_stats(&user.id) {
                    Ok(stats) => {
                        rows.push(DoctorUserRow {
                            user,
                            stats: Some(stats),
                        });
                    }
                    Err(err) if crate::endpoint_missing(&err) => {
                        stats_supported = false;
                        stats_errors.push(err.to_string());
                        break;
                    }
                    Err(err) => {
                        stats_errors.push(format!("{} ({}): {err}", user.username, user.id));
                        rows.push(DoctorUserRow { user, stats: None });
                    }
                }
            }

            if !stats_supported {
                checks.push(DoctorCheck {
                    id: "sync_active",
                    title: "Sync Activity",
                    status: DoctorCheckStatus::Unsupported,
                    summary: "GET /admin/user/{id}/stats is not available on this server"
                        .to_string(),
                    details: stats_errors,
                    guidance: vec![
                        "Upgrade cmdock/server to a build that exposes per-user stats before relying on doctor sync diagnostics."
                            .to_string(),
                    ],
                });
            } else {
                let status = if recent_syncs > 0 {
                    if stats_errors.is_empty() {
                        DoctorCheckStatus::Ok
                    } else {
                        DoctorCheckStatus::Warn
                    }
                } else {
                    DoctorCheckStatus::Fail
                };
                let summary = if recent_syncs > 0 {
                    format!(
                        "{} user(s) synced within the last {} hours",
                        recent_syncs, RECENT_SYNC_WINDOW_HOURS
                    )
                } else {
                    format!(
                        "No user has synced within the last {} hours",
                        RECENT_SYNC_WINDOW_HOURS
                    )
                };
                let mut details = rows.iter().map(render_doctor_user_row).collect::<Vec<_>>();
                details.extend(stats_errors);
                let guidance = if recent_syncs == 0 {
                    vec![
                        "Check the client config, then run `task sync` or reconnect the native client."
                            .to_string(),
                    ]
                } else if rows.iter().any(|row| row.stats.is_none()) {
                    vec![
                        "Investigate the users whose stats could not be read, then rerun doctor."
                            .to_string(),
                    ]
                } else {
                    vec![]
                };
                checks.push(DoctorCheck {
                    id: "sync_active",
                    title: "Sync Activity",
                    status,
                    summary,
                    details,
                    guidance,
                });
            }
        }
        None => checks.push(DoctorCheck {
            id: "sync_active",
            title: "Sync Activity",
            status: DoctorCheckStatus::Unsupported,
            summary: "Skipped because user enumeration did not succeed".to_string(),
            details: vec![],
            guidance: vec!["Fix the users check first, then rerun doctor.".to_string()],
        }),
    }

    match client.list_backups() {
        Ok(backups) if backups.is_empty() => checks.push(DoctorCheck {
            id: "backups",
            title: "Backups",
            status: DoctorCheckStatus::Warn,
            summary: "No backup snapshots found".to_string(),
            details: vec![],
            guidance: vec![
                "Run `cmdock-admin backup`, then copy the staging directory off-host with your existing backup tooling."
                    .to_string(),
                "Check the backup staging directory permissions on the server host directly, especially if backups include secrets."
                    .to_string(),
            ],
        }),
        Ok(backups) => {
            let latest_full = backups
                .iter()
                .find(|backup| backup.backup_type == "full")
                .unwrap_or(&backups[0]);
            let latest_age = parse_backup_timestamp(&latest_full.timestamp)
                .map(|timestamp| crate::describe_age(timestamp, OffsetDateTime::now_utc()));
            let stale = parse_backup_timestamp(&latest_full.timestamp)
                .map(|timestamp| {
                    OffsetDateTime::now_utc() - timestamp
                        > Duration::days(BACKUP_STALE_WARNING_DAYS)
                })
                .unwrap_or(false);
            let summary = match latest_age {
                Some(age) => format!("Latest full backup: {} ({age})", latest_full.timestamp),
                None => format!("Latest full backup: {}", latest_full.timestamp),
            };
            let mut details = vec![
                format!("Snapshots visible: {}", backups.len()),
                format!("Users: {}", latest_full.users),
                format!(
                    "Tasks: {}",
                    latest_full
                        .task_count
                        .map(|count| count.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ),
                format!("Size: {}", crate::format_bytes(latest_full.total_size_bytes)),
                format!(
                    "Secrets included: {}",
                    crate::yes_no(latest_full.secrets_included)
                ),
                format!("Server version: {}", latest_full.server_version),
            ];
            if latest_full.backup_type != "full" {
                details.push(format!("Latest snapshot type: {}", latest_full.backup_type));
            }
            let mut guidance = Vec::new();
            if stale {
                guidance.push(format!(
                    "Latest full backup is older than {} day(s). Run `cmdock-admin backup` and verify your off-host copy job.",
                    BACKUP_STALE_WARNING_DAYS
                ));
            }
            if latest_full.secrets_included {
                guidance.push(
                    "Because backups include secrets, verify that the backup staging directory is not broadly readable on the server host."
                        .to_string(),
                );
            } else {
                guidance.push(
                    "Check the backup staging directory permissions on the server host directly if you later enable secret-inclusive backups."
                        .to_string(),
                );
            }
            checks.push(DoctorCheck {
                id: "backups",
                title: "Backups",
                status: if stale {
                    DoctorCheckStatus::Warn
                } else {
                    DoctorCheckStatus::Ok
                },
                summary,
                details,
                guidance,
            });
        }
        Err(err) if crate::endpoint_missing(&err) => checks.push(DoctorCheck {
            id: "backups",
            title: "Backups",
            status: DoctorCheckStatus::Unsupported,
            summary: "Backup endpoints are not available on this server".to_string(),
            details: vec![err.to_string()],
            guidance: vec![
                "Upgrade cmdock/server to a build that includes cmdock/server#57."
                    .to_string(),
            ],
        }),
        Err(err) => checks.push(DoctorCheck {
            id: "backups",
            title: "Backups",
            status: DoctorCheckStatus::Fail,
            summary: err.to_string(),
            details: vec![],
            guidance: vec![
                "Fix the backup staging directory or admin API error, then rerun `cmdock-admin doctor`."
                    .to_string(),
            ],
        }),
    }

    match client.list_admin_webhooks() {
        Ok(webhooks) => {
            let disabled = webhooks.iter().filter(|webhook| !webhook.enabled).count();
            let summary = if webhooks.is_empty() {
                "No admin webhooks configured".to_string()
            } else if disabled == 0 {
                format!("{} admin webhook(s) configured and enabled", webhooks.len())
            } else {
                format!(
                    "{} admin webhook(s) configured, {} disabled",
                    webhooks.len(),
                    disabled
                )
            };
            let mut details = webhooks
                .iter()
                .map(|webhook| {
                    format!(
                        "{} {} ({})",
                        if webhook.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        },
                        webhook.id,
                        webhook.events.join(",")
                    )
                })
                .collect::<Vec<_>>();
            details.truncate(5);
            let guidance = if disabled == 0 {
                vec!["Run `cmdock-admin webhook list` to review configured endpoints.".to_string()]
            } else {
                vec![
                    "Run `cmdock-admin webhook list` to inspect disabled hooks.".to_string(),
                    "Use `cmdock-admin webhook enable <id>` after fixing the target endpoint."
                        .to_string(),
                ]
            };
            checks.push(DoctorCheck {
                id: "webhooks",
                title: "Webhooks",
                status: if disabled == 0 {
                    DoctorCheckStatus::Ok
                } else {
                    DoctorCheckStatus::Warn
                },
                summary,
                details,
                guidance,
            });
        }
        Err(err) if crate::endpoint_missing(&err) => checks.push(DoctorCheck {
            id: "webhooks",
            title: "Webhooks",
            status: DoctorCheckStatus::Unsupported,
            summary: "Admin webhook endpoints are not available on this server".to_string(),
            details: vec![err.to_string()],
            guidance: vec![
                "Upgrade cmdock/server to a build that includes cmdock/server#65.".to_string(),
            ],
        }),
        Err(err) => checks.push(DoctorCheck {
            id: "webhooks",
            title: "Webhooks",
            status: DoctorCheckStatus::Fail,
            summary: err.to_string(),
            details: vec![],
            guidance: vec![
                "Fix the admin webhook API error, then rerun `cmdock-admin doctor`.".to_string(),
            ],
        }),
    }

    DoctorReport { checks }
}

fn render_report(report: &DoctorReport) {
    for check in &report.checks {
        note(check.title);
        note(format!("  {} {}", doctor_icon(check.status), check.summary));
        for detail in &check.details {
            note(format!("    {detail}"));
        }
        for item in &check.guidance {
            note(format!("    -> {item}"));
        }
        note("");
    }

    if report.healthy() && !report.degraded() {
        note("All checks passed.");
    } else if report.healthy() {
        note("Doctor completed with degraded or unsupported checks.");
    } else {
        note("Doctor found failing checks.");
    }
}

fn render_admin_status_details(status: &ServerStatusResponse) -> Vec<String> {
    let mut details = vec![
        format!("Uptime: {:.0} seconds", status.uptime_seconds),
        format!("Cached replicas: {}", status.cached_replicas),
        format!("Quarantined users: {}", status.quarantined_users),
    ];
    if let Some(value) = status.auth_cache_size.as_deref() {
        details.push(format!("Auth cache: {value}"));
    }
    if let Some(value) = status.config_db.as_deref() {
        details.push(format!("Config DB: {value}"));
    }
    if let Some(value) = status.llm_circuit_breaker.as_deref() {
        details.push(format!("LLM circuit breaker: {value}"));
    }
    details
}

fn render_doctor_user_row(row: &DoctorUserRow) -> String {
    let mut parts = vec![
        format!("{} ({})", row.user.username, row.user.id),
        format!("devices: {}", row.user.device_count),
        format!(
            "last sync: {}",
            row.user
                .last_sync_at
                .as_deref()
                .map(crate::describe_sync_timestamp)
                .unwrap_or_else(|| "never".to_string())
        ),
    ];
    if let Some(stats) = row.stats.as_ref() {
        parts.push(format!(
            "recovery: {}",
            stats.recovery_assessment.status.to_lowercase()
        ));
        parts.push(format!("quarantined: {}", crate::yes_no(stats.quarantined)));
        if let Some(pending) = stats.pending_count {
            parts.push(format!("pending: {pending}"));
        }
        if let Some(task_count) = stats.task_count {
            parts.push(format!("tasks: {task_count}"));
        }
        if !stats.recovery_assessment.notes.is_empty() {
            parts.push(format!(
                "notes: {}",
                stats.recovery_assessment.notes.join("; ")
            ));
        }
    } else {
        parts.push("stats unavailable".to_string());
    }
    parts.join(" | ")
}

fn doctor_icon(status: DoctorCheckStatus) -> &'static str {
    match status {
        DoctorCheckStatus::Ok => "✓",
        DoctorCheckStatus::Warn => "!",
        DoctorCheckStatus::Fail => "✗",
        DoctorCheckStatus::Unsupported => "-",
    }
}

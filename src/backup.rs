use std::io::{self, IsTerminal, Write};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};

use crate::config::ResolvedConfig;
use crate::http::{AdminApiError, AdminClient, BackupRestoreResponse, BackupSummaryResponse};
use crate::output::{print_json, print_table};

#[derive(Args, Debug)]
pub(crate) struct BackupArgs {
    #[arg(long)]
    include_secrets: bool,
    #[command(subcommand)]
    command: Option<BackupSubcommand>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum BackupSubcommand {
    List,
    Restore { timestamp: String },
}

pub(crate) fn run(args: BackupArgs, cfg: &ResolvedConfig, json: bool, yes: bool) -> Result<()> {
    if args.command.is_some() && args.include_secrets {
        bail!("--include-secrets only applies to `cmdock-admin backup` snapshot creation");
    }

    let client = crate::require_client(cfg)?;
    match args.command {
        None => run_backup_create(&client, args.include_secrets, json),
        Some(BackupSubcommand::List) => run_backup_list(&client, json),
        Some(BackupSubcommand::Restore { timestamp }) => {
            run_backup_restore(&client, &timestamp, json, yes)
        }
    }
}

fn run_backup_create(client: &AdminClient, include_secrets: bool, json: bool) -> Result<()> {
    let created = match client.create_backup(include_secrets) {
        Ok(created) => created,
        Err(err) if crate::endpoint_missing(&err) => {
            return crate::unsupported(
                json,
                "backup commands require cmdock/server with the backup endpoints from cmdock/server#57",
            );
        }
        Err(err) => return Err(map_backup_error("create backup", err)),
    };

    if json {
        return print_json(&created);
    }

    println!("Backup created: {}", created.timestamp);
    println!("  Path: {}", created.path);
    println!("  Users: {}", created.users);
    println!("  Size: {}", crate::format_bytes(created.total_size_bytes));
    println!(
        "  Secrets included: {}",
        crate::yes_no(created.secrets_included)
    );
    println!();
    if created.secrets_included {
        println!(
            "Next: copy that staging directory to off-host storage and keep it access-controlled."
        );
    } else {
        println!(
            "Next: copy that staging directory to off-host storage with your existing backup tooling."
        );
    }
    Ok(())
}

fn run_backup_list(client: &AdminClient, json: bool) -> Result<()> {
    let backups = match client.list_backups() {
        Ok(backups) => backups,
        Err(err) if crate::endpoint_missing(&err) => {
            return crate::unsupported(
                json,
                "backup list requires cmdock/server with the backup endpoints from cmdock/server#57",
            );
        }
        Err(err) => return Err(map_backup_error("list backups", err)),
    };

    if json {
        return print_json(&backups);
    }

    if backups.is_empty() {
        println!("No backups found.");
        println!("Next: cmdock-admin backup");
        return Ok(());
    }

    let rows = backups
        .iter()
        .map(|backup| {
            vec![
                backup.timestamp.clone(),
                backup.backup_type.clone(),
                backup.users.to_string(),
                backup
                    .task_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                crate::format_bytes(backup.total_size_bytes),
                crate::yes_no(backup.secrets_included).to_string(),
                backup.server_version.clone(),
            ]
        })
        .collect::<Vec<_>>();
    print_table(
        &[
            "TIMESTAMP",
            "TYPE",
            "USERS",
            "TASKS",
            "SIZE",
            "SECRETS",
            "SERVER",
        ],
        &rows,
    )?;
    println!();
    println!("{} backup(s)", backups.len());
    Ok(())
}

fn run_backup_restore(client: &AdminClient, timestamp: &str, json: bool, yes: bool) -> Result<()> {
    let backup_summary = match client.list_backups() {
        Ok(backups) => backups
            .into_iter()
            .find(|backup| backup.timestamp == timestamp),
        Err(err) if crate::endpoint_missing(&err) => None,
        Err(err) => return Err(map_backup_error("inspect backups before restore", err)),
    };

    if !yes && !confirm_backup_restore(timestamp, backup_summary.as_ref())? {
        println!("Cancelled.");
        return Ok(());
    }

    let restored = match client.restore_backup(timestamp) {
        Ok(restored) => restored,
        Err(err) if crate::endpoint_missing(&err) => {
            return crate::unsupported(
                json,
                "backup restore requires cmdock/server with the backup endpoints from cmdock/server#57",
            );
        }
        Err(err) => return Err(map_backup_error("restore backup", err)),
    };

    if json {
        return print_json(&restored);
    }

    render_backup_restore(&restored);
    Ok(())
}

fn render_backup_restore(restored: &BackupRestoreResponse) {
    println!("Restore complete.");
    println!("  Restored from: {}", restored.restored_from);
    println!("  Pre-restore snapshot: {}", restored.pre_restore_snapshot);
    println!("  Users restored: {}", restored.users_restored);
    println!("  Replicas restored: {}", restored.replicas_restored);
    println!(
        "  Config database: {}",
        if restored.config_database_restored {
            "restored"
        } else {
            "status unavailable"
        }
    );
    for replica in &restored.replicas {
        let task_note = replica
            .task_count
            .map(|count| format!(" ({count} tasks)"))
            .unwrap_or_default();
        println!(
            "  Replica {} ({}): restored{}",
            replica.user_id, replica.username, task_note
        );
    }
    if restored.secrets_restored {
        println!("  Secrets: included");
        println!();
        println!("Next steps:");
        println!("  1. Run: cmdock-admin doctor");
    } else {
        println!("  Secrets: not included - reconfigure admin_token");
        println!();
        println!("Next steps:");
        println!("  1. Reconfigure admin token");
        println!("  2. Run: cmdock-admin doctor");
    }
}

fn confirm_backup_restore(timestamp: &str, backup: Option<&BackupSummaryResponse>) -> Result<bool> {
    if !io::stdin().is_terminal() {
        bail!("backup restore requires confirmation in non-interactive mode; rerun with --yes");
    }

    let mut stderr = io::stderr();
    writeln!(stderr, "Restoring from backup: {timestamp}")?;
    if let Some(backup) = backup {
        writeln!(
            stderr,
            "  Snapshot server version: {}",
            backup.server_version
        )?;
        writeln!(
            stderr,
            "  Compatibility will be verified by the server before restore."
        )?;
        writeln!(
            stderr,
            "  {}, {}, config database",
            format_backup_user_count(backup.users),
            format_backup_task_count(backup.task_count)
        )?;
        writeln!(
            stderr,
            "  Secrets: {}",
            if backup.secrets_included {
                "included"
            } else {
                "not included - admin token will need reconfiguring"
            }
        )?;
    } else {
        writeln!(stderr, "  Snapshot details are unavailable before restore.")?;
        writeln!(
            stderr,
            "  The server will verify compatibility and checksums before applying the snapshot."
        )?;
    }
    writeln!(
        stderr,
        "  A pre-restore snapshot will be created automatically."
    )?;
    writeln!(stderr)?;
    writeln!(stderr, "This will replace ALL current server data.")?;
    write!(stderr, "Continue? [y/N] ")?;
    stderr.flush()?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read input")?;
    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn map_backup_error(action: &str, err: AdminApiError) -> anyhow::Error {
    let message = match err.code() {
        Some("BACKUP_DIR_NOT_CONFIGURED") => {
            "server backup_dir is not configured. Set backup_dir in cmdock/server, restart the server, then retry."
                .to_string()
        }
        Some("BACKUP_DIR_NOT_WRITABLE") => {
            "server backup_dir is not writable. Fix permissions on the backup staging directory, then retry."
                .to_string()
        }
        Some("BACKUP_IN_PROGRESS") | Some("RESTORE_IN_PROGRESS") => {
            "another backup or restore is already running. Wait for it to finish, then retry."
                .to_string()
        }
        Some("SNAPSHOT_NOT_FOUND") => {
            "backup snapshot not found. Run `cmdock-admin backup list` to see valid timestamps."
                .to_string()
        }
        Some("MANIFEST_MISSING") => {
            "backup snapshot is incomplete because manifest.json is missing. Restore from a complete snapshot or clean up the partial one."
                .to_string()
        }
        Some("MANIFEST_INVALID") => {
            "backup snapshot manifest is invalid. Inspect manifest.json or restore from a known-good snapshot."
                .to_string()
        }
        Some("CHECKSUM_MISMATCH") => {
            "backup checksum verification failed. Restore from another snapshot and verify the integrity of your copied backups."
                .to_string()
        }
        Some("VERSION_INCOMPATIBLE") => {
            "backup requires a newer cmdock/server build. Upgrade the server to the required version reported by the API, then retry."
                .to_string()
        }
        Some("SCHEMA_INCOMPATIBLE") => {
            "backup schema is newer than this server supports. Upgrade cmdock/server to a compatible version, then retry."
                .to_string()
        }
        Some("RESTORE_FAILED_ROLLED_BACK") => {
            format!(
                "restore failed and the server rolled back to its pre-restore state. Inspect server logs, review the reported pre-restore snapshot, and rerun `cmdock-admin doctor` before retrying. Server detail: {}",
                err
            )
        }
        _ => format!("{action} failed: {err}"),
    };
    anyhow!(message)
}

fn format_backup_user_count(users: usize) -> String {
    format!("{users} user{}", if users == 1 { "" } else { "s" })
}

fn format_backup_task_count(task_count: Option<u64>) -> String {
    match task_count {
        Some(count) => format!("{count} tasks"),
        None => "unknown task count".to_string(),
    }
}

pub(crate) fn parse_backup_timestamp(value: &str) -> Option<time::OffsetDateTime> {
    let raw = value.strip_prefix("pre-restore-").unwrap_or(value);
    if raw.len() != 19 || !raw.as_bytes().get(10).is_some_and(|ch| *ch == b'T') {
        return None;
    }
    let rfc3339 = format!("{}:{}:{}Z", &raw[..13], &raw[14..16], &raw[17..19]);
    crate::parse_rfc3339(&rfc3339)
}

#[cfg(test)]
mod tests {
    use super::parse_backup_timestamp;
    use crate::format_offset_datetime;

    #[test]
    fn parse_backup_timestamp_handles_standard_snapshot_names() {
        let parsed = parse_backup_timestamp("2026-04-06T10-00-00").unwrap();
        assert_eq!(format_offset_datetime(parsed), "2026-04-06T10:00:00Z");
    }
}

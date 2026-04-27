mod backup;
mod config;
mod connect_config;
mod doctor;
mod http;
mod output;
mod webhook;

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration as StdDuration;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use config::{FileConfig, ResolvedConfig, resolve_config, save_file_config};
use connect_config::{
    BuiltConnectUrl, build_connect_url_with_fallback, build_connect_url_with_fallback_and_scheme,
};
use http::{
    AdminApiError, AdminClient, AdminUserSummary, BootstrapRequest, CreateConnectConfigRequest,
    OperatorDeviceResponse, UserStatsResponse,
};
use output::{note, print_json, print_lines, print_table};
use qr2term::generate_qr_string;
use serde::Serialize;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(
    name = "cmdock-admin",
    version,
    about = "Standalone admin CLI for self-hosted cmdock servers"
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    no_color: bool,
    #[arg(long, global = true)]
    yes: bool,
    #[arg(long, global = true)]
    server: Option<String>,
    #[arg(long, global = true)]
    token: Option<String>,
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Setup(SetupArgs),
    Doctor,
    Backup(backup::BackupArgs),
    Connect(ConnectArgs),
    User(UserArgs),
    Webhook(webhook::WebhookArgs),
}

#[derive(Args, Debug)]
struct SetupArgs {
    #[arg(long)]
    username: Option<String>,
    #[arg(long, default_value = "Taskwarrior")]
    replica_name: String,
    #[arg(long)]
    qr: bool,
}

#[derive(Args, Debug)]
struct ConnectArgs {
    user: Option<String>,
    #[arg(long)]
    taskwarrior: bool,
    #[arg(long)]
    qr: bool,
    /// URL scheme for QR connect URLs (default: cmdock).
    /// Use "cmdock-staging" for staging builds that register a separate URL scheme.
    #[arg(long, default_value = "cmdock", env = "CMDOCK_CONNECT_SCHEME")]
    scheme: String,
    #[command(subcommand)]
    command: Option<ConnectSubcommand>,
}

#[derive(Subcommand, Debug)]
enum ConnectSubcommand {
    List { user: String },
    Revoke { id: String },
}

#[derive(Args, Debug)]
struct UserArgs {
    #[command(subcommand)]
    command: UserSubcommand,
}

#[derive(Subcommand, Debug)]
enum UserSubcommand {
    List,
    Create { name: String },
    Delete { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ConnectMode {
    Taskwarrior,
    Qr,
}

#[derive(Debug, Serialize)]
struct QrConnectJson {
    user_id: String,
    server_url: String,
    token_id: String,
    connect_url: String,
    url_bytes: usize,
    name: Option<String>,
}

fn main() {
    install_rustls_crypto_provider();
    let cli = Cli::parse();
    let exit = match run(cli) {
        Ok(code) => code,
        Err(err) => {
            let code = if err.to_string().contains("configuration") {
                2
            } else {
                1
            };
            let _ = writeln!(io::stderr(), "Error: {err}");
            code
        }
    };
    std::process::exit(exit);
}

fn install_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn run(cli: Cli) -> Result<i32> {
    let cfg = resolve_config(cli.config, cli.server, cli.token, cli.no_color)?;
    match cli.command {
        Commands::Setup(args) => {
            run_setup(args, &cfg, cli.json)?;
            Ok(0)
        }
        Commands::Doctor => Ok(if doctor::run(&cfg, cli.json)? { 0 } else { 1 }),
        Commands::Backup(args) => {
            backup::run(args, &cfg, cli.json, cli.yes)?;
            Ok(0)
        }
        Commands::Connect(args) => {
            run_connect(args, &cfg, cli.json, cli.yes)?;
            Ok(0)
        }
        Commands::User(args) => {
            run_user(args, &cfg, cli.json, cli.yes)?;
            Ok(0)
        }
        Commands::Webhook(args) => {
            webhook::run(args, &cfg, cli.json)?;
            Ok(0)
        }
    }
}

fn run_setup(args: SetupArgs, cfg: &ResolvedConfig, json: bool) -> Result<()> {
    let client = require_client(cfg)?;
    note("Checking server connection...");
    let health = client.health()?;
    note(format!("  ✓ Server reachable at {}", client.base_url()));
    note("  ✓ TLS certificate accepted");
    let status = client.admin_status()?;
    note("  ✓ Admin API authenticated");

    let username = match args.username {
        Some(u) => u,
        None => prompt("Username")?,
    };

    note(format!(
        "Creating or reusing user '{username}' and bootstrap replica..."
    ));
    let bootstrap = client.bootstrap_user_device(&BootstrapRequest {
        device_name: args.replica_name.clone(),
        bootstrap_request_id: Uuid::new_v4().to_string(),
        user_id: None,
        username: Some(username.clone()),
        create_user_if_missing: true,
        public_server_url_override: None,
    })?;

    save_file_config(
        &cfg.config_path,
        &FileConfig {
            server_url: Some(client.base_url().to_string()),
            admin_token: Some(client.token().to_string()),
        },
    )?;

    if json {
        #[derive(Serialize)]
        struct SetupJson<'a> {
            health: &'a str,
            pending_tasks: &'a str,
            user_id: &'a str,
            username: &'a str,
            created_user: bool,
            device_client_id: &'a str,
            taskrc_lines: &'a [String],
            qr: Option<QrConnectJson>,
            config_path: String,
        }
        let qr = if args.qr {
            let rendered = create_qr_connect_payload(
                &client,
                &bootstrap.user_id,
                derived_connect_name(&bootstrap.server_url),
                "cmdock",
            )?;
            Some(QrConnectJson {
                user_id: bootstrap.user_id.clone(),
                server_url: rendered.server_url,
                token_id: rendered.token_id,
                connect_url: rendered.connect.url,
                url_bytes: rendered.connect.byte_len,
                name: rendered.connect.included_name,
            })
        } else {
            None
        };
        return print_json(&SetupJson {
            health: &health.status,
            pending_tasks: &health.pending_tasks,
            user_id: &bootstrap.user_id,
            username: &bootstrap.username,
            created_user: bootstrap.created_user,
            device_client_id: &bootstrap.device_client_id,
            taskrc_lines: &bootstrap.taskrc_lines,
            qr,
            config_path: cfg.config_path.display().to_string(),
        });
    }

    note(format!(
        "  ✓ Server status: {}, uptime {:.0}s",
        status.status, status.uptime_seconds
    ));
    if bootstrap.created_user {
        note(format!("  ✓ User '{}' created", bootstrap.username));
    } else {
        note(format!("  ✓ Using existing user '{}'", bootstrap.username));
    }
    note("  ✓ Replica credential created");
    note("");
    note("Add these lines to your ~/.taskrc:");
    print_lines(&bootstrap.taskrc_lines)?;
    note("");
    if args.qr {
        let rendered = create_qr_connect_payload(
            &client,
            &bootstrap.user_id,
            derived_connect_name(&bootstrap.server_url),
            "cmdock",
        )?;
        note(format!(
            "  ✓ QR connect token issued ({}, {} bytes)",
            rendered.token_id, rendered.connect.byte_len
        ));
        if rendered.connect.included_name.is_none() {
            note("  ! Omitted optional display name to stay within the 250-byte QR budget");
        }
        note("");
        println!("{}", rendered.connect.url);
        println!();
        println!("{}", rendered.qr_string);
    }
    note(format!(
        "Setup complete. Config saved to {}",
        cfg.config_path.display()
    ));
    note("Run `cmdock-admin doctor` to verify server health.");
    Ok(())
}

fn run_connect(args: ConnectArgs, cfg: &ResolvedConfig, json: bool, yes: bool) -> Result<()> {
    if let Some(command) = args.command {
        return match command {
            ConnectSubcommand::List { user } => {
                let client = require_client(cfg)?;
                run_connect_list(&client, &user, json)
            }
            ConnectSubcommand::Revoke { id } => {
                let client = require_client(cfg)?;
                run_connect_revoke(&client, &id, json, yes)
            }
        };
    }

    let mode = resolve_connect_mode(args.taskwarrior, args.qr)?;
    let client = require_client(cfg)?;
    let user = args
        .user
        .ok_or_else(|| anyhow!("connect requires a user name"))?;

    match mode {
        ConnectMode::Taskwarrior => {
            let bootstrap = client.bootstrap_user_device(&BootstrapRequest {
                device_name: "Taskwarrior".to_string(),
                bootstrap_request_id: Uuid::new_v4().to_string(),
                user_id: None,
                username: Some(user),
                create_user_if_missing: false,
                public_server_url_override: None,
            })?;

            if json {
                return print_json(&bootstrap);
            }
            print_lines(&bootstrap.taskrc_lines)?;
            note("");
            note("Replica ready. Next: add those lines to ~/.taskrc and run `task sync`.");
            Ok(())
        }
        ConnectMode::Qr => {
            let user_id = resolve_user_id_for_qr(&client, &user)?;
            let rendered = create_qr_connect_payload(
                &client,
                &user_id,
                derived_connect_name(client.base_url()),
                &args.scheme,
            )?;
            if json {
                return print_json(&QrConnectJson {
                    user_id,
                    server_url: rendered.server_url,
                    token_id: rendered.token_id,
                    connect_url: rendered.connect.url,
                    url_bytes: rendered.connect.byte_len,
                    name: rendered.connect.included_name,
                });
            }
            note(format!(
                "QR connect token issued for {user_id} ({}, {} bytes)",
                rendered.token_id, rendered.connect.byte_len
            ));
            if rendered.connect.included_name.is_none() {
                note("Optional display name omitted to stay within the 250-byte QR budget.");
            }
            println!("{}", rendered.connect.url);
            println!();
            println!("{}", rendered.qr_string);
            Ok(())
        }
    }
}

fn run_connect_list(client: &AdminClient, user: &str, json: bool) -> Result<()> {
    let users = match client.list_users() {
        Ok(users) => users,
        Err(err) if endpoint_missing(&err) => {
            return unsupported(
                json,
                "connect list requires cmdock/server with GET /admin/users (cmdock/server#54)",
            );
        }
        Err(err) => return Err(err.into()),
    };
    let user = find_user_by_name(&users, user)?;
    let devices = match client.list_user_devices(&user.id) {
        Ok(devices) => devices,
        Err(err) if endpoint_missing(&err) => {
            return unsupported(
                json,
                "connect list requires cmdock/server with GET /admin/user/{id}/devices",
            );
        }
        Err(err) => return Err(err.into()),
    };

    if json {
        return print_json(&devices);
    }

    if devices.is_empty() {
        println!("No active connections found for '{}'.", user.username);
        println!(
            "Next: run `cmdock-admin connect {} --taskwarrior` or `cmdock-admin connect {} --qr`.",
            user.username, user.username
        );
        return Ok(());
    }

    let rows = devices
        .iter()
        .map(|device| {
            vec![
                device.name.clone(),
                device.client_id.clone(),
                device.status.clone(),
                device
                    .last_sync_at
                    .as_deref()
                    .map(describe_sync_timestamp)
                    .unwrap_or_else(|| "never".to_string()),
                device.registered_at.clone(),
            ]
        })
        .collect::<Vec<_>>();
    print_table(
        &["NAME", "CLIENT ID", "STATUS", "LAST SYNC", "REGISTERED"],
        &rows,
    )?;
    println!();
    println!("{} connection(s) for '{}'", devices.len(), user.username);
    Ok(())
}

fn run_connect_revoke(client: &AdminClient, raw: &str, json: bool, yes: bool) -> Result<()> {
    let users = match client.list_users() {
        Ok(users) => users,
        Err(err) if endpoint_missing(&err) => {
            return unsupported(
                json,
                "connect revoke requires cmdock/server with GET /admin/users (cmdock/server#54)",
            );
        }
        Err(err) => return Err(err.into()),
    };
    let (user, device) = resolve_device_target(client, &users, raw)?;

    if !yes && !confirm_connect_revoke(user, &device)? {
        println!("Cancelled.");
        return Ok(());
    }

    match client.revoke_user_device(&user.id, &device.client_id) {
        Ok(()) => {}
        Err(err) if endpoint_missing(&err) => {
            return unsupported(
                json,
                "connect revoke requires cmdock/server with POST /admin/user/{id}/devices/{client_id}/revoke",
            );
        }
        Err(err) => return Err(err.into()),
    }

    if json {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct RevokeResult<'a> {
            revoked: bool,
            user_id: &'a str,
            username: &'a str,
            client_id: &'a str,
            name: &'a str,
        }
        return print_json(&RevokeResult {
            revoked: true,
            user_id: &user.id,
            username: &user.username,
            client_id: &device.client_id,
            name: &device.name,
        });
    }

    println!(
        "Revoked connection '{}' for user '{}'.",
        device.name, user.username
    );
    println!("  Client ID: {}", device.client_id);
    println!(
        "  Last sync: {}",
        device
            .last_sync_at
            .as_deref()
            .map(describe_sync_timestamp)
            .unwrap_or_else(|| "never".to_string())
    );
    Ok(())
}

fn run_user(args: UserArgs, cfg: &ResolvedConfig, json: bool, yes: bool) -> Result<()> {
    let client = require_client(cfg)?;

    match args.command {
        UserSubcommand::List => run_user_list(&client, json),
        UserSubcommand::Create { name } => run_user_create(&client, &name, json),
        UserSubcommand::Delete { name } => run_user_delete(&client, &name, json, yes),
    }
}

fn run_user_list(client: &AdminClient, json: bool) -> Result<()> {
    let users = match client.list_users() {
        Ok(users) => users,
        Err(err) if endpoint_missing(&err) => {
            return unsupported(
                json,
                "user list requires cmdock/server with GET /admin/users (cmdock/server#54)",
            );
        }
        Err(err) => return Err(err.into()),
    };

    if json {
        return print_json(&users);
    }

    if users.is_empty() {
        println!("No users found.");
        println!("Next: cmdock-admin user create <name>");
        return Ok(());
    }

    let rows = users
        .iter()
        .map(|user| {
            vec![
                user.username.clone(),
                user.id.clone(),
                user.device_count.to_string(),
                user.last_sync_at
                    .clone()
                    .unwrap_or_else(|| "never".to_string()),
                user.created_at.clone(),
            ]
        })
        .collect::<Vec<_>>();
    print_table(
        &["USERNAME", "ID", "DEVICES", "LAST SYNC", "CREATED"],
        &rows,
    )?;
    println!();
    println!("{} user(s)", users.len());
    Ok(())
}

fn run_user_create(client: &AdminClient, name: &str, json: bool) -> Result<()> {
    match client.list_users() {
        Ok(users) => {
            if users.iter().any(|user| user.username == name) {
                bail!(
                    "user '{name}' already exists. Next: run `cmdock-admin connect {name} --taskwarrior` or `cmdock-admin connect {name} --qr`."
                );
            }
        }
        Err(err) if endpoint_missing(&err) => {}
        Err(err) => return Err(err.into()),
    }

    let bootstrap = client.bootstrap_user_device(&BootstrapRequest {
        device_name: "Taskwarrior".to_string(),
        bootstrap_request_id: Uuid::new_v4().to_string(),
        user_id: None,
        username: Some(name.to_string()),
        create_user_if_missing: true,
        public_server_url_override: None,
    })?;

    if json {
        return print_json(&bootstrap);
    }

    println!(
        "User '{}' {}",
        bootstrap.username,
        if bootstrap.created_user {
            "created."
        } else {
            "already existed; bootstrap issued a new sync credential."
        }
    );
    println!("  ID: {}", bootstrap.user_id);
    println!(
        "  Bootstrap device client ID: {}",
        bootstrap.device_client_id
    );
    println!();
    println!(
        "Next: run `cmdock-admin connect {} --taskwarrior` or `cmdock-admin connect {} --qr` to configure a client.",
        bootstrap.username, bootstrap.username
    );
    Ok(())
}

fn run_user_delete(client: &AdminClient, name: &str, json: bool, yes: bool) -> Result<()> {
    let users = match client.list_users() {
        Ok(users) => users,
        Err(err) if endpoint_missing(&err) => {
            return unsupported(
                json,
                "user delete requires cmdock/server with GET /admin/users (cmdock/server#54)",
            );
        }
        Err(err) => return Err(err.into()),
    };
    let user = find_user_by_name(&users, name)?;
    let stats = client.user_stats(&user.id).ok();

    if !yes && !confirm_user_delete(user, stats.as_ref())? {
        println!("Cancelled.");
        return Ok(());
    }

    let deleted = match client.delete_user(&user.id) {
        Ok(deleted) => deleted,
        Err(err) if endpoint_missing(&err) => {
            return unsupported(
                json,
                "user delete requires cmdock/server with DELETE /admin/user/{id} (cmdock/server#55)",
            );
        }
        Err(err) => return Err(err.into()),
    };

    if json {
        return print_json(&deleted);
    }

    println!("Deleted user '{}' ({})", deleted.username, deleted.user_id);
    println!(
        "  Active connections removed: {}",
        deleted.device_count_removed
    );
    println!(
        "  Replica data removed: {}",
        yes_no(deleted.replica_dir_removed)
    );
    Ok(())
}

fn require_client(cfg: &ResolvedConfig) -> Result<AdminClient> {
    let server_url = cfg.server_url.clone().ok_or_else(|| {
        anyhow!("configuration error: missing server URL. Pass --server or run cmdock-admin setup")
    })?;
    let token = cfg.admin_token.clone().ok_or_else(|| {
        anyhow!("configuration error: missing admin token. Pass --token or set CMDOCK_ADMIN_TOKEN")
    })?;
    Ok(AdminClient::new(server_url, token)?)
}

fn resolve_connect_mode(taskwarrior: bool, qr: bool) -> Result<ConnectMode> {
    match (taskwarrior, qr) {
        (true, false) => Ok(ConnectMode::Taskwarrior),
        (false, true) => Ok(ConnectMode::Qr),
        (false, false) => prompt_connect_mode(),
        (true, true) => bail!("choose only one of --taskwarrior or --qr"),
    }
}

struct RenderedQrConnect {
    server_url: String,
    token_id: String,
    connect: BuiltConnectUrl,
    qr_string: String,
}

fn create_qr_connect_payload(
    client: &AdminClient,
    user_id: &str,
    preferred_name: Option<String>,
    scheme: &str,
) -> Result<RenderedQrConnect> {
    let issued = client.create_connect_config(
        user_id,
        &CreateConnectConfigRequest {
            name: preferred_name.clone(),
        },
    )?;
    let connect = build_connect_url_with_fallback_and_scheme(
        &issued.server_url,
        preferred_name,
        issued.credential,
        Some(issued.token_id.clone()),
        scheme,
    )?;
    let qr_string =
        generate_qr_string(&connect.url).map_err(|err| anyhow!("failed to render QR: {err}"))?;
    Ok(RenderedQrConnect {
        server_url: issued.server_url,
        token_id: issued.token_id,
        connect,
        qr_string,
    })
}

fn derived_connect_name(server_url: &str) -> Option<String> {
    reqwest::Url::parse(server_url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_string()))
}

fn resolve_user_id_for_qr(client: &AdminClient, raw: &str) -> Result<String> {
    if Uuid::parse_str(raw).is_ok() {
        return Ok(raw.to_string());
    }

    let users = match client.list_users() {
        Ok(users) => users,
        Err(err) if endpoint_missing(&err) => {
            bail!(
                "QR connect by username requires GET /admin/users. Upgrade cmdock/server to include cmdock/server#54 or pass the user ID explicitly."
            );
        }
        Err(err) => return Err(err.into()),
    };

    let user = find_user_by_name(&users, raw)?;
    Ok(user.id.clone())
}

fn find_user_by_name<'a>(
    users: &'a [AdminUserSummary],
    name: &str,
) -> Result<&'a AdminUserSummary> {
    users
        .iter()
        .find(|user| user.username == name)
        .ok_or_else(|| {
            anyhow!("unknown user '{name}'. Run `cmdock-admin user list` to see valid usernames.")
        })
}

fn resolve_device_target<'a>(
    client: &AdminClient,
    users: &'a [AdminUserSummary],
    raw: &str,
) -> Result<(&'a AdminUserSummary, OperatorDeviceResponse)> {
    if let Some((username, device_name)) = raw.split_once(':') {
        let user = find_user_by_name(users, username)?;
        let devices = client.list_user_devices(&user.id)?;
        let device = find_device_by_name(&devices, device_name)?;
        return Ok((user, device.clone()));
    }

    if Uuid::parse_str(raw).is_ok() {
        let matches = users
            .iter()
            .filter_map(|user| {
                let devices = client.list_user_devices(&user.id).ok()?;
                let device = devices.into_iter().find(|device| device.client_id == raw)?;
                Some((user, device))
            })
            .collect::<Vec<_>>();

        return match matches.as_slice() {
            [(user, device)] => Ok((user, device.clone())),
            [] => bail!(
                "unknown device '{raw}'. Use `cmdock-admin connect list <user>` to inspect active connections."
            ),
            _ => bail!(
                "device identifier '{raw}' matched more than one user unexpectedly; rerun with '<username>:<device-name>'."
            ),
        };
    }

    bail!("invalid device target '{raw}'. Use a client ID or '<username>:<device-name>'.");
}

fn find_device_by_name<'a>(
    devices: &'a [OperatorDeviceResponse],
    name: &str,
) -> Result<&'a OperatorDeviceResponse> {
    devices
        .iter()
        .find(|device| device.name == name)
        .ok_or_else(|| anyhow!("unknown device '{name}' for that user"))
}

fn confirm_user_delete(user: &AdminUserSummary, stats: Option<&UserStatsResponse>) -> Result<bool> {
    if !io::stdin().is_terminal() {
        bail!("user delete requires confirmation in non-interactive mode; rerun with --yes");
    }

    let mut stderr = io::stderr();
    writeln!(
        stderr,
        "This will delete user '{}' ({}) and:",
        user.username, user.id
    )?;
    writeln!(
        stderr,
        "  - Revoke {} active connection(s)",
        user.device_count
    )?;
    if let Some(stats) = stats {
        if stats.replica_dir_exists {
            writeln!(stderr, "  - Remove replica data from the server")?;
        } else {
            writeln!(stderr, "  - No replica directory is currently recorded")?;
        }
        if let Some(task_count) = stats.task_count {
            writeln!(stderr, "  - Delete {} cached task(s)", task_count)?;
        } else {
            writeln!(stderr, "  - Delete all task data for this user")?;
        }
        if stats.quarantined {
            writeln!(stderr, "  - Remove the current quarantine/offline state")?;
        }
    } else {
        writeln!(stderr, "  - Delete all task data for this user")?;
    }
    writeln!(stderr)?;
    write!(stderr, "This cannot be undone. Continue? [y/N] ")?;
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

fn confirm_connect_revoke(
    user: &AdminUserSummary,
    device: &OperatorDeviceResponse,
) -> Result<bool> {
    if !io::stdin().is_terminal() {
        bail!("connect revoke requires confirmation in non-interactive mode; rerun with --yes");
    }

    let mut stderr = io::stderr();
    writeln!(
        stderr,
        "Revoke connection '{}' for user '{}'?",
        device.name, user.username
    )?;
    writeln!(stderr, "  Client ID: {}", device.client_id)?;
    writeln!(stderr, "  Status: {}", device.status)?;
    writeln!(
        stderr,
        "  Last sync: {}",
        device
            .last_sync_at
            .as_deref()
            .map(describe_sync_timestamp)
            .unwrap_or_else(|| "never".to_string())
    )?;
    writeln!(stderr, "  This device will no longer be able to sync.")?;
    write!(stderr, "[y/N] ")?;
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

fn prompt_connect_mode() -> Result<ConnectMode> {
    if !io::stdin().is_terminal() {
        bail!("connect needs either --taskwarrior or --qr in non-interactive mode");
    }
    note("Select connection format:");
    note("  1. Taskwarrior (.taskrc lines)");
    note("  2. Native app QR code");
    match prompt("Format [1/2]")?.as_str() {
        "1" => Ok(ConnectMode::Taskwarrior),
        "2" => Ok(ConnectMode::Qr),
        _ => bail!("invalid format selection; choose 1 or 2"),
    }
}

fn prompt(label: &str) -> Result<String> {
    let mut stderr = io::stderr();
    write!(stderr, "{label}: ")?;
    stderr.flush()?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read input")?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("{label} cannot be empty");
    }
    Ok(trimmed.to_string())
}

fn unsupported(json: bool, message: &str) -> Result<()> {
    if json {
        #[derive(Serialize)]
        struct Unsupported<'a> {
            supported: bool,
            reason: &'a str,
        }
        print_json(&Unsupported {
            supported: false,
            reason: message,
        })?;
        return Ok(());
    }
    bail!("{message}");
}

fn endpoint_missing(err: &AdminApiError) -> bool {
    err.is_not_found() && err.to_string().contains("endpoint not found")
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn format_offset_datetime(value: OffsetDateTime) -> String {
    value
        .format(&Rfc3339)
        .unwrap_or_else(|_| value.unix_timestamp().to_string())
}

fn describe_optional_duration(duration: Option<StdDuration>) -> String {
    duration
        .map(describe_duration)
        .unwrap_or_else(|| "expired".to_string())
}

fn describe_duration(duration: StdDuration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        "expires in under a minute".to_string()
    } else if seconds < 60 * 60 {
        format!("expires in {} minute(s)", seconds / 60)
    } else if seconds < 24 * 60 * 60 {
        format!("expires in {} hour(s)", seconds / (60 * 60))
    } else {
        format!("expires in {} day(s)", seconds / (24 * 60 * 60))
    }
}

fn parse_rfc3339(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).ok()
}

fn synced_recently(value: Option<&str>) -> bool {
    value
        .and_then(parse_rfc3339)
        .map(|timestamp| OffsetDateTime::now_utc() - timestamp <= Duration::hours(24))
        .unwrap_or(false)
}

fn describe_sync_timestamp(value: &str) -> String {
    parse_rfc3339(value)
        .map(|timestamp| describe_age(timestamp, OffsetDateTime::now_utc()))
        .unwrap_or_else(|| value.to_string())
}

fn describe_age(timestamp: OffsetDateTime, now: OffsetDateTime) -> String {
    if timestamp > now {
        return format!("at {}", format_offset_datetime(timestamp));
    }

    let delta = now - timestamp;
    if delta < Duration::minutes(1) {
        "just now".to_string()
    } else if delta < Duration::hours(1) {
        format!("{} minute(s) ago", delta.whole_minutes())
    } else if delta < Duration::days(1) {
        format!("{} hour(s) ago", delta.whole_hours())
    } else {
        format!("{} day(s) ago", delta.whole_days())
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    let bytes_f = bytes as f64;
    if bytes_f >= GIB {
        format!("{:.1} GiB", bytes_f / GIB)
    } else if bytes_f >= MIB {
        format!("{:.1} MiB", bytes_f / MIB)
    } else if bytes_f >= KIB {
        format!("{:.1} KiB", bytes_f / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_connect_name_uses_host() {
        assert_eq!(
            derived_connect_name("https://tasks.example.com"),
            Some("tasks.example.com".to_string())
        );
    }

    #[test]
    fn find_user_by_name_matches_exact_username() {
        let users = vec![AdminUserSummary {
            id: "u1".to_string(),
            username: "alice".to_string(),
            created_at: "2026-04-01T00:00:00Z".to_string(),
            device_count: 1,
            last_sync_at: None,
        }];
        assert_eq!(find_user_by_name(&users, "alice").unwrap().id, "u1");
    }

    #[test]
    fn find_device_by_name_matches_exact_name() {
        let devices = vec![OperatorDeviceResponse {
            client_id: "c1".to_string(),
            name: "simons-phone".to_string(),
            registered_at: "2026-04-01T00:00:00Z".to_string(),
            last_sync_at: None,
            last_sync_ip: None,
            status: "active".to_string(),
            bootstrap_request_id: None,
            bootstrap_status: None,
            bootstrap_expires_at: None,
        }];
        assert_eq!(
            find_device_by_name(&devices, "simons-phone")
                .unwrap()
                .client_id,
            "c1"
        );
    }

    #[test]
    fn find_device_by_name_reports_unknown_device() {
        let devices = vec![OperatorDeviceResponse {
            client_id: "c1".to_string(),
            name: "simons-phone".to_string(),
            registered_at: "2026-04-01T00:00:00Z".to_string(),
            last_sync_at: None,
            last_sync_ip: None,
            status: "active".to_string(),
            bootstrap_request_id: None,
            bootstrap_status: None,
            bootstrap_expires_at: None,
        }];
        let err = find_device_by_name(&devices, "simons-laptop").unwrap_err();
        assert!(err.to_string().contains("unknown device"));
    }

    #[test]
    fn describe_age_uses_human_units() {
        let now = OffsetDateTime::UNIX_EPOCH + Duration::hours(10);
        let earlier = now - Duration::minutes(90);
        assert_eq!(describe_age(earlier, now), "1 hour(s) ago");
    }
}

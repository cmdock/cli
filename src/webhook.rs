use anyhow::Result;
use clap::{Args, Subcommand};

use crate::config::ResolvedConfig;
use crate::http::{
    AdminApiError, AdminClient, AdminWebhookDelivery, CreateAdminWebhookRequest,
    UpdateAdminWebhookRequest,
};
use crate::output::{print_json, print_table};

#[derive(Args, Debug)]
pub(crate) struct WebhookArgs {
    #[command(subcommand)]
    command: WebhookSubcommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum WebhookSubcommand {
    List,
    Create {
        #[arg(long)]
        url: String,
        #[arg(long)]
        secret: String,
        #[arg(long, value_delimiter = ',', num_args = 1..)]
        events: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        modified_fields: Vec<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        disabled: bool,
    },
    Delete {
        id: String,
    },
    Test {
        id: String,
    },
    Deliveries {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
}

pub(crate) fn run(args: WebhookArgs, cfg: &ResolvedConfig, json: bool) -> Result<()> {
    let client = crate::require_client(cfg)?;
    match args.command {
        WebhookSubcommand::List => run_list(&client, json),
        WebhookSubcommand::Create {
            url,
            secret,
            events,
            modified_fields,
            name,
            disabled,
        } => run_create(
            &client,
            &url,
            &secret,
            events,
            if modified_fields.is_empty() {
                None
            } else {
                Some(modified_fields)
            },
            name,
            disabled,
            json,
        ),
        WebhookSubcommand::Delete { id } => run_delete(&client, &id, json),
        WebhookSubcommand::Test { id } => run_test(&client, &id, json),
        WebhookSubcommand::Deliveries { id } => run_deliveries(&client, &id, json),
        WebhookSubcommand::Enable { id } => run_enable_disable(&client, &id, true, json),
        WebhookSubcommand::Disable { id } => run_enable_disable(&client, &id, false, json),
    }
}

fn run_list(client: &AdminClient, json: bool) -> Result<()> {
    let webhooks = match client.list_admin_webhooks() {
        Ok(webhooks) => webhooks,
        Err(err) if crate::endpoint_missing(&err) => {
            return crate::unsupported(
                json,
                "webhook commands require cmdock/server with the admin webhook endpoints from cmdock/server#65",
            );
        }
        Err(err) => return Err(map_webhook_error("list webhooks", err)),
    };

    if json {
        return print_json(&webhooks);
    }

    if webhooks.is_empty() {
        println!("No admin webhooks configured.");
        println!(
            "Next: cmdock-admin webhook create --url https://hooks.example.invalid/cmdock --secret <secret> --events task.created"
        );
        return Ok(());
    }

    let rows = webhooks
        .iter()
        .map(|webhook| {
            vec![
                webhook.id.clone(),
                webhook.name.clone().unwrap_or_else(|| "-".to_string()),
                webhook.url.clone(),
                webhook.events.join(","),
                crate::yes_no(webhook.enabled).to_string(),
                webhook.consecutive_failures.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    print_table(
        &["ID", "NAME", "URL", "EVENTS", "ENABLED", "FAILURES"],
        &rows,
    )?;
    println!();
    println!("{} webhook(s)", webhooks.len());
    Ok(())
}

fn run_create(
    client: &AdminClient,
    url: &str,
    secret: &str,
    events: Vec<String>,
    modified_fields: Option<Vec<String>>,
    name: Option<String>,
    disabled: bool,
    json: bool,
) -> Result<()> {
    let created = match client.create_admin_webhook(&CreateAdminWebhookRequest {
        url: url.to_string(),
        secret: secret.to_string(),
        events,
        modified_fields,
        name,
    }) {
        Ok(created) => created,
        Err(err) if crate::endpoint_missing(&err) => {
            return crate::unsupported(
                json,
                "webhook commands require cmdock/server with the admin webhook endpoints from cmdock/server#65",
            );
        }
        Err(err) => return Err(map_webhook_error("create webhook", err)),
    };

    let created = if disabled {
        match client
            .update_admin_webhook(&created.id, &UpdateAdminWebhookRequest { enabled: false })
        {
            Ok(updated) => updated,
            Err(err) => return Err(map_webhook_error("disable webhook after create", err)),
        }
    } else {
        created
    };

    if json {
        return print_json(&created);
    }

    println!("Webhook created: {}", created.id);
    println!("  URL: {}", created.url);
    println!("  Events: {}", created.events.join(", "));
    println!("  Enabled: {}", crate::yes_no(created.enabled));
    if let Some(name) = &created.name {
        println!("  Name: {name}");
    }
    println!();
    println!("Next: cmdock-admin webhook test {}", created.id);
    Ok(())
}

fn run_delete(client: &AdminClient, id: &str, json: bool) -> Result<()> {
    match client.delete_admin_webhook(id) {
        Ok(()) => {}
        Err(err) if crate::endpoint_missing(&err) => {
            return crate::unsupported(
                json,
                "webhook commands require cmdock/server with the admin webhook endpoints from cmdock/server#65",
            );
        }
        Err(err) => return Err(map_webhook_error("delete webhook", err)),
    }

    if json {
        return print_json(&serde_json::json!({
            "deleted": true,
            "id": id,
        }));
    }

    println!("Deleted webhook {id}");
    Ok(())
}

fn run_test(client: &AdminClient, id: &str, json: bool) -> Result<()> {
    let response = match client.test_admin_webhook(id) {
        Ok(response) => response,
        Err(err) if crate::endpoint_missing(&err) => {
            return crate::unsupported(
                json,
                "webhook commands require cmdock/server with the admin webhook endpoints from cmdock/server#65",
            );
        }
        Err(err) => return Err(map_webhook_error("test webhook", err)),
    };

    if json {
        return print_json(&response);
    }

    println!("Test delivery: {}", response.delivery.delivery_id);
    println!("  Event: {}", response.delivery.event);
    println!("  Status: {}", response.delivery.status);
    if let Some(status) = response.delivery.response_status {
        println!("  Response status: {status}");
    }
    if let Some(reason) = &response.delivery.failure_reason {
        println!("  Failure: {reason}");
    }
    Ok(())
}

fn run_deliveries(client: &AdminClient, id: &str, json: bool) -> Result<()> {
    let detail = match client.get_admin_webhook(id) {
        Ok(detail) => detail,
        Err(err) if crate::endpoint_missing(&err) => {
            return crate::unsupported(
                json,
                "webhook commands require cmdock/server with the admin webhook endpoints from cmdock/server#65",
            );
        }
        Err(err) => return Err(map_webhook_error("inspect webhook deliveries", err)),
    };

    if json {
        return print_json(&detail.deliveries);
    }

    if detail.deliveries.is_empty() {
        println!("No deliveries recorded for webhook {}.", detail.webhook.id);
        return Ok(());
    }

    let rows = detail
        .deliveries
        .iter()
        .map(delivery_row)
        .collect::<Vec<_>>();
    print_table(
        &[
            "DELIVERY",
            "EVENT",
            "STATUS",
            "ATTEMPT",
            "HTTP",
            "TIMESTAMP",
        ],
        &rows,
    )?;
    Ok(())
}

fn run_enable_disable(client: &AdminClient, id: &str, enabled: bool, json: bool) -> Result<()> {
    let updated = match client.update_admin_webhook(id, &UpdateAdminWebhookRequest { enabled }) {
        Ok(updated) => updated,
        Err(err) if crate::endpoint_missing(&err) => {
            return crate::unsupported(
                json,
                "webhook commands require cmdock/server with the admin webhook endpoints from cmdock/server#65",
            );
        }
        Err(err) => return Err(map_webhook_error("update webhook", err)),
    };

    if json {
        return print_json(&updated);
    }

    println!(
        "{} webhook {}",
        if enabled { "Enabled" } else { "Disabled" },
        updated.id
    );
    Ok(())
}

fn delivery_row(delivery: &AdminWebhookDelivery) -> Vec<String> {
    vec![
        delivery.delivery_id.clone(),
        delivery.event.clone(),
        delivery.status.clone(),
        delivery.attempt.to_string(),
        delivery
            .response_status
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string()),
        delivery.timestamp.clone(),
    ]
}

fn map_webhook_error(action: &str, err: AdminApiError) -> anyhow::Error {
    if let Some(code) = err.code() {
        return anyhow::anyhow!("{action} failed: {code}: {err}");
    }
    anyhow::anyhow!("{action} failed: {err}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_row_formats_missing_status() {
        let row = delivery_row(&AdminWebhookDelivery {
            delivery_id: "del_1".to_string(),
            event_id: "evt_1".to_string(),
            event: "task.created".to_string(),
            timestamp: "2026-04-07T12:00:00Z".to_string(),
            status: "failed".to_string(),
            response_status: None,
            attempt: 4,
            failure_reason: Some("boom".to_string()),
        });
        assert_eq!(row[4], "-");
    }
}

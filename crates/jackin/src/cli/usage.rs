use anyhow::Result;
use clap::{Args, Subcommand};
use jackin_protocol::control::AccountUsageSnapshotView;
use serde::Serialize;

use crate::cli::format::{OutputEnvelope, OutputFormat};
use crate::cli::{BANNER, HELP_STYLES};
use crate::instance::{InstanceIndex, InstanceStatus};
use crate::paths::JackinPaths;
use crate::runtime::snapshot;

mod store;

/// `jackin usage` — read cached Capsule usage/quota snapshots.
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Read cached usage and quota data from a running Capsule daemon",
    long_about = "Read cached usage and quota data from a running Capsule daemon.\n\n\
        This command never polls providers itself. It talks to the selected\n\
        instance's jackin-capsule daemon and renders the daemon-cached account\n\
        snapshots that Capsule uses for the status bar and overlay.\n\n\
        Use `jackin usage cache accounts` to read the explicit host-global\n\
        account cache seeded by `accounts --sync-host-cache`."
)]
pub struct UsageArgs {
    /// Container name, short instance id, or `cache`
    pub instance: String,
    #[command(subcommand)]
    pub scope: UsageScope,
    /// Output format
    #[arg(long, global = true, value_name = "FORMAT", default_value = "human")]
    pub format: String,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageScope {
    /// Show cached provider account/quota buckets
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Accounts(UsageAccountsArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageAccountsArgs {
    /// Also upsert returned rows into ~/.jackin/data/daemon/accounts.db.
    ///
    /// This is an explicit host-side write for seeding the host-global usage
    /// cache before a long-running host daemon owns account refresh.
    #[arg(long)]
    pub sync_host_cache: bool,
}

#[derive(Debug, Serialize)]
struct UsageAccountsOutput {
    container: String,
    accounts: Vec<AccountUsageSnapshotView>,
    synced_host_cache_path: Option<String>,
    host_cache_path: Option<String>,
}

impl UsageArgs {
    fn output_format(&self) -> OutputFormat {
        OutputFormat::parse(&self.format)
    }
}

pub async fn run(args: &UsageArgs, paths: &JackinPaths) -> Result<()> {
    if args.instance == "cache" {
        return run_cache(args, paths).await;
    }
    let target = resolve_usage_target(paths, &args.instance)?;
    match &args.scope {
        UsageScope::Accounts(scope_args) => run_accounts(args, paths, &target, scope_args).await,
    }
}

async fn run_cache(args: &UsageArgs, paths: &JackinPaths) -> Result<()> {
    match &args.scope {
        UsageScope::Accounts(scope_args) => {
            if scope_args.sync_host_cache {
                anyhow::bail!(
                    "`jackin usage cache accounts --sync-host-cache` is invalid; cache reads never write host state"
                );
            }
            let (path, accounts) = store::read_accounts(paths).await?;
            if args.output_format() == OutputFormat::Json {
                let envelope = OutputEnvelope::v1(UsageAccountsOutput {
                    container: "host-cache".to_owned(),
                    accounts,
                    synced_host_cache_path: None,
                    host_cache_path: Some(path.display().to_string()),
                });
                println!("{}", serde_json::to_string_pretty(&envelope)?);
                return Ok(());
            }
            print!("{BANNER}");
            println!("usage accounts for host cache\n");
            println!("  cache {}", path.display());
            render_accounts_table(&accounts);
            Ok(())
        }
    }
}

async fn run_accounts(
    args: &UsageArgs,
    paths: &JackinPaths,
    target: &UsageTarget,
    scope_args: &UsageAccountsArgs,
) -> Result<()> {
    let accounts = snapshot::fetch_usage_accounts(paths, &target.container)?.unwrap_or_default();
    let synced_host_cache_path = if scope_args.sync_host_cache {
        let path = store::upsert_accounts(paths, &accounts).await?;
        Some(path)
    } else {
        None
    };

    if args.output_format() == OutputFormat::Json {
        let envelope = OutputEnvelope::v1(UsageAccountsOutput {
            container: target.container.clone(),
            accounts,
            synced_host_cache_path: synced_host_cache_path
                .as_ref()
                .map(|path| path.display().to_string()),
            host_cache_path: None,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    print!("{BANNER}");
    println!("usage accounts for {}\n", target.display_label());
    if accounts.is_empty() {
        println!("  no cached usage accounts");
        if let Some(path) = synced_host_cache_path {
            println!("  synced host cache {}", path.display());
        }
        return Ok(());
    }

    render_accounts_table(&accounts);
    if let Some(path) = synced_host_cache_path {
        println!("\n  synced host cache {}", path.display());
    }
    Ok(())
}

fn render_accounts_table(accounts: &[AccountUsageSnapshotView]) {
    if accounts.is_empty() {
        println!("  no cached usage accounts");
        return;
    }
    println!(
        "  {:<12}  {:<22}  {:<12}  {:<12}  {:<18}  source",
        "provider", "account", "window", "status", "usage"
    );
    println!("  {}", "─".repeat(94));
    for account in accounts {
        println!(
            "  {:<12}  {:<22}  {:<12}  {:<12}  {:<18}  {}",
            truncate(&account.provider, 12),
            truncate(&account.account_label, 22),
            truncate(&account.window_kind, 12),
            truncate(&account.status, 12),
            usage_amount_label(account),
            truncate(&account.source, 24),
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UsageTarget {
    container: String,
    instance_id: Option<String>,
}

impl UsageTarget {
    fn display_label(&self) -> String {
        match self.instance_id.as_deref() {
            Some(id) if id != self.container => format!("{} ({id})", self.container),
            _ => self.container.clone(),
        }
    }
}

fn resolve_usage_target(paths: &JackinPaths, input: &str) -> Result<UsageTarget> {
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir)?;
    let mut matches = Vec::new();
    for entry in index.instances {
        if entry.status == InstanceStatus::Purged {
            continue;
        }
        if entry.container_base == input || entry.instance_id == input {
            matches.push(UsageTarget {
                container: entry.container_base,
                instance_id: Some(entry.instance_id),
            });
        }
    }
    matches.sort_by(|a, b| a.container.cmp(&b.container));
    matches.dedup_by(|a, b| a.container == b.container);

    match matches.as_slice() {
        [] => Ok(UsageTarget {
            container: input.to_owned(),
            instance_id: None,
        }),
        [target] => Ok(target.clone()),
        _ => anyhow::bail!(
            "instance reference {input:?} is ambiguous; pass the full container name instead"
        ),
    }
}

fn usage_amount_label(account: &AccountUsageSnapshotView) -> String {
    match (
        account.used_amount,
        account.used_unit.as_deref(),
        account.limit_amount,
        account.limit_unit.as_deref(),
    ) {
        (Some(used), Some(used_unit), Some(limit), Some(limit_unit)) if used_unit == limit_unit => {
            format!("{used}/{limit} {used_unit}")
        }
        (Some(used), Some(unit), _, _) => format!("{used} {unit}"),
        (_, _, Some(limit), Some(unit)) => format!("limit {limit} {unit}"),
        _ => "unknown".to_owned(),
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_owned();
    }
    let mut out: String = value.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests;

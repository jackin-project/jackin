use anyhow::Result;
use clap::{Args, Subcommand};
use jackin_protocol::control::{AccountUsageSnapshotView, UsageSummaryView};
use serde::Serialize;

use crate::cli::format::{OutputEnvelope, OutputFormat};
use crate::cli::{BANNER, HELP_STYLES};
use crate::instance::{InstanceIndex, InstanceStatus};
use crate::paths::JackinPaths;
use crate::runtime::snapshot::{self, UsageSummaryScope};

/// `jackin usage` — read cached Capsule usage/quota snapshots.
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Read cached usage and quota data from a running Capsule daemon",
    long_about = "Read cached usage and quota data from a running Capsule daemon.\n\n\
        This command never polls providers itself. It talks to the selected\n\
        instance's jackin-capsule daemon and renders the daemon-cached account\n\
        and token/cost snapshots that Capsule uses for the status bar and overlay."
)]
pub struct UsageArgs {
    /// Container name or short instance id
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
    Accounts,
    /// Show cached workspace token/cost attribution
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Workspace(UsageWorkspaceArgs),
    /// Show cached session token/cost attribution
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Session(UsageSessionArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageWorkspaceArgs {
    /// Workspace name. Defaults to the selected instance's workspace.
    pub workspace: Option<String>,
    /// Limit attribution to a recent window.
    #[arg(long, value_name = "SECONDS")]
    pub window_seconds: Option<i64>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageSessionArgs {
    /// Capsule session id
    pub session_id: i64,
    /// Limit attribution to a recent window.
    #[arg(long, value_name = "SECONDS")]
    pub window_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
struct UsageAccountsOutput {
    container: String,
    accounts: Vec<AccountUsageSnapshotView>,
}

#[derive(Debug, Serialize)]
struct UsageSummaryOutput {
    container: String,
    summary: UsageSummaryView,
}

impl UsageArgs {
    fn output_format(&self) -> OutputFormat {
        OutputFormat::parse(&self.format)
    }
}

pub async fn run(args: &UsageArgs, paths: &JackinPaths) -> Result<()> {
    let target = resolve_usage_target(paths, &args.instance)?;
    match &args.scope {
        UsageScope::Accounts => run_accounts(args, paths, &target),
        UsageScope::Workspace(scope_args) => run_workspace(args, paths, &target, scope_args),
        UsageScope::Session(scope_args) => run_session(args, paths, &target, scope_args),
    }
}

fn run_accounts(args: &UsageArgs, paths: &JackinPaths, target: &UsageTarget) -> Result<()> {
    let accounts = snapshot::fetch_usage_accounts(paths, &target.container)?.unwrap_or_default();

    if args.output_format() == OutputFormat::Json {
        let envelope = OutputEnvelope::v1(UsageAccountsOutput {
            container: target.container.clone(),
            accounts,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    print!("{BANNER}");
    println!("usage accounts for {}\n", target.display_label());
    if accounts.is_empty() {
        println!("  no cached usage accounts");
        return Ok(());
    }

    println!(
        "  {:<12}  {:<22}  {:<12}  {:<12}  {:<18}  source",
        "provider", "account", "window", "status", "usage"
    );
    println!("  {}", "─".repeat(94));
    for account in &accounts {
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
    Ok(())
}

fn run_workspace(
    args: &UsageArgs,
    paths: &JackinPaths,
    target: &UsageTarget,
    scope_args: &UsageWorkspaceArgs,
) -> Result<()> {
    let workspace = scope_args
        .workspace
        .as_deref()
        .or(target.workspace_name.as_deref());
    let summary = snapshot::fetch_usage_summary(
        paths,
        &target.container,
        UsageSummaryScope::Workspace {
            workspace,
            window_seconds: scope_args.window_seconds,
        },
    )?
    .unwrap_or_default();
    render_summary(args, target, summary)
}

fn run_session(
    args: &UsageArgs,
    paths: &JackinPaths,
    target: &UsageTarget,
    scope_args: &UsageSessionArgs,
) -> Result<()> {
    let summary = snapshot::fetch_usage_summary(
        paths,
        &target.container,
        UsageSummaryScope::Session {
            session_id: scope_args.session_id,
            window_seconds: scope_args.window_seconds,
        },
    )?
    .unwrap_or_default();
    render_summary(args, target, summary)
}

fn render_summary(args: &UsageArgs, target: &UsageTarget, summary: UsageSummaryView) -> Result<()> {
    if args.output_format() == OutputFormat::Json {
        let envelope = OutputEnvelope::v1(UsageSummaryOutput {
            container: target.container.clone(),
            summary,
        });
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    print!("{BANNER}");
    println!("usage summary for {}\n", target.display_label());
    println!("  samples          {}", summary.sample_count);
    println!(
        "  tokens           input={} output={} cache_read={} cache_write={}",
        summary.token_input,
        summary.token_output,
        summary.token_cache_read,
        summary.token_cache_write
    );
    println!("  cost             {}", cost_label(summary.cost_usd_micros));
    println!(
        "  provenance       exact={} estimated={} unpriced={}",
        summary.exact_cost_sample_count,
        summary.estimated_cost_sample_count,
        summary.unpriced_sample_count
    );
    if let Some(workspace) = summary.workspace.as_deref() {
        println!("  workspace        {workspace}");
    }
    if let Some(session_id) = summary.session_id {
        println!("  session          {session_id}");
    }
    if let Some(window_seconds) = summary.window_seconds {
        println!("  window           {window_seconds}s");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UsageTarget {
    container: String,
    instance_id: Option<String>,
    workspace_name: Option<String>,
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
                workspace_name: entry.workspace_name.or(Some(entry.workspace_label)),
            });
        }
    }
    matches.sort_by(|a, b| a.container.cmp(&b.container));
    matches.dedup_by(|a, b| a.container == b.container);

    match matches.as_slice() {
        [] => Ok(UsageTarget {
            container: input.to_owned(),
            instance_id: None,
            workspace_name: None,
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

fn cost_label(micros: u64) -> String {
    format!("${:.2}", micros as f64 / 1_000_000.0)
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

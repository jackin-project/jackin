// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::{Args, Subcommand};
use jackin_protocol::control::AccountUsageSnapshotView;
use serde::Serialize;

use crate::cli::format::{OutputEnvelope, OutputFormat};
use crate::cli::{BANNER, HELP_STYLES};
use jackin_core::JackinPaths;
use jackin_runtime::instance::{InstanceIndex, InstanceStatus};
use jackin_runtime::runtime::snapshot;

mod store;

/// `jackin usage` — Capsule-cached or host-probed usage snapshots.
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Read usage and quota data from Capsule cache or host probes",
    long_about = "Read usage and quota data.\n\n\
        Default path talks to a Capsule daemon instance and renders daemon-cached\n\
        account snapshots (status bar + overlay). Use `jackin usage cache accounts`\n\
        for the host-global account cache.\n\n\
        Host path (menu bar / offline Capsule): `jackin usage host snapshot`\n\
        probes via jackin-usage HostUsageRuntime — same FocusedUsageView fields\n\
        as Capsule, from host credentials."
)]
pub struct UsageArgs {
    /// Container name, short instance id, `cache`, or `host`
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
    /// Verify all provider quota rows are present and trusted
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Verify,
    /// Host-side probe snapshot (no Capsule; uses jackin-usage host runtime)
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Snapshot(UsageHostSnapshotArgs),
}

/// `jackin usage host snapshot --agent claude`
#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageHostSnapshotArgs {
    /// Host surface id: claude, codex, amp, grok, kimi, opencode, zai, minimax
    #[arg(long, value_name = "SURFACE")]
    pub agent: String,
    /// Skip network refresh (read cache / honest refreshing only)
    #[arg(long, default_value_t = false)]
    pub no_refresh: bool,
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
    if args.instance == "host" {
        return run_host(args, paths);
    }
    let target = resolve_usage_target(paths, &args.instance)?;
    match &args.scope {
        UsageScope::Accounts(scope_args) => run_accounts(args, paths, &target, scope_args).await,
        UsageScope::Verify => run_verify(paths, &target),
        UsageScope::Snapshot(_) => {
            anyhow::bail!("`jackin usage <instance> snapshot` is only valid with instance `host`")
        }
    }
}

fn run_host(args: &UsageArgs, paths: &JackinPaths) -> Result<()> {
    match &args.scope {
        UsageScope::Snapshot(scope) => run_host_snapshot(args, paths, scope),
        UsageScope::Accounts(_) | UsageScope::Verify => {
            anyhow::bail!(
                "`jackin usage host` supports `snapshot` only; use `jackin usage cache accounts` for the host cache"
            )
        }
    }
}

fn run_host_snapshot(
    args: &UsageArgs,
    paths: &JackinPaths,
    scope: &UsageHostSnapshotArgs,
) -> Result<()> {
    use jackin_usage::host::{HostRuntimeConfig, HostSurfaceId, HostUsageRuntime};

    let surface = HostSurfaceId::from_id(&scope.agent).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown host surface `{}`; expected one of: {}",
            scope.agent,
            HostSurfaceId::ALL
                .iter()
                .map(|s| s.id())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(HostRuntimeConfig::under_data_dir(&paths.data_dir))
        .map_err(|err| anyhow::anyhow!(err))?;

    if !scope.no_refresh {
        runtime
            .refresh(Some(surface.id()), true)
            .map_err(|err| anyhow::anyhow!(err))?;
    }
    let view = runtime
        .snapshot(surface.id())
        .map_err(|err| anyhow::anyhow!(err))?;

    if args.output_format() == OutputFormat::Json {
        let envelope = OutputEnvelope::v1(view);
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    print!("{BANNER}");
    println!("host usage snapshot · {}\n", surface.label());
    println!("  status_bar_label  {}", view.status_bar_label);
    println!("  status            {:?}", view.status);
    println!("  source            {:?}", view.source);
    println!("  confidence        {:?}", view.confidence);
    println!("  account           {}", view.account.account_label);
    if let Some(plan) = &view.account.plan_label {
        println!("  plan              {plan}");
    }
    if let Some(origin) = &view.account.credential_origin {
        println!("  credential        {origin}");
    }
    if view.buckets.is_empty() {
        println!("  buckets           (none — no invented percentages)");
    } else {
        for bucket in &view.buckets {
            println!(
                "  bucket            {} remaining={:?} resets_at={:?} status={:?}",
                bucket.label, bucket.remaining_percent, bucket.resets_at, bucket.status
            );
        }
    }
    if let Some(err) = &view.last_error {
        println!("  last_error        {err}");
    }
    Ok(())
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
        UsageScope::Verify => {
            anyhow::bail!(
                "`jackin usage cache verify` is invalid; verification must query a running Capsule daemon"
            )
        }
        UsageScope::Snapshot(_) => {
            anyhow::bail!(
                "`jackin usage cache snapshot` is invalid; use `jackin usage host snapshot`"
            )
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

fn run_verify(paths: &JackinPaths, target: &UsageTarget) -> Result<()> {
    let accounts = snapshot::fetch_usage_accounts(paths, &target.container)?.unwrap_or_default();
    let checks = verify_usage_accounts(&accounts);
    print!("{BANNER}");
    println!("usage verification for {}\n", target.display_label());
    for check in &checks {
        println!(
            "  {:<9} {}",
            check.label,
            check.detail.as_deref().unwrap_or(check.status)
        );
    }
    let failures = checks
        .iter()
        .filter(|check| check.status != "ok")
        .map(|check| format!("{}: {}", check.label, check.status))
        .collect::<Vec<_>>();
    if !failures.is_empty() {
        anyhow::bail!("usage verification failed: {}", failures.join(", "));
    }
    println!("\n  usage verification passed");
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
struct UsageVerifyCheck {
    label: &'static str,
    status: &'static str,
    detail: Option<String>,
}

fn verify_usage_accounts(accounts: &[AccountUsageSnapshotView]) -> Vec<UsageVerifyCheck> {
    usage_verify_provider_aliases()
        .iter()
        .map(|(label, aliases)| verify_usage_provider(label, aliases, accounts))
        .collect()
}

fn usage_verify_provider_aliases() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        ("OpenAI", &["Codex", "OpenAI / Codex"]),
        ("Anthropic", &["Claude", "Anthropic / Claude"]),
        ("Amp", &["Amp"]),
        ("xAI", &["Grok Build", "xAI / Grok"]),
        ("Z.AI", &["GLM / Z.AI"]),
        ("Kimi", &["Kimi"]),
        ("MiniMax", &["MiniMax"]),
    ]
}

fn verify_usage_provider(
    label: &'static str,
    aliases: &[&str],
    accounts: &[AccountUsageSnapshotView],
) -> UsageVerifyCheck {
    let rows = accounts
        .iter()
        .filter(|account| {
            aliases
                .iter()
                .any(|alias| usage_provider_matches(alias, &account.provider))
        })
        .collect::<Vec<_>>();
    // `max_by_key` is `None` exactly when there are no matching rows, so it
    // doubles as the "missing" guard.
    let Some(latest) = rows.iter().max_by_key(|row| row.fetched_at) else {
        return UsageVerifyCheck {
            label,
            status: "missing",
            detail: None,
        };
    };
    if rows.iter().any(|row| usage_row_proves_live_quota(row)) {
        return UsageVerifyCheck {
            label,
            status: "ok",
            detail: Some(format!(
                "ok: {} {} {} {} row(s)",
                latest.status,
                latest.source,
                latest.confidence,
                rows.len()
            )),
        };
    }
    UsageVerifyCheck {
        label,
        status: "untrusted",
        detail: Some(format!(
            "untrusted: latest status={} source={} confidence={} error={}",
            latest.status,
            latest.source,
            latest.confidence,
            latest.last_error.as_deref().unwrap_or("none")
        )),
    }
}

fn usage_row_proves_live_quota(row: &AccountUsageSnapshotView) -> bool {
    row.status == "fresh"
        && row.confidence == "authoritative"
        && matches!(row.source.as_str(), "provider_api" | "cli")
        && !row.window_kind.trim().is_empty()
        && !row.account_label.trim().is_empty()
        && !row.account_label.to_ascii_lowercase().contains("needs")
}

fn usage_provider_matches(needle: &str, provider: &str) -> bool {
    // Interchangeable provider/agent labels: a match needs one member of a group
    // on each side. Bidirectional and extensible — add a group, not two arms.
    const SYNONYMS: &[&[&str]] = &[
        &["openai", "codex"],
        &["anthropic", "claude"],
        &["xai", "grok"],
        &["zai", "glm"],
    ];
    let needle = normalize_usage_provider_label(needle);
    let provider = normalize_usage_provider_label(provider);
    provider.contains(&needle)
        || needle.contains(&provider)
        || SYNONYMS.iter().any(|group| {
            group.iter().any(|m| needle.contains(m)) && group.iter().any(|m| provider.contains(m))
        })
}

fn normalize_usage_provider_label(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
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

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::{Args, Subcommand};
use jackin_protocol::control::UsageAccountMembershipV1;
use jackin_protocol::usage_broker::{UsageLimitWindowV2, UsageProjectionV2};
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;

use crate::cli::format::{OutputEnvelope, OutputFormat};
use crate::cli::{BANNER, HELP_STYLES};
use jackin_core::JackinPaths;
use jackin_docker::docker_client::{BollardDockerClient, DockerApi};
use jackin_runtime::instance::{InstanceIndex, InstanceStatus};
use jackin_runtime::runtime::snapshot;

mod store;

#[derive(Default)]
pub(crate) struct CliUsageSecretSource;

impl jackin_usage::host::ProviderCredentialSecretSource for CliUsageSecretSource {
    fn lookup_declaration(
        &self,
        config: &jackin_config::AppConfig,
        workspace: Option<&jackin_core::WorkspaceName>,
        role: Option<&str>,
        entry: jackin_core::UsageCredentialEnvName,
    ) -> Option<jackin_config::EnvValue> {
        jackin_env::lookup_operator_env_declaration(config, role, workspace, entry.name)
    }

    fn resolve_secret(
        &self,
        config: &jackin_config::AppConfig,
        workspace: Option<&jackin_core::WorkspaceName>,
        role: Option<&str>,
        entry: jackin_core::UsageCredentialEnvName,
    ) -> Option<jackin_usage::host::ProviderCredentialSecretResolution> {
        use jackin_usage::host::{
            ProviderCredentialSecretOutcome, ProviderCredentialSecretResolution,
        };

        let declaration =
            jackin_env::lookup_operator_env_declaration(config, role, workspace, entry.name)?;
        let resolved =
            jackin_env::resolve_operator_env_per_key_matching(config, role, workspace, |key| {
                key == entry.name
            })
            .into_iter()
            .next();
        let outcome = match resolved {
            Some(result)
                if result.status() == jackin_env::OperatorEnvKeyStatus::Resolved
                    && result.resolved_value().is_some() =>
            {
                ProviderCredentialSecretOutcome::Resolved(
                    result.resolved_value().unwrap_or_default().to_owned(),
                )
            }
            Some(result) => match result.status() {
                jackin_env::OperatorEnvKeyStatus::Resolved => {
                    ProviderCredentialSecretOutcome::Malformed
                }
                jackin_env::OperatorEnvKeyStatus::Missing => {
                    ProviderCredentialSecretOutcome::Missing
                }
                jackin_env::OperatorEnvKeyStatus::DeniedOrUnavailable => {
                    ProviderCredentialSecretOutcome::Denied
                }
                jackin_env::OperatorEnvKeyStatus::Malformed => {
                    ProviderCredentialSecretOutcome::Malformed
                }
                jackin_env::OperatorEnvKeyStatus::InteractionRequired => {
                    ProviderCredentialSecretOutcome::InteractionRequired
                }
            },
            None => return None,
        };
        Some(ProviderCredentialSecretResolution {
            declaration,
            outcome,
        })
    }
}

pub(crate) type CliUsageCredentialResolver =
    jackin_usage::host::CachedProviderCredentialResolver<CliUsageSecretSource>;

/// `jackin usage` — simple host projection output, or explicit instance inspection.
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Read simple limits-only usage output",
    long_about = "Read the canonical host usage projection.\n\n\
        Human output stays intentionally compact for scripts and quick inspection.\n\
        Use `jackin usage <instance> accounts|verify` for explicit Capsule\n\
        inspection, or `jackin usage host snapshot` for a provider snapshot."
)]
pub struct UsageArgs {
    /// Container name, short instance id, `cache`, or `host`; omit for host-wide usage
    pub instance: Option<String>,
    #[command(subcommand)]
    pub scope: Option<UsageScope>,
    /// Output format
    #[arg(long, global = true, value_name = "FORMAT", default_value = "human")]
    pub format: String,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageScope {
    /// Show canonical scoped account membership and quota
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
    /// Host surface id: codex, claude, amp, grok, zai, kimi, minimax, opencode,
    /// google, cursor, meta, openrouter
    #[arg(long, value_name = "SURFACE")]
    pub agent: String,
    /// Skip network refresh (read cache / honest refreshing only)
    #[arg(long, default_value_t = false)]
    pub no_refresh: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageAccountsArgs {
    /// Persist current scoped membership in ~/.jackin/data/daemon/accounts.db.
    ///
    /// Store the accepted publication or explicit unavailable/revoked state
    /// for this immutable container identity.
    #[arg(long)]
    pub sync_host_cache: bool,
}

#[derive(Debug, Serialize)]
struct UsageAccountsOutput {
    container: String,
    membership: Vec<UsageScopedOutput>,
    synced_host_cache_path: Option<String>,
    host_cache_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct UsageScopedOutput {
    scope_id: String,
    membership: UsageAccountMembershipV1,
}

impl UsageArgs {
    fn output_format(&self) -> OutputFormat {
        OutputFormat::parse(&self.format)
    }
}

pub async fn run(args: &UsageArgs, paths: &JackinPaths) -> Result<()> {
    let Some(instance) = args.instance.as_deref() else {
        if args.scope.is_some() {
            anyhow::bail!(
                "`jackin usage` does not accept an instance scope; use `jackin usage <instance> ...`"
            );
        }
        return run_bare_host(args, paths);
    };
    if instance == "cache" {
        return run_cache(args, paths).await;
    }
    if instance == "host" {
        return run_host(args, paths);
    }
    let scope = args
        .scope
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing usage scope; choose `accounts` or `verify`"))?;
    let target = resolve_usage_target(paths, instance)?;
    let docker = BollardDockerClient::connect()?;
    let inspection = docker.inspect_container_by_name(&target.container).await;
    let container = inspection.handle.ok_or_else(|| {
        anyhow::anyhow!(
            "cannot resolve container {}: {}",
            target.container,
            inspection.state.inspect_label()
        )
    })?;
    match scope {
        UsageScope::Accounts(scope_args) => {
            run_accounts(args, paths, &target, &container, scope_args).await
        }
        UsageScope::Verify => run_verify(args, paths, &target, &container),
        UsageScope::Snapshot(_) => {
            anyhow::bail!("`jackin usage <instance> snapshot` is only valid with instance `host`")
        }
    }
}

/// Render one bounded, final-only host projection. This intentionally stays
/// plainer than the Capsule/Console surfaces: no meters, animation, or
/// interactive chrome belong in a command intended for pipes and scripts.
fn run_bare_host(args: &UsageArgs, paths: &JackinPaths) -> Result<()> {
    use jackin_usage::host::{
        HostRuntimeConfig, HostUsageRuntime, UsageBrokerConfig, UsageDiscoveryScope,
        discover_and_ensure_usage_broker,
    };

    let resolver = Arc::new(CliUsageCredentialResolver::default());
    let discovery_scope = UsageDiscoveryScope::HostDesktop {
        config_root: paths.config_dir.clone(),
        operator_home: paths.home_dir.clone(),
    };
    let host_config = HostRuntimeConfig {
        data_dir: paths.data_dir.clone(),
        refresh_floor_secs: 300,
        enabled_surface_ids: Vec::new(),
        probe_policy: jackin_usage::host::HostProbePolicy::Live,
        discovery_scope: discovery_scope.clone(),
    };
    let broker = discover_and_ensure_usage_broker(
        UsageBrokerConfig::for_data_dir(paths.data_dir.clone()),
        discovery_scope,
        resolver,
    )
    .map_err(|error| anyhow::anyhow!(error.message))?;
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open_with_validated_discovery(host_config, broker.discovery)
        .map_err(|error| anyhow::anyhow!(error))?;
    let client = broker.client;

    for capability in broker.capabilities {
        let current = client
            .current(capability.clone())
            .map_err(|error| anyhow::anyhow!(error.message))?;
        let state = client
            .refresh(capability.clone(), current.generation, true)
            .map_err(|error| anyhow::anyhow!(error.message))?;
        let state = if state.phase.is_active() {
            client
                .join(capability, state.generation, Duration::from_secs(30))
                .map_err(|error| anyhow::anyhow!(error.message))?
        } else {
            state
        };
        runtime
            .apply_broker_generation(state)
            .map_err(|error| anyhow::anyhow!(error))?;
    }

    let projection = runtime
        .canonical_projection("und")
        .map_err(|error| anyhow::anyhow!(error))?;
    if args.output_format() == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&projection)?);
    } else {
        print_bare_host_projection(&projection);
    }
    Ok(())
}

fn print_bare_host_projection(projection: &UsageProjectionV2) {
    print!("{BANNER}");
    println!("usage\n");
    if projection.providers.is_empty() {
        println!("no configured accounts");
    }
    for provider in &projection.providers {
        println!("{}", provider.display_name);
        for account in &provider.accounts {
            let status = account.status_label.as_deref().unwrap_or("available");
            println!("  {} · {status}", account.display_label);
            for window in &account.windows {
                print_bare_limit(window);
            }
        }
    }
    for unresolved in &projection.unresolved {
        println!("{} · {:?}", unresolved.provider_id, unresolved.state);
    }
}

fn print_bare_limit(window: &UsageLimitWindowV2) {
    let value = if window.value_label.is_empty() {
        "—"
    } else {
        window.value_label.as_str()
    };
    if window.reset_label.is_empty() {
        println!("    {}  {}", window.label, value);
    } else {
        println!("    {}  {} · {}", window.label, value, window.reset_label);
    }
}

fn run_host(args: &UsageArgs, paths: &JackinPaths) -> Result<()> {
    let scope = args
        .scope
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing host usage scope; choose `snapshot`"))?;
    match scope {
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
    use jackin_usage::host::{
        HostProbePolicy, HostRuntimeConfig, HostSurfaceId, HostUsageRuntime, UsageBrokerConfig,
        UsageDiscoveryScope, discover_and_ensure_usage_broker,
    };

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

    let resolver = Arc::new(CliUsageCredentialResolver::default());
    let discovery_scope = UsageDiscoveryScope::HostDesktop {
        config_root: paths.config_dir.clone(),
        operator_home: paths.home_dir.clone(),
    };
    let host_config = HostRuntimeConfig {
        data_dir: paths.data_dir.clone(),
        refresh_floor_secs: 300,
        enabled_surface_ids: Vec::new(),
        probe_policy: HostProbePolicy::Live,
        discovery_scope: discovery_scope.clone(),
    };
    let broker_config = UsageBrokerConfig::for_data_dir(paths.data_dir.clone());
    let mut runtime = HostUsageRuntime::new();
    if scope.no_refresh {
        runtime
            .open_with_discovery(host_config, resolver.as_ref())
            .map_err(|err| anyhow::anyhow!(err))?;
    } else {
        let broker = discover_and_ensure_usage_broker(broker_config, discovery_scope, resolver)
            .map_err(|error| anyhow::anyhow!(error.message))?;
        runtime
            .open_with_validated_discovery(host_config, broker.discovery)
            .map_err(|err| anyhow::anyhow!(err))?;
        let client = broker.client;
        for capability in broker
            .capabilities
            .into_iter()
            .filter(|capability| capability.surface_id == surface.id())
        {
            let current = client
                .current(capability.clone())
                .map_err(|error| anyhow::anyhow!(error.message))?;
            let mut state = client
                .refresh(capability.clone(), current.generation, true)
                .map_err(|error| anyhow::anyhow!(error.message))?;
            if state.phase.is_active() {
                state = client
                    .join(capability, state.generation, Duration::from_secs(30))
                    .map_err(|error| anyhow::anyhow!(error.message))?;
            }
            runtime
                .apply_broker_generation(state)
                .map_err(|err| anyhow::anyhow!(err))?;
        }
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
    let scope = args
        .scope
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing cache usage scope; choose `accounts`"))?;
    match scope {
        UsageScope::Accounts(scope_args) => {
            if scope_args.sync_host_cache {
                anyhow::bail!(
                    "`jackin usage cache accounts --sync-host-cache` is invalid; cache reads never write host state"
                );
            }
            let (path, memberships) = store::read_memberships(paths).await?;
            let memberships = validate_cached_memberships(paths, memberships).await?;
            if args.output_format() == OutputFormat::Json {
                let envelope = OutputEnvelope::v1(UsageAccountsOutput {
                    container: "host-cache".to_owned(),
                    membership: memberships
                        .into_iter()
                        .map(|entry| UsageScopedOutput {
                            scope_id: entry.scope.container_id,
                            membership: entry.membership,
                        })
                        .collect(),
                    synced_host_cache_path: None,
                    host_cache_path: Some(path.display().to_string()),
                });
                println!("{}", serde_json::to_string_pretty(&envelope)?);
                return Ok(());
            }
            print!("{BANNER}");
            println!("usage accounts for host cache\n");
            println!("  cache {}", path.display());
            for entry in &memberships {
                println!("  scope {}", entry.scope.container_id);
                render_membership(&entry.membership);
            }
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
    container: &jackin_core::ContainerHandle,
    scope_args: &UsageAccountsArgs,
) -> Result<()> {
    let (scope, membership) = fetch_validated_membership(paths, container)?;
    let synced_host_cache_path = if scope_args.sync_host_cache {
        let path = store::store_membership(paths, &scope, &membership).await?;
        Some(path)
    } else {
        None
    };

    if args.output_format() == OutputFormat::Json {
        let envelope = OutputEnvelope::v1(UsageAccountsOutput {
            container: target.container.clone(),
            membership: vec![UsageScopedOutput {
                scope_id: container.id().to_owned(),
                membership: membership.clone(),
            }],
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
    render_membership(&membership);
    if let Some(path) = synced_host_cache_path {
        println!("\n  synced host cache {}", path.display());
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct UsageVerificationOutput<'a> {
    container: &'a str,
    scope_id: &'a str,
    membership: &'a UsageAccountMembershipV1,
    checks: Vec<UsageVerifyCheck>,
}

fn run_verify(
    args: &UsageArgs,
    paths: &JackinPaths,
    target: &UsageTarget,
    container: &jackin_core::ContainerHandle,
) -> Result<()> {
    let (_, membership) = fetch_validated_membership(paths, container)?;
    write_usage_verification(
        &mut std::io::stdout().lock(),
        args.output_format(),
        target,
        container.id(),
        &membership,
    )
}

fn write_usage_verification(
    writer: &mut impl std::io::Write,
    format: OutputFormat,
    target: &UsageTarget,
    scope_id: &str,
    membership: &UsageAccountMembershipV1,
) -> Result<()> {
    let checks = verify_usage_membership(membership);
    let failures = checks
        .iter()
        .filter(|check| check.status != "ok")
        .map(|check| format!("{}: {}", check.label, check.status))
        .collect::<Vec<_>>();
    if format == OutputFormat::Json {
        let envelope = OutputEnvelope::v1(UsageVerificationOutput {
            container: &target.container,
            scope_id,
            membership,
            checks,
        });
        serde_json::to_writer_pretty(&mut *writer, &envelope)?;
        writeln!(writer)?;
    } else {
        write!(writer, "{BANNER}")?;
        writeln!(
            writer,
            "usage verification for {}\n",
            target.display_label()
        )?;
        for check in &checks {
            writeln!(
                writer,
                "  {:<9} {}",
                check.label,
                check.detail.as_deref().unwrap_or(check.status)
            )?;
        }
        if failures.is_empty() {
            writeln!(writer, "\n  usage verification passed")?;
        }
    }
    if !failures.is_empty() {
        anyhow::bail!("usage verification failed: {}", failures.join(", "));
    }
    Ok(())
}

fn fetch_validated_membership(
    paths: &JackinPaths,
    container: &jackin_core::ContainerHandle,
) -> Result<(
    jackin_usage::usage_snapshot_store::UsageMembershipScope,
    UsageAccountMembershipV1,
)> {
    use jackin_runtime::usage_relay::validated_usage_inventory_config_generation;
    let proof = validated_usage_inventory_config_generation(paths, container)?;
    let mut membership = snapshot::fetch_usage_accounts(container)?;
    if let UsageAccountMembershipV1::Current { projection } = &membership {
        match jackin_runtime::usage_relay::validate_usage_inventory_projection(
            paths, container, projection,
        ) {
            Ok(accepted_proof) => anyhow::ensure!(
                accepted_proof == proof,
                "usage membership changed during inspection; retry"
            ),
            Err(_) => membership = UsageAccountMembershipV1::Unavailable,
        }
    }
    let current_proof = validated_usage_inventory_config_generation(paths, container)?;
    anyhow::ensure!(
        proof == current_proof,
        "usage membership changed during inspection; retry"
    );
    Ok((
        jackin_usage::usage_snapshot_store::UsageMembershipScope {
            container_id: container.id().to_owned(),
            workspace_config_proof: proof,
        },
        membership,
    ))
}

enum CachedMembershipAuthority {
    Validated { container_id: String, proof: String },
    Absent,
    Unavailable,
}

fn apply_cached_membership_authority(
    entry: &mut jackin_usage::usage_snapshot_store::StoredUsageMembership,
    authority: CachedMembershipAuthority,
) {
    if !matches!(entry.membership, UsageAccountMembershipV1::Current { .. }) {
        return;
    }
    match authority {
        CachedMembershipAuthority::Validated {
            container_id,
            proof,
        } if container_id == entry.scope.container_id
            && proof == entry.scope.workspace_config_proof => {}
        CachedMembershipAuthority::Validated { .. } | CachedMembershipAuthority::Absent => {
            entry.membership = UsageAccountMembershipV1::Revoked
        }
        CachedMembershipAuthority::Unavailable => {
            entry.membership = UsageAccountMembershipV1::Unavailable
        }
    }
}

async fn validate_cached_memberships(
    paths: &JackinPaths,
    mut memberships: Vec<jackin_usage::usage_snapshot_store::StoredUsageMembership>,
) -> Result<Vec<jackin_usage::usage_snapshot_store::StoredUsageMembership>> {
    if !memberships
        .iter()
        .any(|entry| matches!(entry.membership, UsageAccountMembershipV1::Current { .. }))
    {
        return Ok(memberships);
    }
    let docker = match BollardDockerClient::connect() {
        Ok(docker) => docker,
        Err(_) => {
            for entry in &mut memberships {
                if matches!(entry.membership, UsageAccountMembershipV1::Current { .. }) {
                    apply_cached_membership_authority(
                        entry,
                        CachedMembershipAuthority::Unavailable,
                    );
                }
            }
            return Ok(memberships);
        }
    };
    validate_cached_memberships_with_docker(paths, &docker, memberships).await
}

async fn validate_cached_memberships_with_docker(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    mut memberships: Vec<jackin_usage::usage_snapshot_store::StoredUsageMembership>,
) -> Result<Vec<jackin_usage::usage_snapshot_store::StoredUsageMembership>> {
    let containers = match docker.list_containers(&[], true).await {
        Ok(containers) => containers,
        Err(_) => {
            for entry in &mut memberships {
                if matches!(entry.membership, UsageAccountMembershipV1::Current { .. }) {
                    apply_cached_membership_authority(
                        entry,
                        CachedMembershipAuthority::Unavailable,
                    );
                }
            }
            return Ok(memberships);
        }
    };
    for entry in &mut memberships {
        let UsageAccountMembershipV1::Current { projection } = &entry.membership else {
            continue;
        };
        // List supplies the Docker display name; membership identity remains exact immutable ID.
        let Some(row) = containers
            .iter()
            .find(|row| row.id == entry.scope.container_id)
        else {
            apply_cached_membership_authority(entry, CachedMembershipAuthority::Absent);
            continue;
        };
        let container = jackin_core::ContainerHandle::new(&row.name, &row.id)?;
        let authority = match jackin_runtime::usage_relay::validate_usage_inventory_projection(
            paths, &container, projection,
        ) {
            Ok(proof) => CachedMembershipAuthority::Validated {
                container_id: container.id().to_owned(),
                proof,
            },
            Err(_) => CachedMembershipAuthority::Unavailable,
        };
        apply_cached_membership_authority(entry, authority);
    }
    Ok(memberships)
}

fn render_membership(membership: &UsageAccountMembershipV1) {
    match membership {
        UsageAccountMembershipV1::Unavailable => println!("  membership unavailable"),
        UsageAccountMembershipV1::Revoked => println!("  membership revoked"),
        UsageAccountMembershipV1::Current { projection } => {
            if projection.providers.is_empty()
                && projection.unresolved.is_empty()
                && projection.unresolved_grants.is_empty()
            {
                println!("  no configured accounts");
            }
            for provider in &projection.providers {
                println!("  {}", provider.display_name);
                for account in &provider.accounts {
                    println!("    {} · {:?}", account.display_label, account.lifecycle);
                    for window in &account.windows {
                        print_bare_limit(window);
                    }
                    for group in &account.metric_groups {
                        println!(
                            "    {}  {} · {:?}",
                            group.label,
                            metric_value_label(&group.value),
                            group.quota_state
                        );
                    }
                }
            }
            for unresolved in &projection.unresolved {
                println!("  {} · {:?}", unresolved.provider_id, unresolved.state);
            }
            for grant in &projection.unresolved_grants {
                println!(
                    "  {} / {} · unresolved",
                    grant.surface_id, grant.configured_account_id
                );
            }
        }
    }
}

fn metric_value_label(value: &jackin_protocol::usage_broker::UsageMetricValueV2) -> String {
    use jackin_protocol::usage_broker::UsageMetricValueV2;
    let count =
        |value: Option<u64>| value.map_or_else(|| "unknown".to_owned(), |value| value.to_string());
    match value {
        UsageMetricValueV2::Window {
            count_quota: Some(quota),
            ..
        } => jackin_usage::usage::usage_count_quota_summary(quota),
        UsageMetricValueV2::Window {
            remaining_raw_percent,
            used_raw_percent,
            ..
        } => match (remaining_raw_percent, used_raw_percent) {
            (Some(remaining), _) => format!("{remaining}% remaining"),
            (_, Some(used)) => format!("{used}% used"),
            _ => "unknown".to_owned(),
        },
        UsageMetricValueV2::Balance { amount, .. } => {
            jackin_usage::usage::usage_money_amounts_summary(None, None, Some(amount))
        }
        UsageMetricValueV2::SpendCap {
            cap,
            spent,
            remaining,
        } => jackin_usage::usage::usage_money_amounts_summary(
            spent.as_ref(),
            cap.as_ref(),
            remaining.as_ref(),
        ),
        UsageMetricValueV2::TokenTotals {
            input,
            output,
            cached,
            reasoning,
            ..
        } => format!(
            "input={} output={} cached={} reasoning={}",
            count(*input),
            count(*output),
            count(*cached),
            count(*reasoning)
        ),
        UsageMetricValueV2::RateLimit {
            limit,
            remaining,
            window_label,
        } => format!(
            "limit={} remaining={} {}",
            count(*limit),
            count(*remaining),
            window_label.as_deref().unwrap_or("")
        ),
        UsageMetricValueV2::Plan { plan_label, tier } => format!(
            "{} {}",
            plan_label.as_deref().unwrap_or("unknown plan"),
            tier.as_deref().unwrap_or("")
        ),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct UsageVerifyCheck {
    label: String,
    status: &'static str,
    detail: Option<String>,
}

fn verify_usage_membership(membership: &UsageAccountMembershipV1) -> Vec<UsageVerifyCheck> {
    use jackin_protocol::usage_broker::{
        UsageFreshnessPhaseV2, UsageLifecycleV2, UsageQuotaStateV2,
    };
    let projection = match membership {
        UsageAccountMembershipV1::Current { projection } => projection,
        UsageAccountMembershipV1::Unavailable | UsageAccountMembershipV1::Revoked => {
            return vec![UsageVerifyCheck {
                label: "membership".to_owned(),
                status: match membership {
                    UsageAccountMembershipV1::Revoked => "revoked",
                    _ => "unavailable",
                },
                detail: None,
            }];
        }
    };
    if let Err(error) = UsageAccountMembershipV1::validate_current_projection(projection) {
        return vec![UsageVerifyCheck {
            label: "membership".to_owned(),
            status: "invalid",
            detail: Some(error),
        }];
    }
    let trusted_quota = |state| {
        matches!(
            state,
            UsageQuotaStateV2::Available
                | UsageQuotaStateV2::Warning
                | UsageQuotaStateV2::Exhausted
                | UsageQuotaStateV2::NotStarted
                | UsageQuotaStateV2::NotApplicable
        )
    };
    let mut checks = Vec::new();
    for provider in &projection.providers {
        if provider.accounts.is_empty() {
            checks.push(UsageVerifyCheck {
                label: provider.provider_id.clone(),
                status: "untrusted",
                detail: None,
            });
        }
        for account in &provider.accounts {
            let trusted = provider.freshness.phase == UsageFreshnessPhaseV2::Current
                && !provider.freshness.is_stale
                && account.lifecycle == UsageLifecycleV2::Available
                && account.freshness.phase == UsageFreshnessPhaseV2::Current
                && !account.freshness.is_stale
                && (!account.windows.is_empty()
                    || account
                        .metric_groups
                        .iter()
                        .any(|group| group.quota_state != UsageQuotaStateV2::NotApplicable))
                && account
                    .windows
                    .iter()
                    .all(|window| trusted_quota(window.quota_state))
                && account.metric_groups.iter().all(|group| {
                    trusted_quota(group.quota_state)
                        && group.phase == UsageFreshnessPhaseV2::Current
                        && !group.is_stale
                });
            checks.push(UsageVerifyCheck {
                label: format!(
                    "{} / {}",
                    provider.provider_id, account.canonical_account_id
                ),
                status: if trusted { "ok" } else { "untrusted" },
                detail: None,
            });
        }
    }
    for unresolved in &projection.unresolved {
        checks.push(UsageVerifyCheck {
            label: unresolved.provider_id.clone(),
            status: "unresolved",
            detail: Some(format!("{:?}", unresolved.state)),
        });
    }
    for grant in &projection.unresolved_grants {
        checks.push(UsageVerifyCheck {
            label: format!("{} / {}", grant.surface_id, grant.configured_account_id),
            status: "unresolved",
            detail: None,
        });
    }
    checks
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

#[cfg(test)]
mod tests;

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use jackin_protocol::control::AccountUsageSnapshotView;
use jackin_protocol::control::Money;
use jackin_protocol::usage_broker::UsageProjectionV1;
use jackin_protocol::usage_monitor::{
    MonitorAccountBindingInput, MonitorConfig, MonitorIssue, MonitorIssueCode, MonitorOperation,
    MonitorPolicy, MonitorPolicyApprovalInput, MonitorProvider, MonitorPurpose, MonitorReply,
    MonitorScope, MonitorServiceStatus, MonitorStatus, MonitorTrackingReadiness, SpendRecordInput,
    SpendRecordSource, StatuslineObservation, USAGE_MONITOR_MAX_STATUSLINE_BYTES,
};
use serde::{Deserialize, Serialize};
use std::io::Read as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::cli::format::{OutputEnvelope, OutputFormat};
use crate::cli::{BANNER, HELP_STYLES};
use jackin_core::JackinPaths;
use jackin_docker::docker_client::{BollardDockerClient, DockerApi};
use jackin_runtime::instance::{InstanceIndex, InstanceStatus};
use jackin_runtime::runtime::snapshot;

mod auth;
mod errors;
mod statusline;
mod store;

use auth::all_stdio_are_terminal;
pub use auth::{UsageAuthArgs, UsageAuthCommand, UsageProviderArgs};
#[cfg(test)]
use auth::{foreground_auth_bootstrap_args, validate_keychain_service};
pub use errors::UsageCommandExit;
use errors::{
    issue_error, json_value_exit, usage_error, validate_monitor_goal_id,
    validate_monitor_identifier, validate_monitor_operator_label, validate_monitor_revision,
    validate_monitor_start_fields,
};

/// `jackin usage` — broker-owned cached usage and durable monitor operations.
#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Read broker-owned cached usage or manage durable Claude quota monitors",
    long_about = "Read the broker-owned cached usage projection without starting a broker or refreshing a provider.\n\n\
        Explicit monitor and service start commands may start a passive local broker. Monitor state and policy decisions are durable.\n\
        All monitor evidence is local; statusline observations do not contain spend."
)]
pub struct UsageArgs {
    /// Instance name for Capsule account inspection, or `cache` for the host account cache
    pub instance: Option<String>,
    #[command(subcommand)]
    pub scope: Option<UsageScope>,
    /// Output format
    #[arg(long, global = true, value_name = "FORMAT", default_value = "human")]
    pub format: String,
    /// Use an isolated jackin data directory for this usage operation
    #[arg(long, global = true, value_name = "PATH")]
    pub data_dir: Option<PathBuf>,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageScope {
    /// Show cached provider account/quota buckets
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Accounts(UsageAccountsArgs),
    /// Verify all provider quota rows are present and trusted
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Verify,
    /// Run passive readiness checks without accessing credentials or providers
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Doctor(UsageDoctorArgs),
    /// Manage the local usage broker
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Service(UsageServiceArgs),
    /// Manage an unattended quota monitor
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Monitor(UsageMonitorArgs),
    /// Confirm a local monitor-account to source-scope mapping for quota monitoring
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Binding(UsageBindingArgs),
    /// Approve a durable quota-monitor policy for a bound account and goal
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Policy(UsagePolicyArgs),
    /// Read the status of a durable monitor
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Status(UsageMonitorIdArgs),
    /// Reconcile a monitor against local evidence without provider refresh
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Refresh(UsageMonitorIdArgs),
    /// Stream new monitor events as JSON Lines
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Watch(UsageWatchArgs),
    /// Wait for runnable evidence for a bounded time
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Wait(UsageWaitArgs),
    /// Ingest or compose Claude Code statusline integration
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Statusline(UsageStatuslineArgs),
    /// Record an explicit account-bound spend baseline
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Spend(UsageSpendArgs),
    /// Prepare provider authentication in an attached terminal
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Auth(UsageAuthArgs),
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

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum UsageProviderArg {
    Claude,
}

impl From<UsageProviderArg> for MonitorProvider {
    fn from(value: UsageProviderArg) -> Self {
        match value {
            UsageProviderArg::Claude => Self::Claude,
        }
    }
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageDoctorArgs {
    #[arg(long, value_enum, required = true)]
    pub provider: UsageProviderArg,
    /// Require a noninteractive readiness report
    #[arg(long, required = true)]
    pub unattended: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageServiceArgs {
    #[command(subcommand)]
    pub command: UsageServiceCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageServiceCommand {
    /// Start the passive broker explicitly
    Start,
    /// Stop the already-running broker
    Stop,
    /// Read local broker status without starting it
    Status,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageMonitorArgs {
    #[command(subcommand)]
    pub command: UsageMonitorCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageMonitorCommand {
    /// Start an observation-only monitor and the broker if needed
    Observe(UsageMonitorObserveArgs),
    /// Start dispatch guard using an already approved policy
    Start(UsageMonitorStartArgs),
    /// Stop one monitor
    Stop(UsageMonitorIdArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageMonitorStartArgs {
    #[arg(long, value_enum, required = true)]
    pub provider: UsageProviderArg,
    #[arg(long, value_name = "ID", required = true)]
    pub binding: String,
    #[arg(long, value_name = "REVISION", required = true)]
    pub binding_revision: u64,
    #[arg(long, value_name = "ID", required = true)]
    pub goal: String,
    #[arg(long, value_name = "REVISION", required = true)]
    pub policy_revision: u64,
    #[arg(long, value_name = "KEY", required = true)]
    pub idempotency_key: String,
    #[arg(long, value_name = "ID")]
    pub session: Option<String>,
    #[arg(long, value_name = "MODEL")]
    pub expected_model: Option<String>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageMonitorObserveArgs {
    #[arg(long, value_enum, required = true)]
    pub provider: UsageProviderArg,
    /// Unbound observer scope. If omitted, pass `--binding` and its revision.
    #[arg(long, value_name = "ID", required_unless_present = "binding")]
    pub session: Option<String>,
    /// Previously confirmed account binding
    #[arg(long, value_name = "ID", requires = "binding_revision")]
    pub binding: Option<String>,
    #[arg(long, value_name = "REVISION", requires = "binding")]
    pub binding_revision: Option<u64>,
    #[arg(long, value_name = "KEY", required = true)]
    pub idempotency_key: String,
    #[arg(long, value_name = "MODEL")]
    pub expected_model: Option<String>,
    /// Opt into the experimental collector for a bound account (undocumented endpoint, Jackin identity); policy is unchanged.
    #[arg(long)]
    pub experimental_collector: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageBindingArgs {
    #[command(subcommand)]
    pub command: UsageBindingCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageBindingCommand {
    /// Confirm that one local account label represents the selected provider account
    Confirm(UsageBindingConfirmArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageBindingConfirmArgs {
    #[arg(long, value_enum, required = true)]
    pub provider: UsageProviderArg,
    #[arg(long, value_name = "ACCOUNT", required = true)]
    pub account: String,
    /// Canonical local provider source-scope account ID to bind.
    #[arg(long, value_name = "CANONICAL_LOCAL_ID")]
    pub provider_account: Option<String>,
    /// Approve the mapped source for the undocumented experimental Claude collector.
    #[arg(long)]
    pub approve_experimental_collector: bool,
    #[arg(long, value_name = "LABEL", required = true)]
    pub operator_label: String,
    /// Confirm this operator-supplied account binding
    #[arg(long, required = true)]
    pub confirm: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum UsageMonitorPolicyArg {
    StrictSgd,
    QuotaOnly,
}

impl From<UsageMonitorPolicyArg> for MonitorPolicy {
    fn from(value: UsageMonitorPolicyArg) -> Self {
        match value {
            UsageMonitorPolicyArg::StrictSgd => Self::StrictSgd,
            UsageMonitorPolicyArg::QuotaOnly => Self::QuotaOnly,
        }
    }
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsagePolicyArgs {
    #[command(subcommand)]
    pub command: UsagePolicyCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsagePolicyCommand {
    /// Explicitly approve a durable policy for one bound account and goal
    Approve(UsagePolicyApproveArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsagePolicyApproveArgs {
    #[arg(long, value_name = "ID", required = true)]
    pub binding: String,
    #[arg(long, value_name = "REVISION", required = true)]
    pub binding_revision: u64,
    #[arg(long, value_name = "GOAL", required = true)]
    pub goal: String,
    #[arg(long, value_enum, required = true)]
    pub policy: UsageMonitorPolicyArg,
    /// SGD budget ceiling for strict-sgd, e.g. `50.25` (default `50`)
    #[arg(long, value_name = "SGD", value_parser = parse_sgd_budget_arg)]
    pub budget_sgd: Option<String>,
    #[arg(long, value_name = "LABEL", required = true)]
    pub operator_label: String,
    /// Confirm this persisted policy change
    #[arg(long, required = true)]
    pub confirm: bool,
    /// Acknowledge that quota-only has no SGD spend cap
    #[arg(long)]
    pub acknowledge_no_sgd_cap: bool,
    #[arg(long, value_name = "REVISION")]
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageMonitorIdArgs {
    #[arg(long, value_name = "ID", required = true)]
    pub monitor: String,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageWatchArgs {
    #[arg(long, value_name = "ID", required = true)]
    pub monitor: String,
    /// Bound watch duration; without this option, stream until interrupted
    #[arg(long, value_name = "SECONDS", value_parser = clap::value_parser!(u64).range(1..=300))]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum UsageWaitCondition {
    Runnable,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageWaitArgs {
    #[arg(long, value_name = "ID", required = true)]
    pub monitor: String,
    #[arg(long, value_enum, required = true)]
    pub until: UsageWaitCondition,
    /// Maximum wait duration, clamped to five minutes
    #[arg(long = "timeout-secs", default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=300))]
    pub timeout_secs: u64,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageStatuslineArgs {
    #[command(subcommand)]
    pub command: UsageStatuslineCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageStatuslineCommand {
    /// Ingest one bounded JSON document from stdin
    Ingest(UsageStatuslineIngestArgs),
    /// Print a statusLine-only JSON merge patch; merge it into settings without replacing the file
    Compose(UsageStatuslineComposeArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageStatuslineIngestArgs {
    #[command(flatten)]
    pub scope: UsageStatuslineScopeArgs,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageStatuslineComposeArgs {
    #[arg(long, value_name = "PATH", required = true)]
    pub settings: PathBuf,
    #[command(flatten)]
    pub scope: UsageStatuslineScopeArgs,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageStatuslineScopeArgs {
    /// Ingest this callback only under its session ID from the JSON payload
    #[arg(long, required_unless_present = "binding", conflicts_with = "binding")]
    pub session_only: bool,
    /// Previously confirmed account binding
    #[arg(long, value_name = "ID", requires = "binding_revision")]
    pub binding: Option<String>,
    #[arg(long, value_name = "REVISION", requires = "binding")]
    pub binding_revision: Option<u64>,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageSpendArgs {
    #[command(subcommand)]
    pub command: UsageSpendCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageSpendCommand {
    /// Record a bounded JSON spend observation from an operator-supplied file
    Record(UsageSpendRecordArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageSpendRecordArgs {
    #[arg(long, value_name = "ID", required = true)]
    pub account: String,
    #[arg(long, value_name = "PATH", required = true)]
    pub file: PathBuf,
    /// Operator attestation only; this does not prove a provider billing record
    #[arg(long)]
    pub verified: bool,
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
    let paths = with_data_dir(paths, args.data_dir.as_deref());
    let Some(instance) = args.instance.as_deref() else {
        if let Some(scope) = args.scope.as_ref() {
            return run_local_scope(&paths, scope);
        }
        return run_bare_host(args, &paths);
    };
    if instance == "cache" {
        return run_cache(args, &paths).await;
    }
    let scope = args
        .scope
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing usage scope; choose `accounts` or `verify`"))?;
    if !matches!(scope, UsageScope::Accounts(_) | UsageScope::Verify) {
        return Err(usage_error(
            "invalid_scope",
            "monitor, binding, policy, service, statusline, spend, auth, and doctor commands do not take an instance",
            3,
        ));
    }
    let target = resolve_usage_target(&paths, instance)?;
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
            run_accounts(args, &paths, &target, &container, scope_args).await
        }
        UsageScope::Verify => run_verify(&paths, &target, &container),
        _ => unreachable!("non-instance usage scope checked above"),
    }
}

fn with_data_dir(paths: &JackinPaths, data_dir: Option<&std::path::Path>) -> JackinPaths {
    let mut paths = paths.clone();
    if let Some(data_dir) = data_dir {
        paths.data_dir = data_dir.to_path_buf();
    }
    paths
}

/// Bare `usage` only reads an already-published broker projection. It never
/// starts a service, discovers accounts, resolves credentials, or refreshes.
fn run_bare_host(args: &UsageArgs, paths: &JackinPaths) -> Result<()> {
    let client =
        jackin_usage::host::UsageBrokerConfig::for_data_dir(paths.data_dir.clone()).client();
    let projection = client.current_projection().map_err(|error| {
        usage_error(
            "broker_unavailable",
            &format!("cached usage projection is unavailable ({error:?})"),
            3,
        )
    })?;
    if args.output_format() == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&projection)?);
    } else {
        print_bare_host_projection(&projection);
    }
    Ok(())
}

fn print_bare_host_projection(projection: &UsageProjectionV1) {
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
        }
    }
    for unresolved in &projection.unresolved {
        println!("{} · {:?}", unresolved.provider_id, unresolved.state);
    }
}

fn run_local_scope(paths: &JackinPaths, scope: &UsageScope) -> Result<()> {
    match scope {
        UsageScope::Accounts(_) | UsageScope::Verify => Err(usage_error(
            "invalid_scope",
            "account and verify scopes require an instance name; use `usage cache accounts` for the host cache",
            3,
        )),
        UsageScope::Doctor(command) => run_doctor(paths, command),
        UsageScope::Service(command) => run_service(paths, command),
        UsageScope::Monitor(command) => run_monitor(paths, command),
        UsageScope::Binding(command) => run_binding(paths, command),
        UsageScope::Policy(command) => run_policy(paths, command),
        UsageScope::Status(command) => run_monitor_read(
            paths,
            MonitorOperation::Status {
                monitor_id: command.monitor.clone(),
            },
        ),
        UsageScope::Refresh(command) => run_monitor_read(
            paths,
            MonitorOperation::Refresh {
                monitor_id: command.monitor.clone(),
            },
        ),
        UsageScope::Watch(command) => run_watch(paths, command),
        UsageScope::Wait(command) => run_wait(paths, command),
        UsageScope::Statusline(command) => run_statusline(paths, command),
        UsageScope::Spend(command) => run_spend(paths, command),
        UsageScope::Auth(command) => auth::run_auth(paths, command),
    }
}

fn broker_config(paths: &JackinPaths) -> jackin_usage::host::UsageBrokerConfig {
    jackin_usage::host::UsageBrokerConfig::for_data_dir(paths.data_dir.clone())
}

fn discovery_scope(paths: &JackinPaths) -> jackin_usage::host::UsageDiscoveryScope {
    jackin_usage::host::UsageDiscoveryScope::HostDesktop {
        config_root: paths.config_dir.clone(),
        operator_home: paths.home_dir.clone(),
    }
}

fn start_broker(paths: &JackinPaths) -> Result<jackin_usage::host::UsageBrokerClient> {
    jackin_usage::host::ensure_usage_broker_process(broker_config(paths), &discovery_scope(paths))
        .map_err(|error| usage_error("broker_unavailable", &error.message, 3))
}

fn attach_client(paths: &JackinPaths) -> jackin_usage::host::UsageBrokerClient {
    broker_config(paths).client()
}

fn emit_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}

fn run_doctor(paths: &JackinPaths, command: &UsageDoctorArgs) -> Result<()> {
    let reply = attach_client(paths)
        .monitor(MonitorOperation::Doctor {
            provider: command.provider.into(),
        })
        .map_err(|issue| issue_error(issue, 3))?;
    if !matches!(&reply, MonitorReply::Doctor { .. }) {
        return Err(usage_error(
            "unexpected_reply",
            "broker returned a non-doctor reply",
            3,
        ));
    }
    let status = doctor_exit_code(&reply);
    if status == 0 {
        emit_json(&reply)
    } else {
        Err(json_value_exit(&reply, status))
    }
}

fn doctor_exit_code(reply: &MonitorReply) -> i32 {
    match reply {
        MonitorReply::Doctor { report } if !report.broker_available => 3,
        MonitorReply::Doctor { report }
            if report.issues.iter().any(|issue| {
                !matches!(
                    issue.code,
                    MonitorIssueCode::AuthStatusUnknown
                        | MonitorIssueCode::IndependentRefreshDisabled
                )
            }) =>
        {
            2
        }
        _ => 0,
    }
}

fn run_service(paths: &JackinPaths, command: &UsageServiceArgs) -> Result<()> {
    let reply = match &command.command {
        UsageServiceCommand::Start => start_broker(paths)?.monitor(MonitorOperation::ServiceStatus),
        UsageServiceCommand::Stop => attach_client(paths).monitor(MonitorOperation::ServiceStop),
        UsageServiceCommand::Status => {
            attach_client(paths).monitor(MonitorOperation::ServiceStatus)
        }
    }
    .map_err(|issue| issue_error(issue, 3))?;
    let expected_reply = match &command.command {
        UsageServiceCommand::Stop => matches!(&reply, MonitorReply::ServiceStopped),
        UsageServiceCommand::Start | UsageServiceCommand::Status => {
            matches!(&reply, MonitorReply::ServiceStatus { .. })
        }
    };
    if !expected_reply {
        return Err(usage_error(
            "unexpected_reply",
            "broker returned a mismatched service reply",
            3,
        ));
    }
    let status = match &reply {
        MonitorReply::ServiceStatus { status } if !status.running => 2,
        _ => 0,
    };
    if status == 0 {
        emit_json(&reply)
    } else {
        Err(json_value_exit(&reply, status))
    }
}

fn run_monitor(paths: &JackinPaths, command: &UsageMonitorArgs) -> Result<()> {
    match &command.command {
        UsageMonitorCommand::Observe(args) => {
            validate_experimental_collector_scope(
                args.experimental_collector,
                args.binding.as_deref(),
            )?;
            let scope = monitor_scope_from_selection(
                args.binding.as_deref(),
                args.binding_revision,
                args.session.as_deref(),
            )?;
            validate_monitor_start_fields(&args.idempotency_key, args.expected_model.as_deref())?;
            let client = if args.experimental_collector {
                let client = attach_client(paths);
                require_experimental_collector_service(&client)?;
                client
            } else {
                start_broker(paths)?
            };
            let reply = client
                .monitor(MonitorOperation::Start {
                    config: MonitorConfig {
                        provider: args.provider.into(),
                        purpose: MonitorPurpose::ObserveOnly,
                        scope,
                        goal_id: None,
                        expected_model: args.expected_model.clone(),
                        policy_revision: None,
                        experimental_collector: args.experimental_collector,
                    },
                    idempotency_key: args.idempotency_key.clone(),
                })
                .map_err(|issue| issue_error(issue, 3))?;
            emit_monitor_reply(&reply)
        }
        UsageMonitorCommand::Start(args) => {
            let scope = monitor_scope_from_selection(
                Some(&args.binding),
                Some(args.binding_revision),
                args.session.as_deref(),
            )?;
            validate_monitor_goal_id("goal", &args.goal)?;
            validate_monitor_revision("policy revision", args.policy_revision)?;
            validate_monitor_start_fields(&args.idempotency_key, args.expected_model.as_deref())?;
            let client = start_broker(paths)?;
            let reply = client
                .monitor(MonitorOperation::Start {
                    config: MonitorConfig {
                        provider: args.provider.into(),
                        purpose: MonitorPurpose::DispatchGuard,
                        scope,
                        goal_id: Some(args.goal.clone()),
                        expected_model: args.expected_model.clone(),
                        policy_revision: Some(args.policy_revision),
                        experimental_collector: false,
                    },
                    idempotency_key: args.idempotency_key.clone(),
                })
                .map_err(|issue| issue_error(issue, 3))?;
            emit_monitor_reply(&reply)
        }
        UsageMonitorCommand::Stop(args) => {
            validate_monitor_identifier("monitor ID", &args.monitor)?;
            let reply = attach_client(paths)
                .monitor(MonitorOperation::Stop {
                    monitor_id: args.monitor.clone(),
                })
                .map_err(|issue| issue_error(issue, 3))?;
            emit_json(&reply)
        }
    }
}

fn monitor_scope_from_selection(
    binding_id: Option<&str>,
    binding_revision: Option<u64>,
    session_id: Option<&str>,
) -> Result<MonitorScope> {
    if let Some(session_id) = session_id {
        validate_monitor_identifier("session ID", session_id)?;
    }
    match (binding_id, binding_revision) {
        (Some(binding_id), Some(binding_revision)) => {
            validate_monitor_identifier("binding ID", binding_id)?;
            validate_monitor_revision("binding revision", binding_revision)?;
            Ok(MonitorScope::BoundAccount {
                binding_id: binding_id.to_owned(),
                binding_revision,
                session_id: session_id.map(str::to_owned),
            })
        }
        (None, None) => {
            let session_id = session_id.ok_or_else(|| {
                usage_error(
                    "invalid_argument",
                    "choose a session ID or a binding and binding revision",
                    3,
                )
            })?;
            Ok(MonitorScope::Session {
                session_id: session_id.to_owned(),
            })
        }
        _ => Err(usage_error(
            "invalid_argument",
            "binding ID and binding revision must be supplied together",
            3,
        )),
    }
}

fn validate_experimental_collector_scope(enabled: bool, binding_id: Option<&str>) -> Result<()> {
    if enabled && binding_id.is_none() {
        return Err(usage_error(
            "invalid_argument",
            "the experimental collector requires an account-bound observer",
            3,
        ));
    }
    Ok(())
}

fn require_experimental_collector_service(
    client: &jackin_usage::host::UsageBrokerClient,
) -> Result<()> {
    let reply = client
        .monitor(MonitorOperation::ServiceStatus)
        .map_err(|issue| {
            if issue.code == MonitorIssueCode::BrokerUnavailable {
                collector_auth_required_error()
            } else {
                issue_error(issue, 3)
            }
        })?;
    let MonitorReply::ServiceStatus { status } = reply else {
        return Err(usage_error(
            "unexpected_reply",
            "broker returned a non-service reply while checking collector mode",
            3,
        ));
    };
    if foreground_experimental_collector_source(&status).is_some() {
        Ok(())
    } else {
        Err(collector_auth_required_error())
    }
}

fn foreground_experimental_collector_source(status: &MonitorServiceStatus) -> Option<&str> {
    status
        .running
        .then_some(status.experimental_collector_source.as_deref())
        .flatten()
        .filter(|source| {
            validate_monitor_identifier("experimental collector source ID", source).is_ok()
        })
}

fn collector_auth_required_error() -> anyhow::Error {
    issue_error(
        MonitorIssue {
            code: MonitorIssueCode::CollectorAuthRequired,
            message: "experimental collection requires a running foreground `usage auth prepare` service for the mapped Claude source; this command does not start or prepare credentials".to_owned(),
            retry_at_epoch: None,
        },
        3,
    )
}

fn run_binding(paths: &JackinPaths, command: &UsageBindingArgs) -> Result<()> {
    match &command.command {
        UsageBindingCommand::Confirm(args) => {
            let binding = binding_confirmation_input(args)?;
            require_operator_confirmation_terminal()?;
            let reply = attach_client(paths)
                .monitor(MonitorOperation::BindAccount { binding })
                .map_err(|issue| issue_error(issue, 3))?;
            if !matches!(&reply, MonitorReply::AccountBound { .. }) {
                return Err(usage_error(
                    "unexpected_reply",
                    "broker returned a non-binding reply",
                    3,
                ));
            }
            emit_json(&reply)
        }
    }
}

fn binding_confirmation_input(
    args: &UsageBindingConfirmArgs,
) -> Result<MonitorAccountBindingInput> {
    validate_monitor_identifier("account ID", &args.account)?;
    if let Some(provider_account) = args.provider_account.as_deref() {
        validate_monitor_identifier("canonical local source account ID", provider_account)?;
    }
    if args.approve_experimental_collector && args.provider_account.is_none() {
        return Err(usage_error(
            "invalid_argument",
            "--approve-experimental-collector requires --provider-account",
            3,
        ));
    }
    validate_monitor_operator_label(&args.operator_label)?;
    if !args.confirm {
        return Err(usage_error(
            "confirmation_required",
            "account binding requires the explicit --confirm flag",
            2,
        ));
    }
    Ok(MonitorAccountBindingInput {
        provider: args.provider.into(),
        account_id: args.account.clone(),
        provider_account_id: args.provider_account.clone(),
        experimental_collector_approved: args.approve_experimental_collector,
        operator_label: args.operator_label.clone(),
        operator_confirmed: true,
    })
}

fn run_policy(paths: &JackinPaths, command: &UsagePolicyArgs) -> Result<()> {
    match &command.command {
        UsagePolicyCommand::Approve(args) => {
            let approval = policy_approval_input(args)?;
            require_operator_confirmation_terminal()?;
            let reply = attach_client(paths)
                .monitor(MonitorOperation::ApprovePolicy { approval })
                .map_err(|issue| issue_error(issue, 3))?;
            if !matches!(&reply, MonitorReply::PolicyApproved { .. }) {
                return Err(usage_error(
                    "unexpected_reply",
                    "broker returned a non-policy reply",
                    3,
                ));
            }
            emit_json(&reply)
        }
    }
}

fn policy_approval_input(args: &UsagePolicyApproveArgs) -> Result<MonitorPolicyApprovalInput> {
    validate_monitor_identifier("binding ID", &args.binding)?;
    validate_monitor_revision("binding revision", args.binding_revision)?;
    validate_monitor_goal_id("goal ID", &args.goal)?;
    validate_monitor_operator_label(&args.operator_label)?;
    if let Some(expected_revision) = args.expected_revision {
        validate_monitor_revision("expected policy revision", expected_revision)?;
    }
    if !args.confirm {
        return Err(usage_error(
            "confirmation_required",
            "policy approval requires the explicit --confirm flag",
            2,
        ));
    }
    let (new_policy, budget, acknowledge_no_sgd_cap) = match args.policy {
        UsageMonitorPolicyArg::StrictSgd => {
            if args.acknowledge_no_sgd_cap {
                return Err(usage_error(
                    "invalid_argument",
                    "--acknowledge-no-sgd-cap applies only to quota-only policy",
                    3,
                ));
            }
            let budget = parse_sgd_budget(args.budget_sgd.as_deref().unwrap_or("50"))?;
            (MonitorPolicy::StrictSgd, Some(budget), false)
        }
        UsageMonitorPolicyArg::QuotaOnly => {
            if args.budget_sgd.is_some() {
                return Err(usage_error(
                    "invalid_argument",
                    "--budget-sgd applies only to strict-sgd policy",
                    3,
                ));
            }
            if !args.acknowledge_no_sgd_cap {
                return Err(usage_error(
                    "confirmation_required",
                    "quota-only policy requires --acknowledge-no-sgd-cap",
                    2,
                ));
            }
            (MonitorPolicy::QuotaOnly, None, true)
        }
    };
    Ok(MonitorPolicyApprovalInput {
        binding_id: args.binding.clone(),
        binding_revision: args.binding_revision,
        goal_id: args.goal.clone(),
        new_policy,
        budget,
        operator_label: args.operator_label.clone(),
        operator_confirmed: true,
        acknowledge_no_sgd_cap,
        expected_revision: args.expected_revision,
    })
}

fn require_operator_confirmation_terminal() -> Result<()> {
    use std::io::IsTerminal as _;
    require_operator_terminal(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        std::io::stderr().is_terminal(),
    )
}

fn require_operator_terminal(stdin: bool, stdout: bool, stderr: bool) -> Result<()> {
    if all_stdio_are_terminal(stdin, stdout, stderr) {
        return Ok(());
    }
    Err(issue_error(
        MonitorIssue {
            code: MonitorIssueCode::InteractionRequired,
            message: "binding confirmation and policy approval require attached stdin, stdout, and stderr terminals".to_owned(),
            retry_at_epoch: None,
        },
        2,
    ))
}

fn run_monitor_read(paths: &JackinPaths, operation: MonitorOperation) -> Result<()> {
    let monitor_id = match &operation {
        MonitorOperation::Status { monitor_id } | MonitorOperation::Refresh { monitor_id } => {
            monitor_id
        }
        _ => {
            return Err(usage_error(
                "invalid_argument",
                "monitor read requires a status or refresh operation",
                3,
            ));
        }
    };
    validate_monitor_identifier("monitor ID", monitor_id)?;
    let expects_refresh = matches!(&operation, MonitorOperation::Refresh { .. });
    let reply = attach_client(paths)
        .monitor(operation)
        .map_err(|issue| issue_error(issue, 3))?;
    let expected_reply = if expects_refresh {
        matches!(&reply, MonitorReply::Refreshed { .. })
    } else {
        matches!(&reply, MonitorReply::Status { .. })
    };
    if !expected_reply {
        return Err(usage_error(
            "unexpected_reply",
            "broker returned a mismatched monitor reply",
            3,
        ));
    }
    emit_monitor_reply(&reply)
}

fn emit_monitor_reply(reply: &MonitorReply) -> Result<()> {
    let code = match reply_status(reply) {
        Some(status) => monitor_status_exit_code(
            status.purpose,
            status.runnable,
            status.readiness.tracking,
            matches!(reply, MonitorReply::Started { .. }),
        ),
        None => {
            return Err(usage_error(
                "unexpected_reply",
                "broker returned a reply without monitor status",
                3,
            ));
        }
    };
    if code == 0 {
        emit_json(reply)
    } else {
        Err(json_value_exit(reply, code))
    }
}

fn monitor_status_exit_code(
    purpose: MonitorPurpose,
    runnable: bool,
    tracking: MonitorTrackingReadiness,
    is_start_reply: bool,
) -> i32 {
    if tracking == MonitorTrackingReadiness::Unavailable {
        3
    } else if (is_start_reply && purpose == MonitorPurpose::ObserveOnly) || runnable {
        0
    } else {
        2
    }
}

fn reply_status(reply: &MonitorReply) -> Option<&MonitorStatus> {
    match reply {
        MonitorReply::Started { status }
        | MonitorReply::Stopped { status }
        | MonitorReply::Status { status }
        | MonitorReply::Refreshed { status } => Some(status),
        _ => None,
    }
}

fn run_watch(paths: &JackinPaths, args: &UsageWatchArgs) -> Result<()> {
    validate_monitor_identifier("monitor ID", &args.monitor)?;
    let client = attach_client(paths);
    let deadline = args
        .timeout_secs
        .map(|timeout| Instant::now() + Duration::from_secs(timeout.min(300)));
    // The broker treats cursor zero as a fresh attach: it reconciles the
    // monitor and returns only the newest current event, never the retained
    // history. Later requests continue from the cursor returned with that
    // current snapshot, so old runnable events cannot act as current state.
    let mut sequence = 0_u64;
    loop {
        let timeout_ms = deadline.map_or(30_000, |deadline| {
            u64::try_from(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis(),
            )
            .unwrap_or(u64::MAX)
            .min(30_000)
        });
        if timeout_ms == 0 {
            return Ok(());
        }
        let reply = client
            .monitor(MonitorOperation::Watch {
                monitor_id: args.monitor.clone(),
                after_sequence: sequence,
                timeout_ms,
            })
            .map_err(|issue| issue_error(issue, 3))?;
        let MonitorReply::Watch {
            events,
            next_sequence,
            timed_out,
        } = reply
        else {
            return Err(usage_error(
                "unexpected_reply",
                "broker returned a non-watch reply",
                3,
            ));
        };
        let after_sequence = sequence;
        sequence = advance_watch_cursor(
            sequence,
            next_sequence,
            events.iter().map(|event| event.sequence),
        );
        for event in events {
            if event.sequence <= after_sequence {
                continue;
            }
            println!("{}", serde_json::to_string(&event)?);
        }
        if timed_out && deadline.is_some() {
            return Ok(());
        }
    }
}

fn advance_watch_cursor(
    current: u64,
    next_sequence: u64,
    event_sequences: impl IntoIterator<Item = u64>,
) -> u64 {
    event_sequences
        .into_iter()
        .fold(current.max(next_sequence), u64::max)
}

fn run_wait(paths: &JackinPaths, args: &UsageWaitArgs) -> Result<()> {
    match args.until {
        UsageWaitCondition::Runnable => {
            run_wait_until_runnable(paths, &args.monitor, args.timeout_secs)
        }
    }
}

fn run_wait_until_runnable(
    paths: &JackinPaths,
    monitor_id: &str,
    timeout_seconds: u64,
) -> Result<()> {
    validate_monitor_identifier("monitor ID", monitor_id)?;
    let client = attach_client(paths);
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds.min(300));
    let mut sequence = 0_u64;
    loop {
        let status_reply = client
            .monitor(MonitorOperation::Status {
                monitor_id: monitor_id.to_owned(),
            })
            .map_err(|issue| issue_error(issue, 3))?;
        let MonitorReply::Status { status } = status_reply else {
            return Err(usage_error(
                "unexpected_reply",
                "broker returned a non-status reply",
                3,
            ));
        };
        if status.runnable {
            return emit_monitor_reply(&MonitorReply::Status { status });
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let timeout_ms = u64::try_from(remaining.as_millis())
            .unwrap_or(u64::MAX)
            .min(1_000);
        let reply = client
            .monitor(MonitorOperation::Watch {
                monitor_id: monitor_id.to_owned(),
                after_sequence: sequence,
                timeout_ms,
            })
            .map_err(|issue| issue_error(issue, 3))?;
        match reply {
            MonitorReply::Watch {
                events,
                next_sequence,
                ..
            } => {
                sequence = sequence.max(next_sequence);
                for event in events {
                    sequence = sequence.max(event.sequence);
                }
            }
            _ => {
                return Err(usage_error(
                    "unexpected_reply",
                    "broker returned a non-watch reply",
                    3,
                ));
            }
        }
    }
    let reply = client
        .monitor(MonitorOperation::Status {
            monitor_id: monitor_id.to_owned(),
        })
        .map_err(|issue| issue_error(issue, 3))?;
    let MonitorReply::Status { mut status } = reply else {
        return Err(usage_error(
            "unexpected_reply",
            "broker returned a non-status reply",
            3,
        ));
    };
    if status.runnable {
        return emit_monitor_reply(&MonitorReply::Status { status });
    }
    append_wait_timeout_issue(&mut status.issues);
    Err(json_value_exit(&MonitorReply::Status { status }, 2))
}

fn append_wait_timeout_issue(issues: &mut Vec<MonitorIssue>) {
    issues.push(MonitorIssue {
        code: MonitorIssueCode::WaitTimeout,
        message: "wait for runnable evidence expired before the monitor became runnable".to_owned(),
        retry_at_epoch: None,
    });
}

fn run_statusline(paths: &JackinPaths, command: &UsageStatuslineArgs) -> Result<()> {
    match &command.command {
        UsageStatuslineCommand::Ingest(args) => {
            let mut bytes = Vec::with_capacity(USAGE_MONITOR_MAX_STATUSLINE_BYTES + 1);
            std::io::stdin()
                .take((USAGE_MONITOR_MAX_STATUSLINE_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .context("read statusline JSON from stdin")
                .map_err(|error| usage_error("statusline_invalid", &error.to_string(), 3))?;
            let observation = parse_statusline(&bytes).map_err(|issue| issue_error(issue, 3))?;
            let scope = statusline_monitor_scope(&args.scope, &observation.session_id)?;
            let reply = attach_client(paths)
                .monitor(MonitorOperation::Ingest { scope, observation })
                .map_err(|issue| issue_error(issue, 3))?;
            if !matches!(&reply, MonitorReply::Ingested { .. }) {
                return Err(usage_error(
                    "unexpected_reply",
                    "broker returned a non-ingest reply",
                    3,
                ));
            }
            emit_json(&reply)
        }
        UsageStatuslineCommand::Compose(args) => {
            if let Some(binding_id) = args.scope.binding.as_deref() {
                validate_monitor_identifier("binding ID", binding_id)?;
            }
            if let Some(binding_revision) = args.scope.binding_revision {
                validate_monitor_revision("binding revision", binding_revision)?;
            }
            let binary = std::env::current_exe()
                .map_err(|error| usage_error("path_unavailable", &error.to_string(), 3))?;
            let proposed =
                statusline::compose(&args.settings, &binary, &args.scope, &paths.data_dir)
                    .map_err(|error| usage_error("compose_failed", &format!("{error:#}"), 3))?;
            println!("{}", serde_json::to_string_pretty(&proposed)?);
            Ok(())
        }
    }
}

fn statusline_monitor_scope(
    selection: &UsageStatuslineScopeArgs,
    payload_session_id: &str,
) -> Result<MonitorScope> {
    if selection.session_only {
        if selection.binding.is_some() || selection.binding_revision.is_some() {
            return Err(usage_error(
                "invalid_argument",
                "choose exactly `--session-only` or `--binding` with `--binding-revision`",
                3,
            ));
        }
        validate_monitor_identifier("statusline payload session ID", payload_session_id)?;
        return Ok(MonitorScope::Session {
            session_id: payload_session_id.to_owned(),
        });
    }
    monitor_scope_from_selection(
        selection.binding.as_deref(),
        selection.binding_revision,
        None,
    )
}

fn run_spend(paths: &JackinPaths, command: &UsageSpendArgs) -> Result<()> {
    match &command.command {
        UsageSpendCommand::Record(args) => {
            let record = read_spend_record(&args.file, &args.account, args.verified)?;
            let reply = attach_client(paths)
                .monitor(MonitorOperation::RecordSpend { record })
                .map_err(|issue| issue_error(issue, 3))?;
            if !matches!(&reply, MonitorReply::SpendRecorded { .. }) {
                return Err(usage_error(
                    "unexpected_reply",
                    "broker returned a non-spend reply",
                    3,
                ));
            }
            emit_json(&reply)
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpendEvidenceFile {
    billing_period_start_epoch: i64,
    billing_period_end_epoch: i64,
    amount: Money,
    evidence_at_epoch: Option<i64>,
}

#[expect(
    clippy::disallowed_methods,
    reason = "bounded synchronous CLI input read outside render/runtime threads"
)]
fn read_spend_record(
    path: &std::path::Path,
    account: &str,
    verified: bool,
) -> Result<SpendRecordInput> {
    const MAX_SPEND_FILE_BYTES: usize = 16 * 1024;
    let file = std::fs::File::open(path)
        .with_context(|| format!("open spend evidence file {}", path.display()))
        .map_err(|error| usage_error("spend_file_invalid", &format!("{error:#}"), 3))?;
    let mut bytes = Vec::with_capacity(MAX_SPEND_FILE_BYTES + 1);
    file.take((MAX_SPEND_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| usage_error("spend_file_invalid", &error.to_string(), 3))?;
    if bytes.len() > MAX_SPEND_FILE_BYTES {
        return Err(usage_error(
            "spend_file_too_large",
            "spend evidence file exceeds 16 KiB",
            3,
        ));
    }
    let evidence: SpendEvidenceFile = serde_json::from_slice(&bytes)
        .map_err(|error| usage_error("spend_file_invalid", &error.to_string(), 3))?;
    if account.trim().is_empty()
        || evidence.billing_period_start_epoch >= evidence.billing_period_end_epoch
        || evidence.amount.amount_minor < 0
        || evidence.amount.currency.trim().is_empty()
    {
        return Err(usage_error(
            "spend_file_invalid",
            "account, billing period, and nonnegative monetary amount must be valid",
            3,
        ));
    }
    Ok(SpendRecordInput {
        account_id: account.to_owned(),
        billing_period_start_epoch: evidence.billing_period_start_epoch,
        billing_period_end_epoch: evidence.billing_period_end_epoch,
        amount: evidence.amount,
        evidence_at_epoch: evidence.evidence_at_epoch,
        verified,
        source: SpendRecordSource::OperatorReceipt,
    })
}

fn parse_sgd_budget(value: &str) -> Result<Money> {
    let mut parts = value.split('.');
    let major = parts.next().unwrap_or_default();
    let minor = parts.next();
    if parts.next().is_some()
        || major.is_empty()
        || !major.bytes().all(|byte| byte.is_ascii_digit())
        || minor
            .is_some_and(|part| part.len() > 2 || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(usage_error(
            "invalid_budget",
            "SGD budget must be a positive amount with at most two decimal places",
            3,
        ));
    }
    let major = major
        .parse::<i64>()
        .map_err(|_| usage_error("invalid_budget", "SGD budget is out of range", 3))?;
    let minor = minor.unwrap_or_default();
    let minor = if minor.is_empty() {
        0
    } else if minor.len() == 1 {
        minor.parse::<i64>().unwrap_or(0) * 10
    } else {
        minor.parse::<i64>().unwrap_or(0)
    };
    let amount_minor = major
        .checked_mul(100)
        .and_then(|amount| amount.checked_add(minor))
        .ok_or_else(|| usage_error("invalid_budget", "SGD budget is out of range", 3))?;
    if amount_minor == 0 {
        return Err(usage_error(
            "invalid_budget",
            "SGD budget must be a positive amount with at most two decimal places",
            3,
        ));
    }
    Ok(Money::new(amount_minor, "SGD", 2))
}

fn parse_sgd_budget_arg(value: &str) -> std::result::Result<String, String> {
    parse_sgd_budget(value)
        .map(|_| value.to_owned())
        .map_err(|_| {
            "SGD budget must be a positive amount with at most two decimal places".to_owned()
        })
}

fn parse_statusline(bytes: &[u8]) -> Result<StatuslineObservation, MonitorIssue> {
    jackin_usage::host::parse_statusline(bytes)
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
        _ => Err(usage_error(
            "invalid_scope",
            "this usage command cannot be scoped to the host cache",
            3,
        )),
    }
}

async fn run_accounts(
    args: &UsageArgs,
    paths: &JackinPaths,
    target: &UsageTarget,
    container: &jackin_core::ContainerHandle,
    scope_args: &UsageAccountsArgs,
) -> Result<()> {
    let accounts = snapshot::fetch_usage_accounts(paths, container)?.unwrap_or_default();
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

fn run_verify(
    paths: &JackinPaths,
    target: &UsageTarget,
    container: &jackin_core::ContainerHandle,
) -> Result<()> {
    let accounts = snapshot::fetch_usage_accounts(paths, container)?.unwrap_or_default();
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

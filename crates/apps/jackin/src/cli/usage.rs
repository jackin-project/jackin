// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use jackin_protocol::control::{
    AccountUsageSnapshotView, CountQuotaUnit, PercentObservationOrigin, ProviderQuotaState,
};
use jackin_protocol::usage_broker::UsageProjectionV1;
use jackin_protocol::usage_monitor::{
    MonitorAccountBindingInput, MonitorConfig, MonitorIssue, MonitorIssueCode, MonitorOperation,
    MonitorPolicy, MonitorPolicyApprovalInput, MonitorProvider, MonitorPurpose, MonitorReply,
    MonitorScope, MonitorStatus, MonitorTrackingReadiness, StatuslineObservation,
    USAGE_MONITOR_MAX_STATUSLINE_BYTES,
};
use serde::Serialize;
use std::io::Read as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::{ffi::OsString, io::IsTerminal as _};

use crate::cli::format::{OutputEnvelope, OutputFormat};
use crate::cli::{BANNER, HELP_STYLES};
use jackin_core::JackinPaths;
use jackin_docker::docker_client::{BollardDockerClient, DockerApi};
use jackin_runtime::instance::{InstanceIndex, InstanceStatus};
use jackin_runtime::runtime::snapshot;

mod errors;
mod spend;
mod statusline;
mod store;
mod verify;
use errors::MonitorId;
use spend::{parse_sgd_budget, parse_sgd_budget_arg, read_spend_record};

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Read broker-owned cached usage or manage durable Claude quota monitors",
    long_about = "Read the broker-owned cached usage projection without starting a broker or refreshing a provider.\n\n\
        Explicit monitor and service start commands may start a local-only broker. Monitor state and policy decisions are durable.\n\
        All monitor evidence is local; statusline observations do not contain spend."
)]
#[command(
    about = "`jackin usage` — broker-owned cached usage and durable monitor operations.",
    long_about = "`jackin usage` — broker-owned cached usage and durable monitor operations."
)]
pub struct UsageArgs {
    #[arg(
        help = "Instance name for Capsule account inspection, or `cache` for the host account cache",
        long_help = "Instance name for Capsule account inspection, or `cache` for the host account cache"
    )]
    pub instance: Option<String>,
    #[command(subcommand)]
    pub scope: Option<UsageScope>,
    #[arg(
        help = "Output format",
        long_help = "Output format",
        long,
        global = true,
        value_name = "FORMAT",
        default_value = "human"
    )]
    pub format: String,
    #[arg(
        help = "Use an isolated jackin data directory for this usage operation",
        long_help = "Use an isolated jackin data directory for this usage operation",
        long,
        global = true,
        value_name = "PATH"
    )]
    pub data_dir: Option<PathBuf>,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageScope {
    #[command(about = "Show cached provider account/quota buckets", long_about = "Show cached provider account/quota buckets", before_help = BANNER, styles = HELP_STYLES)]
    Accounts(UsageAccountsArgs),
    #[command(about = "Verify all provider quota rows are present and trusted", long_about = "Verify all provider quota rows are present and trusted", before_help = BANNER, styles = HELP_STYLES)]
    Verify,
    #[command(about = "Run passive readiness checks without accessing credentials or providers", long_about = "Run passive readiness checks without accessing credentials or providers", before_help = BANNER, styles = HELP_STYLES)]
    Doctor(UsageDoctorArgs),
    #[command(about = "Manage the local-only usage broker", long_about = "Manage the local-only usage broker", before_help = BANNER, styles = HELP_STYLES)]
    Service(UsageServiceArgs),
    #[command(about = "Manage an unattended quota monitor", long_about = "Manage an unattended quota monitor", before_help = BANNER, styles = HELP_STYLES)]
    Monitor(UsageMonitorArgs),
    #[command(about = "Confirm a provider account identity binding for quota monitoring", long_about = "Confirm a provider account identity binding for quota monitoring", before_help = BANNER, styles = HELP_STYLES)]
    Binding(UsageBindingArgs),
    #[command(about = "Approve a durable quota-monitor policy for a bound account and goal", long_about = "Approve a durable quota-monitor policy for a bound account and goal", before_help = BANNER, styles = HELP_STYLES)]
    Policy(UsagePolicyArgs),
    #[command(about = "Read the status of a durable monitor", long_about = "Read the status of a durable monitor", before_help = BANNER, styles = HELP_STYLES)]
    Status(UsageMonitorIdArgs),
    #[command(about = "Reconcile a monitor against local evidence without provider refresh", long_about = "Reconcile a monitor against local evidence without provider refresh", before_help = BANNER, styles = HELP_STYLES)]
    Refresh(UsageMonitorIdArgs),
    #[command(about = "Stream new monitor events as JSON Lines", long_about = "Stream new monitor events as JSON Lines", before_help = BANNER, styles = HELP_STYLES)]
    Watch(UsageWatchArgs),
    #[command(about = "Wait for runnable evidence for a bounded time", long_about = "Wait for runnable evidence for a bounded time", before_help = BANNER, styles = HELP_STYLES)]
    Wait(UsageWaitArgs),
    #[command(about = "Ingest or compose Claude Code statusline integration", long_about = "Ingest or compose Claude Code statusline integration", before_help = BANNER, styles = HELP_STYLES)]
    Statusline(UsageStatuslineArgs),
    #[command(about = "Record an explicit account-bound spend baseline", long_about = "Record an explicit account-bound spend baseline", before_help = BANNER, styles = HELP_STYLES)]
    Spend(UsageSpendArgs),
    #[command(about = "Prepare provider authentication in an attached terminal", long_about = "Prepare provider authentication in an attached terminal", before_help = BANNER, styles = HELP_STYLES)]
    Auth(UsageAuthArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageAccountsArgs {
    #[arg(
        help = "Also upsert returned rows into ~/.jackin/data/daemon/accounts.db.",
        long_help = "Also upsert returned rows into ~/.jackin/data/daemon/accounts.db.

This is an explicit host-side write for seeding the host-global usage cache before a long-running host daemon owns account refresh.",
        long
    )]
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
    #[arg(
        help = "Require a noninteractive readiness report",
        long_help = "Require a noninteractive readiness report",
        long,
        required = true
    )]
    pub unattended: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageServiceArgs {
    #[command(subcommand)]
    pub command: UsageServiceCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageServiceCommand {
    #[command(
        about = "Start the local-only broker explicitly",
        long_about = "Start the local-only broker explicitly"
    )]
    Start,
    #[command(
        about = "Stop the already-running broker",
        long_about = "Stop the already-running broker"
    )]
    Stop,
    #[command(
        about = "Read local broker status without starting it",
        long_about = "Read local broker status without starting it"
    )]
    Status,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageMonitorArgs {
    #[command(subcommand)]
    pub command: UsageMonitorCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageMonitorCommand {
    #[command(
        about = "Start an observation-only monitor and the local-only broker if needed",
        long_about = "Start an observation-only monitor and the local-only broker if needed"
    )]
    Observe(UsageMonitorObserveArgs),
    #[command(
        about = "Start dispatch guard using an already approved policy",
        long_about = "Start dispatch guard using an already approved policy"
    )]
    Start(UsageMonitorStartArgs),
    #[command(about = "Stop one monitor", long_about = "Stop one monitor")]
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
    #[arg(
        help = "Unbound observer scope. If omitted, pass `--binding` and its revision.",
        long_help = "Unbound observer scope. If omitted, pass `--binding` and its revision.",
        long,
        value_name = "ID",
        required_unless_present = "binding"
    )]
    pub session: Option<String>,
    #[arg(
        help = "Previously confirmed account binding",
        long_help = "Previously confirmed account binding",
        long,
        value_name = "ID",
        requires = "binding_revision"
    )]
    pub binding: Option<String>,
    #[arg(long, value_name = "REVISION", requires = "binding")]
    pub binding_revision: Option<u64>,
    #[arg(long, value_name = "KEY", required = true)]
    pub idempotency_key: String,
    #[arg(long, value_name = "MODEL")]
    pub expected_model: Option<String>,
    #[arg(
        help = "Opt into the experimental collector for a bound Claude account.",
        long_help = "Opt into the experimental collector for a bound Claude account.",
        long
    )]
    pub experimental_collector: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageBindingArgs {
    #[command(subcommand)]
    pub command: UsageBindingCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageBindingCommand {
    #[command(
        about = "Confirm that one local account label represents the selected provider account",
        long_about = "Confirm that one local account label represents the selected provider account"
    )]
    Confirm(UsageBindingConfirmArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageBindingConfirmArgs {
    #[arg(long, value_enum, required = true)]
    pub provider: UsageProviderArg,
    #[arg(long, value_name = "ACCOUNT", required = true)]
    pub account: String,
    #[arg(
        help = "Opaque source capability ID reported by the foreground service status.",
        long_help = "Opaque source capability ID reported by the foreground service status.",
        long = "source-capability-id",
        value_name = "SOURCE_ID"
    )]
    pub source_capability_id: Option<String>,
    #[arg(
        help = "Approve the mapped source for the experimental Claude collector.",
        long_help = "Approve the mapped source for the experimental Claude collector.",
        long
    )]
    pub approve_experimental_collector: bool,
    #[arg(long, value_name = "LABEL", required = true)]
    pub operator_label: String,
    #[arg(
        help = "Confirm this operator-supplied account binding",
        long_help = "Confirm this operator-supplied account binding",
        long,
        required = true
    )]
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
    #[command(
        about = "Explicitly approve a durable policy for one bound account and goal",
        long_about = "Explicitly approve a durable policy for one bound account and goal"
    )]
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
    #[arg(help = "SGD budget ceiling for strict-sgd, e.g. `50.25` (default `50`)", long_help = "SGD budget ceiling for strict-sgd, e.g. `50.25` (default `50`)", long, value_name = "SGD", value_parser = parse_sgd_budget_arg)]
    pub budget_sgd: Option<String>,
    #[arg(long, value_name = "LABEL", required = true)]
    pub operator_label: String,
    #[arg(
        help = "Confirm this persisted policy change",
        long_help = "Confirm this persisted policy change",
        long,
        required = true
    )]
    pub confirm: bool,
    #[arg(
        help = "Acknowledge that quota-only has no SGD spend cap",
        long_help = "Acknowledge that quota-only has no SGD spend cap",
        long
    )]
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
    #[arg(help = "Bound watch duration; without this option, stream until interrupted", long_help = "Bound watch duration; without this option, stream until interrupted", long, value_name = "SECONDS", value_parser = clap::value_parser!(u64).range(1..=300))]
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
    #[arg(help = "Maximum wait duration, clamped to five minutes", long_help = "Maximum wait duration, clamped to five minutes", long = "timeout-secs", default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=300))]
    pub timeout_secs: u64,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageStatuslineArgs {
    #[command(subcommand)]
    pub command: UsageStatuslineCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageStatuslineCommand {
    #[command(
        about = "Ingest one bounded JSON document from stdin",
        long_about = "Ingest one bounded JSON document from stdin"
    )]
    Ingest(UsageStatuslineIngestArgs),
    #[command(
        about = "Print a statusLine-only settings merge patch with a composed command",
        long_about = "Print a statusLine-only settings merge patch with a composed command"
    )]
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
    #[arg(
        help = "Ingest this callback only under its session ID from the JSON payload",
        long_help = "Ingest this callback only under its session ID from the JSON payload",
        long,
        required_unless_present = "binding",
        conflicts_with = "binding"
    )]
    pub session_only: bool,
    #[arg(
        help = "Previously confirmed account binding",
        long_help = "Previously confirmed account binding",
        long,
        value_name = "ID",
        requires = "binding_revision"
    )]
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
    #[command(
        about = "Record a bounded JSON spend observation from an operator-supplied file",
        long_about = "Record a bounded JSON spend observation from an operator-supplied file"
    )]
    Record(UsageSpendRecordArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageSpendRecordArgs {
    #[arg(long, value_name = "ID", required = true)]
    pub account: String,
    #[arg(long, value_name = "PATH", required = true)]
    pub file: PathBuf,
    #[arg(
        help = "Operator attestation only; this does not prove a provider billing record",
        long_help = "Operator attestation only; this does not prove a provider billing record",
        long
    )]
    pub verified: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageAuthArgs {
    #[command(subcommand)]
    pub command: UsageAuthCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageAuthCommand {
    #[command(
        about = "Explicitly request interactive authentication preparation",
        long_about = "Explicitly request interactive authentication preparation"
    )]
    Prepare(UsageProviderArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageProviderArgs {
    #[arg(long, value_enum, required = true)]
    pub provider: UsageProviderArg,
    #[arg(
        help = "Explicit Claude Keychain generic-password service name",
        long_help = "Explicit Claude Keychain generic-password service name",
        long,
        value_name = "SERVICE"
    )]
    pub keychain_service: Option<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("{json}")]
pub struct UsageCommandExit {
    exit_code: i32,
    json: String,
}

impl UsageCommandExit {
    #[must_use]
    pub fn new(exit_code: i32, json: String) -> Self {
        Self { exit_code, json }
    }

    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        self.exit_code
    }

    #[must_use]
    pub fn json(&self) -> &str {
        &self.json
    }
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
        UsageScope::Verify => verify::run_verify(&paths, &target, &container),
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
                monitor_id: MonitorId::parse(&command.monitor)?.into_string(),
            },
        ),
        UsageScope::Refresh(command) => run_monitor_read(
            paths,
            MonitorOperation::Refresh {
                monitor_id: MonitorId::parse(&command.monitor)?.into_string(),
            },
        ),
        UsageScope::Watch(command) => run_watch(paths, command),
        UsageScope::Wait(command) => run_wait(paths, command),
        UsageScope::Statusline(command) => run_statusline(paths, command),
        UsageScope::Spend(command) => run_spend(paths, command),
        UsageScope::Auth(command) => run_auth(paths, command),
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
    jackin_usage::host::ensure_usage_monitor_process(broker_config(paths), &discovery_scope(paths))
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
            validate_nonempty("idempotency key", &args.idempotency_key)?;
            validate_expected_model(args.expected_model.as_deref())?;
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
            validate_nonempty("goal", &args.goal)?;
            validate_nonempty("idempotency key", &args.idempotency_key)?;
            validate_expected_model(args.expected_model.as_deref())?;
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
            let monitor_id = MonitorId::parse(&args.monitor)?;
            let reply = attach_client(paths)
                .monitor(MonitorOperation::Stop {
                    monitor_id: monitor_id.into_string(),
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
        validate_nonempty("session ID", session_id)?;
    }
    match (binding_id, binding_revision) {
        (Some(binding_id), Some(binding_revision)) => {
            validate_nonempty("binding ID", binding_id)?;
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

fn foreground_experimental_collector_source(
    status: &jackin_protocol::usage_monitor::MonitorServiceStatus,
) -> Option<&str> {
    status
        .running
        .then_some(status.experimental_collector_source.as_deref())
        .flatten()
        .filter(|source| validate_source_capability_id(source).is_ok())
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

fn validate_expected_model(model: Option<&str>) -> Result<()> {
    if let Some(model) = model {
        validate_nonempty("expected model", model)?;
    }
    Ok(())
}

fn validate_nonempty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() || value.contains('\0') {
        return Err(usage_error(
            "invalid_argument",
            &format!("{field} must be nonempty and contain no NUL byte"),
            3,
        ));
    }
    Ok(())
}

fn validate_source_capability_id(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(usage_error(
            "invalid_argument",
            "source capability ID must be the 64-character lowercase hex value reported by foreground service status",
            3,
        ));
    }
    Ok(())
}

fn run_binding(paths: &JackinPaths, command: &UsageBindingArgs) -> Result<()> {
    match &command.command {
        UsageBindingCommand::Confirm(args) => {
            require_operator_confirmation_terminal()?;
            let binding = binding_confirmation_input(args)?;
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
    validate_nonempty("account ID", &args.account)?;
    if let Some(source_capability_id) = args.source_capability_id.as_deref() {
        validate_source_capability_id(source_capability_id)?;
    }
    if args.approve_experimental_collector && args.source_capability_id.is_none() {
        return Err(usage_error(
            "invalid_argument",
            "--approve-experimental-collector requires --source-capability-id",
            3,
        ));
    }
    validate_nonempty("operator label", &args.operator_label)?;
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
        provider_account_id: args.source_capability_id.clone(),
        experimental_collector_approved: args.approve_experimental_collector,
        operator_label: args.operator_label.clone(),
        operator_confirmed: true,
    })
}

fn run_policy(paths: &JackinPaths, command: &UsagePolicyArgs) -> Result<()> {
    match &command.command {
        UsagePolicyCommand::Approve(args) => {
            require_operator_confirmation_terminal()?;
            let approval = policy_approval_input(args)?;
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
    validate_nonempty("binding ID", &args.binding)?;
    validate_nonempty("goal ID", &args.goal)?;
    validate_nonempty("operator label", &args.operator_label)?;
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
    let monitor_id = MonitorId::parse(&args.monitor)?;
    let client = attach_client(paths);
    let deadline = args
        .timeout_secs
        .map(|timeout| Instant::now() + Duration::from_secs(timeout.min(300)));
    let mut sequence = 0_u64;
    loop {
        let timeout_ms = watch_timeout_ms(deadline, Instant::now());
        if timeout_ms == 0 {
            return Ok(());
        }
        let reply = client
            .monitor(MonitorOperation::Watch {
                monitor_id: monitor_id.as_str().to_owned(),
                after_sequence: sequence,
                timeout_ms,
            })
            .map_err(|issue| issue_error(issue, 3))?;
        let MonitorReply::Watch {
            events,
            next_sequence,
            ..
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
        if watch_deadline_reached(deadline, Instant::now()) {
            return Ok(());
        }
    }
}

fn watch_deadline_reached(deadline: Option<Instant>, now: Instant) -> bool {
    deadline.is_some_and(|deadline| now >= deadline)
}

fn watch_timeout_ms(deadline: Option<Instant>, now: Instant) -> u64 {
    deadline.map_or(30_000, |deadline| {
        u64::try_from(deadline.saturating_duration_since(now).as_millis())
            .unwrap_or(u64::MAX)
            .min(30_000)
    })
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
            run_wait_until_runnable(paths, MonitorId::parse(&args.monitor)?, args.timeout_secs)
        }
    }
}

fn run_wait_until_runnable(
    paths: &JackinPaths,
    monitor_id: MonitorId,
    timeout_seconds: u64,
) -> Result<()> {
    let client = attach_client(paths);
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds.min(300));
    let mut sequence = 0_u64;
    loop {
        let status_reply = client
            .monitor(MonitorOperation::Status {
                monitor_id: monitor_id.as_str().to_owned(),
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
                monitor_id: monitor_id.as_str().to_owned(),
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
            monitor_id: monitor_id.as_str().to_owned(),
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
        validate_nonempty("statusline payload session ID", payload_session_id)?;
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

fn run_auth(paths: &JackinPaths, command: &UsageAuthArgs) -> Result<()> {
    match &command.command {
        UsageAuthCommand::Prepare(args) => {
            if args.provider != UsageProviderArg::Claude {
                return Err(usage_error(
                    "unsupported_provider",
                    "authentication preparation supports Claude only",
                    3,
                ));
            }
            let service = args
                .keychain_service
                .as_deref()
                .unwrap_or(jackin_core::CLAUDE_KEYCHAIN_SERVICE_BASE);
            validate_keychain_service(service)?;
            if !all_stdio_are_terminal(
                std::io::stdin().is_terminal(),
                std::io::stdout().is_terminal(),
                std::io::stderr().is_terminal(),
            ) {
                return Err(issue_error(
                    MonitorIssue {
                        code: MonitorIssueCode::InteractionRequired,
                        message: "authentication preparation requires stdin, stdout, and stderr attached to a terminal"
                            .to_owned(),
                        retry_at_epoch: None,
                    },
                    2,
                ));
            }
            let config = broker_config(paths);
            let executable = config.service_executable.clone().ok_or_else(|| {
                usage_error(
                    "broker_unavailable",
                    "the sibling usage broker executable could not be located",
                    3,
                )
            })?;
            let child = Command::new(executable)
                .args(foreground_auth_bootstrap_args(&config, paths, service))
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .map_err(|error| {
                    usage_error(
                        "broker_unavailable",
                        &format!("cannot launch the usage broker auth helper: {error}"),
                        3,
                    )
                })?;
            if child.success() {
                Ok(())
            } else {
                Err(UsageCommandExit::new(child.code().unwrap_or(3), String::new()).into())
            }
        }
    }
}

fn foreground_auth_bootstrap_args(
    config: &jackin_usage::host::UsageBrokerConfig,
    paths: &JackinPaths,
    service: &str,
) -> Vec<OsString> {
    [
        "--prepare-auth".into(),
        "--provider".into(),
        "claude".into(),
        "--keychain-service".into(),
        service.into(),
        "--data-dir".into(),
        config.data_dir.as_os_str().to_owned(),
        "--config-root".into(),
        paths.config_dir.as_os_str().to_owned(),
        "--operator-home".into(),
        paths.home_dir.as_os_str().to_owned(),
        "--build-id".into(),
        config.build_id.clone().into(),
    ]
    .into()
}

fn all_stdio_are_terminal(stdin: bool, stdout: bool, stderr: bool) -> bool {
    stdin && stdout && stderr
}

fn validate_keychain_service(service: &str) -> Result<()> {
    if service.trim().is_empty() || service.len() > 512 || service.contains('\0') {
        return Err(usage_error(
            "invalid_keychain_service",
            "Keychain service must be nonempty, contain no NUL, and be at most 512 bytes",
            3,
        ));
    }
    Ok(())
}

fn parse_statusline(bytes: &[u8]) -> Result<StatuslineObservation, MonitorIssue> {
    jackin_usage::host::parse_statusline(bytes)
}

#[derive(Serialize)]
struct UsageErrorEnvelope<'a> {
    version: u16,
    error: UsageErrorBody<'a>,
}

#[derive(Serialize)]
struct UsageErrorBody<'a> {
    code: &'a str,
    message: &'a str,
}

fn usage_error(code: &str, message: &str, exit_code: i32) -> anyhow::Error {
    let json = serde_json::to_string(&UsageErrorEnvelope {
        version: 1,
        error: UsageErrorBody { code, message },
    })
    .unwrap_or_else(|_| {
        "{\"version\":1,\"error\":{\"code\":\"internal\",\"message\":\"unable to encode error\"}}"
            .to_owned()
    });
    UsageCommandExit::new(exit_code, json).into()
}

fn issue_error(issue: MonitorIssue, _fallback_exit_code: i32) -> anyhow::Error {
    let code = serde_json::to_value(issue.code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "monitor_error".to_owned());
    usage_error(&code, &issue.message, monitor_issue_exit_code(issue.code))
}

fn monitor_issue_exit_code(code: MonitorIssueCode) -> i32 {
    match code {
        MonitorIssueCode::BudgetUnverifiable
        | MonitorIssueCode::SpendUnavailable
        | MonitorIssueCode::SpendStale
        | MonitorIssueCode::SpendUnverified
        | MonitorIssueCode::PolicyRequired
        | MonitorIssueCode::BindingRequired
        | MonitorIssueCode::SgdCapAcknowledgementRequired
        | MonitorIssueCode::InteractionRequired => 2,
        _ => 3,
    }
}

fn json_value_exit<T: Serialize>(value: &T, exit_code: i32) -> anyhow::Error {
    let json = serde_json::to_string(value).unwrap_or_else(|_| {
        "{\"version\":1,\"error\":{\"code\":\"internal\",\"message\":\"unable to encode result\"}}".to_owned()
    });
    UsageCommandExit::new(exit_code, json).into()
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
    if let Some(bucket) = &account.quota_bucket {
        let mut parts = Vec::new();
        if let Some(state) = bucket.provider_quota_state {
            parts.push(match state {
                ProviderQuotaState::Exhausted => "Exhausted".to_owned(),
                ProviderQuotaState::Unlimited => "Unlimited".to_owned(),
            });
        }
        let empty_count_unit = if let Some(count) = bucket.count_quota {
            let unit = match count.unit {
                CountQuotaUnit::Requests => "requests",
                CountQuotaUnit::Unknown => "count",
            };
            let mut count_parts = Vec::new();
            if let Some(used) = count.used {
                count_parts.push(format!("{used} used"));
            }
            if let Some(limit) = count.limit {
                count_parts.push(format!("{limit} limit"));
            }
            if let Some(remaining) = count.remaining {
                count_parts.push(format!("{remaining} remaining"));
            }
            if count_parts.is_empty() {
                Some(unit)
            } else {
                if let Some(last) = count_parts.last_mut() {
                    last.push(' ');
                    last.push_str(unit);
                }
                parts.push(count_parts.join(" · "));
                None
            }
        } else {
            None
        };

        let has_money = bucket.used_money.is_some() || bucket.limit_money.is_some();
        if has_money {
            for label in [bucket.used_label.as_deref(), bucket.limit_label.as_deref()]
                .into_iter()
                .flatten()
                .filter(|label| !label.trim().is_empty())
            {
                parts.push(label.trim().to_owned());
            }
        }
        if let Some(percent) = bucket.used_percent {
            parts.push(format!(
                "{}% used ({})",
                percent.display_value(),
                percent_origin_label(bucket.used_percent_origin)
            ));
        }
        if let Some(percent) = bucket.remaining_percent {
            parts.push(format!(
                "{}% remaining ({})",
                percent.display_value(),
                percent_origin_label(bucket.remaining_percent_origin)
            ));
        }
        if let Some(unit) = empty_count_unit {
            if parts.is_empty() {
                return format!("{unit} unavailable");
            }
            parts.push(format!("{unit} unavailable"));
        }
        if !parts.is_empty() {
            return parts.join(" · ");
        }
        return bucket
            .used_label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
            .or_else(|| {
                bucket
                    .limit_label
                    .as_deref()
                    .filter(|label| !label.trim().is_empty())
            })
            .map_or_else(|| "unknown".to_owned(), |label| label.trim().to_owned());
    }
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

fn percent_origin_label(origin: Option<PercentObservationOrigin>) -> &'static str {
    match origin {
        Some(PercentObservationOrigin::ProviderReported) => "reported",
        Some(PercentObservationOrigin::Derived) => "derived",
        Some(PercentObservationOrigin::HistoricalUnknown) => "historical",
        None => "unknown source",
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

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::{Args, Subcommand};
use std::path::PathBuf;

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkflowCommand {
    #[command(about = "Validate a repair-workflow definition without compiling a plan", long_about = "Validate a repair-workflow definition without compiling a plan", before_help = BANNER, styles = HELP_STYLES)]
    Validate(WorkflowFileArgs),
    #[command(about = "Compile a repair-workflow definition into an immutable plan", long_about = "Compile a repair-workflow definition into an immutable plan", before_help = BANNER, styles = HELP_STYLES)]
    Plan(WorkflowPlanArgs),
    #[command(about = "Start a durable run for a stored plan hash", long_about = "Start a durable run for a stored plan hash", before_help = BANNER, styles = HELP_STYLES)]
    Run(WorkflowRunArgs),
    #[command(about = "Show a durable run's stage, round, and candidate (lists runs without a run ID)", long_about = "Show a durable run's stage, round, and candidate (lists runs without a run ID)", before_help = BANNER, styles = HELP_STYLES)]
    Status(WorkflowStatusArgs),
    #[command(about = "Show the fixed stage graph with node states for a run", long_about = "Show the fixed stage graph with node states for a run", before_help = BANNER, styles = HELP_STYLES)]
    Graph(WorkflowGraphArgs),
    #[command(about = "Show the durable event history for a run", long_about = "Show the durable event history for a run", before_help = BANNER, styles = HELP_STYLES)]
    Events(WorkflowEventsArgs),
    #[command(about = "Pause a durable run", long_about = "Pause a durable run", before_help = BANNER, styles = HELP_STYLES)]
    Pause(WorkflowPauseArgs),
    #[command(about = "Resume a paused or operator-blocked run", long_about = "Resume a paused or operator-blocked run", before_help = BANNER, styles = HELP_STYLES)]
    Resume(WorkflowResumeArgs),
    #[command(about = "Cancel a durable run and preserve its source", long_about = "Cancel a durable run and preserve its source", before_help = BANNER, styles = HELP_STYLES)]
    Cancel(WorkflowCancelArgs),
    #[command(about = "Resolve the exact session key for a run worker", long_about = "Resolve the exact session key for a run worker", before_help = BANNER, styles = HELP_STYLES)]
    Attach(WorkflowAttachArgs),
    #[command(about = "Resolve uncertain prompt delivery for a run command", long_about = "Resolve uncertain prompt delivery for a run command", before_help = BANNER, styles = HELP_STYLES)]
    ResolveInput(WorkflowResolveInputArgs),
    #[command(about = "Write the Markdown run report for a run", long_about = "Write the Markdown run report for a run", before_help = BANNER, styles = HELP_STYLES)]
    Export(WorkflowExportArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Workflow definition file argument shared by `validate` and `plan`.",
    long_about = "Workflow definition file argument shared by `validate` and `plan`."
)]
pub struct WorkflowFileArgs {
    #[arg(
        help = "Repair-workflow definition file (amends the sibling Workflow.pkl schema)",
        long_help = "Repair-workflow definition file (amends the sibling Workflow.pkl schema)",
        value_name = "FILE"
    )]
    pub file: PathBuf,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow run`.",
    long_about = "Arguments for `jackin workflow run`."
)]
pub struct WorkflowRunArgs {
    #[arg(
        help = "Stored plan hash to start a run for",
        long_help = "Stored plan hash to start a run for",
        value_name = "PLAN_HASH"
    )]
    pub plan_hash: String,
    #[arg(
        help = "Idempotency key (defaults to the plan hash)",
        long_help = "Idempotency key (defaults to the plan hash)",
        long = "idempotency-key",
        value_name = "KEY"
    )]
    pub idempotency_key: Option<String>,
    #[arg(
        help = "Print the run as JSON",
        long_help = "Print the run as JSON",
        long
    )]
    pub json: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow status`.",
    long_about = "Arguments for `jackin workflow status`."
)]
pub struct WorkflowStatusArgs {
    #[arg(
        help = "Run ID (lists runs when omitted)",
        long_help = "Run ID (lists runs when omitted)",
        value_name = "RUN_ID"
    )]
    pub run_id: Option<String>,
    #[arg(
        help = "Print the status or list as JSON",
        long_help = "Print the status or list as JSON",
        long
    )]
    pub json: bool,
    #[arg(
        help = "Maximum runs to list (bare status only)",
        long_help = "Maximum runs to list (bare status only)",
        long = "limit",
        value_name = "N",
        default_value_t = 50
    )]
    pub limit: u32,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow graph`.",
    long_about = "Arguments for `jackin workflow graph`."
)]
pub struct WorkflowGraphArgs {
    #[arg(help = "Run ID", long_help = "Run ID", value_name = "RUN_ID")]
    pub run_id: String,
    #[arg(
        help = "Print the graph as JSON",
        long_help = "Print the graph as JSON",
        long
    )]
    pub json: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow events`.",
    long_about = "Arguments for `jackin workflow events`."
)]
pub struct WorkflowEventsArgs {
    #[arg(help = "Run ID", long_help = "Run ID", value_name = "RUN_ID")]
    pub run_id: String,
    #[arg(
        help = "Return events after this sequence (resume cursor)",
        long_help = "Return events after this sequence (resume cursor)",
        long = "after",
        value_name = "SEQ",
        default_value_t = -1
    )]
    pub after: i64,
    #[arg(
        help = "Maximum events to return",
        long_help = "Maximum events to return",
        long = "limit",
        value_name = "N",
        default_value_t = 200
    )]
    pub limit: u32,
    #[arg(
        help = "Print the events as JSON",
        long_help = "Print the events as JSON",
        long
    )]
    pub json: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow pause`.",
    long_about = "Arguments for `jackin workflow pause`."
)]
pub struct WorkflowPauseArgs {
    #[arg(help = "Run ID", long_help = "Run ID", value_name = "RUN_ID")]
    pub run_id: String,
    #[arg(
        help = "Pause reason",
        long_help = "Pause reason",
        long = "reason",
        value_name = "REASON",
        default_value = "operator pause"
    )]
    pub reason: String,
    #[arg(
        help = "Mark a checkpoint source-capture freeze instead of a normal pause",
        long_help = "Mark a checkpoint source-capture freeze instead of a normal pause",
        long = "checkpoint"
    )]
    pub checkpoint: bool,
    #[arg(
        help = "Print the run as JSON",
        long_help = "Print the run as JSON",
        long
    )]
    pub json: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow resume`.",
    long_about = "Arguments for `jackin workflow resume`."
)]
pub struct WorkflowResumeArgs {
    #[arg(help = "Run ID", long_help = "Run ID", value_name = "RUN_ID")]
    pub run_id: String,
    #[arg(
        help = "Print the run as JSON",
        long_help = "Print the run as JSON",
        long
    )]
    pub json: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow cancel`.",
    long_about = "Arguments for `jackin workflow cancel`."
)]
pub struct WorkflowCancelArgs {
    #[arg(help = "Run ID", long_help = "Run ID", value_name = "RUN_ID")]
    pub run_id: String,
    #[arg(
        help = "Cancel reason",
        long_help = "Cancel reason",
        long = "reason",
        value_name = "REASON",
        default_value = "operator cancel"
    )]
    pub reason: String,
    #[arg(
        help = "Print the run as JSON",
        long_help = "Print the run as JSON",
        long
    )]
    pub json: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow attach`.",
    long_about = "Arguments for `jackin workflow attach`."
)]
pub struct WorkflowAttachArgs {
    #[arg(help = "Run ID", long_help = "Run ID", value_name = "RUN_ID")]
    pub run_id: String,
    #[arg(
        help = "Worker to attach (implementer or reviewer)",
        long_help = "Worker to attach (implementer or reviewer)",
        long = "worker",
        value_name = "WORKER",
        default_value = "implementer"
    )]
    pub worker: String,
    #[arg(
        help = "Observe without taking input ownership",
        long_help = "Observe without taking input ownership",
        long = "observe",
        conflicts_with = "control"
    )]
    pub observe: bool,
    #[arg(
        help = "Take exclusive input ownership",
        long_help = "Take exclusive input ownership",
        long = "control",
        conflicts_with = "observe"
    )]
    pub control: bool,
    #[arg(
        help = "Print the attachment as JSON",
        long_help = "Print the attachment as JSON",
        long
    )]
    pub json: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow resolve-input`.",
    long_about = "Arguments for `jackin workflow resolve-input`."
)]
pub struct WorkflowResolveInputArgs {
    #[arg(help = "Run ID", long_help = "Run ID", value_name = "RUN_ID")]
    pub run_id: String,
    #[arg(
        help = "Command ID in uncertain delivery",
        long_help = "Command ID in uncertain delivery",
        value_name = "COMMAND"
    )]
    pub command: String,
    #[arg(
        help = "Resolution action (continue_from_current, submit_new, cancel)",
        long_help = "Resolution action (continue_from_current, submit_new, cancel)",
        long = "action",
        value_name = "ACTION"
    )]
    pub action: String,
    #[arg(
        help = "Reason for the decision",
        long_help = "Reason for the decision",
        long = "reason",
        value_name = "REASON",
        default_value = "operator resolution"
    )]
    pub reason: String,
    #[arg(
        help = "Observed session state (required: inspect the original session first)",
        long_help = "Observed session state (required: inspect the original session first)",
        long = "observed-session-state",
        value_name = "STATE"
    )]
    pub observed_session_state: String,
    #[arg(
        help = "Print the run as JSON",
        long_help = "Print the run as JSON",
        long
    )]
    pub json: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow export`.",
    long_about = "Arguments for `jackin workflow export`."
)]
pub struct WorkflowExportArgs {
    #[arg(help = "Run ID", long_help = "Run ID", value_name = "RUN_ID")]
    pub run_id: String,
    #[arg(
        help = "Destination directory for the Markdown report",
        long_help = "Destination directory for the Markdown report",
        long = "out",
        value_name = "DIRECTORY"
    )]
    pub out: PathBuf,
    #[arg(
        help = "Include terminal text (explicit content selection for sensitive artifacts)",
        long_help = "Include terminal text (explicit content selection for sensitive artifacts)",
        long = "include-terminal"
    )]
    pub include_terminal: bool,
    #[arg(
        help = "Print the export manifest as JSON",
        long_help = "Print the export manifest as JSON",
        long
    )]
    pub json: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow plan`.",
    long_about = "Arguments for `jackin workflow plan`."
)]
pub struct WorkflowPlanArgs {
    #[arg(
        help = "Repair-workflow definition file (amends the sibling Workflow.pkl schema)",
        long_help = "Repair-workflow definition file (amends the sibling Workflow.pkl schema)",
        value_name = "FILE"
    )]
    pub file: PathBuf,
    #[arg(
        help = "Print the plan or diagnostics as JSON",
        long_help = "Print the plan or diagnostics as JSON",
        long
    )]
    pub json: bool,
}

/// Typed workflow CLI failure: carries the C7 exit code (0/2/3/4/1)
/// and the stable JSON or human error payload. Handled in `main`
/// like [`super::usage::UsageCommandExit`]: the payload goes to
/// stdout so `--json` stays machine-readable, and the process exits
/// with the mapped code.
#[derive(Debug, thiserror::Error)]
#[error("{json}")]
pub struct WorkflowCommandExit {
    exit_code: i32,
    json: String,
}

impl WorkflowCommandExit {
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

/// Map a C7 daemon error code to the CLI exit code: 0 is success
/// (never an error), 2 is invalid input, 3 is denied access or an
/// unsupported capability, 4 is a state conflict or required
/// operator action, and 1 is an execution or transport error.
#[must_use]
pub const fn exit_code_for_daemon_code(code: &str) -> i32 {
    match code.as_bytes() {
        b"invalid_request" | b"not_found" | b"unsupported_version" => 2,
        b"denied" | b"unsupported_capability" => 3,
        b"busy" | b"state_conflict" | b"needs_operator" => 4,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::exit_code_for_daemon_code;

    #[test]
    fn daemon_codes_map_to_documented_exit_codes() {
        assert_eq!(exit_code_for_daemon_code("invalid_request"), 2);
        assert_eq!(exit_code_for_daemon_code("not_found"), 2);
        assert_eq!(exit_code_for_daemon_code("unsupported_version"), 2);
        assert_eq!(exit_code_for_daemon_code("denied"), 3);
        assert_eq!(exit_code_for_daemon_code("unsupported_capability"), 3);
        assert_eq!(exit_code_for_daemon_code("busy"), 4);
        assert_eq!(exit_code_for_daemon_code("state_conflict"), 4);
        assert_eq!(exit_code_for_daemon_code("needs_operator"), 4);
        assert_eq!(exit_code_for_daemon_code("execution_error"), 1);
        assert_eq!(exit_code_for_daemon_code(""), 1);
        assert_eq!(exit_code_for_daemon_code("anything-new"), 1);
    }
}

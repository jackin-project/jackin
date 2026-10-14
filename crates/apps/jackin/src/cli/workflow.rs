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
    #[command(about = "Show a durable run's stage, round, and candidate", long_about = "Show a durable run's stage, round, and candidate", before_help = BANNER, styles = HELP_STYLES)]
    Status(WorkflowStatusArgs),
    #[command(about = "Pause a durable run", long_about = "Pause a durable run", before_help = BANNER, styles = HELP_STYLES)]
    Pause(WorkflowPauseArgs),
    #[command(about = "Resume a paused or operator-blocked run", long_about = "Resume a paused or operator-blocked run", before_help = BANNER, styles = HELP_STYLES)]
    Resume(WorkflowResumeArgs),
    #[command(about = "Cancel a durable run and preserve its source", long_about = "Cancel a durable run and preserve its source", before_help = BANNER, styles = HELP_STYLES)]
    Cancel(WorkflowCancelArgs),
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
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow status`.",
    long_about = "Arguments for `jackin workflow status`."
)]
pub struct WorkflowStatusArgs {
    #[arg(
        help = "Run ID (defaults to the active run)",
        long_help = "Run ID (defaults to the active run)",
        value_name = "RUN_ID"
    )]
    pub run_id: Option<String>,
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
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    about = "Arguments for `jackin workflow resume`.",
    long_about = "Arguments for `jackin workflow resume`."
)]
pub struct WorkflowResumeArgs {
    #[arg(help = "Run ID", long_help = "Run ID", value_name = "RUN_ID")]
    pub run_id: String,
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

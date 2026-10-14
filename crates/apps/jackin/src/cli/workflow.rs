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

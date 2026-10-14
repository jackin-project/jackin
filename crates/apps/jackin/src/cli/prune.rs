// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::{Args, Subcommand};

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum PruneCommand {
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Roles,
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Cache,
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Images,
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Instances(PruneInstancesArgs),
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    System(PruneSystemArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(before_help = BANNER, styles = HELP_STYLES)]
pub struct PruneInstancesArgs {
    #[arg(long, short = 'a')]
    pub all: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(before_help = BANNER, styles = HELP_STYLES)]
pub struct PruneSystemArgs {
    #[arg(long, short = 'y')]
    pub yes: bool,
    #[arg(long, short = 'a')]
    pub all: bool,
}

#[cfg(test)]
mod tests;

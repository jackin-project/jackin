// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::Args;

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin eject agent-smith
  jackin eject agent-smith --all
  jackin eject agent-smith --purge
  jackin eject k7p9m2xq
  jackin eject jk-k7p9m2xq-agentsmith"
)]
pub struct EjectArgs {
    pub selector: String,

    #[arg(long)]
    pub all: bool,

    #[arg(long)]
    pub purge: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(
    before_help = BANNER,
    styles = HELP_STYLES,
    after_long_help = "\
Examples:
  jackin purge agent-smith
  jackin purge agent-smith --all
  jackin purge k7p9m2xq
  jackin purge chainargos/the-architect"
)]
pub struct PurgeArgs {
    pub selector: String,

    #[arg(long)]
    pub all: bool,
}

#[cfg(test)]
mod tests;

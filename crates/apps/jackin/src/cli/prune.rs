// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::{Args, Subcommand};

use super::{BANNER, HELP_STYLES};

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum PruneCommand {
    #[command(about = "Delete cached role repositories", long_about = "Delete cached role repositories

Removes the local role-repo cache. Role repos are re-cloned automatically the next time a role is launched.", before_help = BANNER, styles = HELP_STYLES)]
    Roles,
    #[command(about = "Delete shared caches", long_about = "Delete shared caches

Removes the shared cache directory. Includes version-check results and other rebuildable data. All caches regenerate automatically.", before_help = BANNER, styles = HELP_STYLES)]
    Cache,
    #[command(about = "Remove unused jackin-managed Docker images", long_about = "Remove unused jackin-managed Docker images

Removes jk_* images that have no role containers (running or stopped). Images still used by a role container are skipped.", before_help = BANNER, styles = HELP_STYLES)]
    Images,
    #[command(about = "Purge on-disk state for instances", long_about = "Purge on-disk state for instances

By default removes state directories and index entries for instances in terminal statuses: `clean_exited`, `superseded`, `failed_setup`, and `purged` tombstones. Running and recoverable instances are skipped.

Pass `--all` to also stop and remove running instances (equivalent to `jackin eject <selector> --purge` for every instance).", before_help = BANNER, styles = HELP_STYLES)]
    Instances(PruneInstancesArgs),
    #[command(about = "Remove all prunable data", long_about = "Remove all prunable data

Runs in order: prune instances, prune images, prune roles, prune cache. By default skips running instances; pass `--all` to stop and remove them too.", before_help = BANNER, styles = HELP_STYLES)]
    System(PruneSystemArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(about = "Arguments for `jackin prune instances`", long_about = "Arguments for `jackin prune instances`", before_help = BANNER, styles = HELP_STYLES)]
pub struct PruneInstancesArgs {
    #[arg(
        help = "Also stop and remove running or recoverable instances",
        long_help = "Also stop and remove running or recoverable instances",
        long,
        short = 'a'
    )]
    pub all: bool,
}

#[derive(Debug, Args, PartialEq, Eq)]
#[command(about = "Arguments for `jackin prune system`", long_about = "Arguments for `jackin prune system`", before_help = BANNER, styles = HELP_STYLES)]
pub struct PruneSystemArgs {
    #[arg(
        help = "Skip the confirmation prompt",
        long_help = "Skip the confirmation prompt",
        long,
        short = 'y'
    )]
    pub yes: bool,
    #[arg(
        help = "Also stop and remove running or recoverable instances",
        long_help = "Also stop and remove running or recoverable instances",
        long,
        short = 'a'
    )]
    pub all: bool,
}

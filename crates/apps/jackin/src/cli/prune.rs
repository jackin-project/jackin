// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! CLI argument structs for `jackin prune` subcommands (images, instances, cache, diagnostics, all).
//!
//! Not responsible for: executing deletion logic — structs are parsed by `clap`
//! and dispatched to handlers in `src/commands/prune/`.

use clap::{Args, Subcommand};

use super::{BANNER, HELP_STYLES};

/// Delete cached or stale jackin data
#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum PruneCommand {
    /// Delete cached role repositories
    ///
    /// Removes the local role-repo cache. Role repos are re-cloned
    /// automatically the next time a role is launched.
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Roles,
    /// Delete shared caches
    ///
    /// Removes the shared cache directory. Includes version-check results and
    /// other rebuildable data. All caches regenerate automatically.
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Cache,
    /// Remove unused jackin-managed Docker images
    ///
    /// Removes jk_* images that have no role containers (running or stopped).
    /// Images still used by a role container are skipped.
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Images,
    /// Purge on-disk state for instances
    ///
    /// By default removes state directories and index entries for instances
    /// in terminal statuses: `clean_exited`, `superseded`, `failed_setup`,
    /// and `purged` tombstones. Running and recoverable instances are skipped.
    ///
    /// Pass `--all` to also stop and remove running instances (equivalent to
    /// `jackin eject <selector> --purge` for every instance).
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Instances(PruneInstancesArgs),
    /// Remove all prunable data
    ///
    /// Runs in order: prune instances, prune images, prune roles, prune cache.
    /// By default skips running instances; pass `--all` to stop and remove them too.
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    System(PruneSystemArgs),
}

/// Arguments for `jackin prune instances`
#[derive(Debug, Args, PartialEq, Eq)]
#[command(before_help = BANNER, styles = HELP_STYLES)]
pub struct PruneInstancesArgs {
    /// Also stop and remove running or recoverable instances
    #[arg(long, short = 'a')]
    pub all: bool,
}

/// Arguments for `jackin prune system`
#[derive(Debug, Args, PartialEq, Eq)]
#[command(before_help = BANNER, styles = HELP_STYLES)]
pub struct PruneSystemArgs {
    /// Skip the confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Also stop and remove running or recoverable instances
    #[arg(long, short = 'a')]
    pub all: bool,
}

#[cfg(test)]
mod tests;

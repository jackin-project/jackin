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
    /// Removes the shared cache directory. Includes the compiled terminfo
    /// cache and version-check results. All caches regenerate automatically.
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Cache,
    /// Remove unused jackin-managed Docker images
    ///
    /// Removes jk-* images that have no containers (running or stopped).
    /// Images still used by any container are skipped.
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Images,
    /// Purge on-disk state for terminated instances
    ///
    /// Removes state directories and index entries for instances in terminal
    /// statuses: clean_exited, superseded, failed_setup, and purged
    /// tombstones. Crashed, preserved, and recoverable instances are skipped
    /// — use `jackin eject <selector> --purge` for those.
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    Instances,
    /// Remove all prunable data
    ///
    /// Runs in order: prune instances, prune images, prune roles, prune cache.
    #[command(before_help = BANNER, styles = HELP_STYLES)]
    All(PruneAllArgs),
}

/// Arguments for `jackin prune all`
#[derive(Debug, Args, PartialEq, Eq)]
#[command(before_help = BANNER, styles = HELP_STYLES)]
pub struct PruneAllArgs {
    /// Skip the confirmation prompt
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    fn strip_ansi(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                for inner in chars.by_ref() {
                    if inner.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                result.push(ch);
            }
        }
        result
    }

    fn help_text(args: &[&str]) -> String {
        let err = Cli::try_parse_from(args).unwrap_err();
        strip_ansi(&err.to_string())
    }

    #[test]
    fn prune_help_lists_subcommands() {
        let help = help_text(&["jackin", "prune", "--help"]);
        for sub in ["roles", "cache", "images", "instances", "all"] {
            assert!(help.contains(sub), "missing subcommand: {sub}");
        }
    }

    #[test]
    fn prune_roles_parses() {
        let cli = Cli::try_parse_from(["jackin", "prune", "roles"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(crate::cli::Command::Prune(PruneCommand::Roles))
        ));
    }

    #[test]
    fn prune_cache_parses() {
        let cli = Cli::try_parse_from(["jackin", "prune", "cache"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(crate::cli::Command::Prune(PruneCommand::Cache))
        ));
    }

    #[test]
    fn prune_images_parses() {
        let cli = Cli::try_parse_from(["jackin", "prune", "images"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(crate::cli::Command::Prune(PruneCommand::Images))
        ));
    }

    #[test]
    fn prune_instances_parses() {
        let cli = Cli::try_parse_from(["jackin", "prune", "instances"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(crate::cli::Command::Prune(PruneCommand::Instances))
        ));
    }

    #[test]
    fn prune_all_defaults_yes_false() {
        let cli = Cli::try_parse_from(["jackin", "prune", "all"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(crate::cli::Command::Prune(PruneCommand::All(
                PruneAllArgs { yes: false }
            )))
        ));
    }

    #[test]
    fn prune_all_yes_flag_parses() {
        let cli = Cli::try_parse_from(["jackin", "prune", "all", "--yes"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(crate::cli::Command::Prune(PruneCommand::All(
                PruneAllArgs { yes: true }
            )))
        ));
    }

    #[test]
    fn prune_all_short_y_flag_parses() {
        let cli = Cli::try_parse_from(["jackin", "prune", "all", "-y"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(crate::cli::Command::Prune(PruneCommand::All(
                PruneAllArgs { yes: true }
            )))
        ));
    }
}

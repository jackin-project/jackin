// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `prune`.
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
    for sub in ["roles", "cache", "images", "instances", "system"] {
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
        Some(crate::cli::Command::Prune(PruneCommand::Instances(
            PruneInstancesArgs { all: false }
        )))
    ));
}

#[test]
fn prune_instances_all_flag_parses() {
    let cli = Cli::try_parse_from(["jackin", "prune", "instances", "--all"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(crate::cli::Command::Prune(PruneCommand::Instances(
            PruneInstancesArgs { all: true }
        )))
    ));
}

#[test]
fn prune_system_defaults_yes_false_all_false() {
    let cli = Cli::try_parse_from(["jackin", "prune", "system"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(crate::cli::Command::Prune(PruneCommand::System(
            PruneSystemArgs {
                yes: false,
                all: false
            }
        )))
    ));
}

#[test]
fn prune_system_yes_flag_parses() {
    let cli = Cli::try_parse_from(["jackin", "prune", "system", "--yes"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(crate::cli::Command::Prune(PruneCommand::System(
            PruneSystemArgs {
                yes: true,
                all: false
            }
        )))
    ));
}

#[test]
fn prune_system_short_y_flag_parses() {
    let cli = Cli::try_parse_from(["jackin", "prune", "system", "-y"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(crate::cli::Command::Prune(PruneCommand::System(
            PruneSystemArgs {
                yes: true,
                all: false
            }
        )))
    ));
}

#[test]
fn prune_system_all_flag_parses() {
    let cli = Cli::try_parse_from(["jackin", "prune", "system", "--all"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(crate::cli::Command::Prune(PruneCommand::System(
            PruneSystemArgs {
                yes: false,
                all: true
            }
        )))
    ));
}

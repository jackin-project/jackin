// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `config`.
use super::super::{Cli, Command};
use super::*;
use clap::Parser;

/// Strip ANSI escape sequences for clean test assertions.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip until 'm' (SGR) or other terminator
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
fn parses_config_mount_add() {
    let cli = Cli::try_parse_from([
        "jackin",
        "config",
        "mount",
        "add",
        "gradle-cache",
        "--src",
        "~/.gradle/caches",
        "--dst",
        "/home/agent/.gradle/caches",
        "--readonly",
        "--scope",
        "chainargos/*",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Config(ConfigCommand::Mount(
            MountCommand::Add { .. }
        )))
    ));
}

#[test]
fn parses_config_mount_remove() {
    let cli = Cli::try_parse_from(["jackin", "config", "mount", "remove", "gradle-cache"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Config(ConfigCommand::Mount(
            MountCommand::Remove { .. }
        )))
    ));
}

#[test]
fn parses_config_mount_list() {
    let cli = Cli::try_parse_from(["jackin", "config", "mount", "list"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Config(ConfigCommand::Mount(MountCommand::List)))
    ));
}

#[test]
fn parses_config_trust_grant() {
    let cli = Cli::try_parse_from([
        "jackin",
        "config",
        "trust",
        "grant",
        "chainargos/the-architect",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Config(ConfigCommand::Trust(
            TrustCommand::Grant { .. }
        )))
    ));
}

#[test]
fn parses_config_trust_revoke() {
    let cli = Cli::try_parse_from([
        "jackin",
        "config",
        "trust",
        "revoke",
        "chainargos/the-architect",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Config(ConfigCommand::Trust(
            TrustCommand::Revoke { .. }
        )))
    ));
}

#[test]
fn parses_config_trust_list() {
    let cli = Cli::try_parse_from(["jackin", "config", "trust", "list"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Config(ConfigCommand::Trust(TrustCommand::List)))
    ));
}

// ── help subcommand disabled ────────────────────────────────────────

#[test]
fn config_mount_rejects_help_subcommand() {
    let err = Cli::try_parse_from(["jackin", "config", "mount", "help"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
}

// ── Config mount help ───────────────────────────────────────────────

#[test]
fn config_mount_add_help_shows_examples() {
    let help = help_text(&["jackin", "config", "mount", "add", "--help"]);
    assert!(help.contains("Examples:"));
    assert!(help.contains("jackin config mount add gradle-cache"));
    assert!(help.contains("--scope"));
}

#[test]
fn config_mount_remove_help_shows_examples() {
    let help = help_text(&["jackin", "config", "mount", "remove", "--help"]);
    assert!(help.contains("Examples:"));
    assert!(help.contains("jackin config mount remove gradle-cache"));
}

// ── Config auth help ─────────────────────────────────────────────────

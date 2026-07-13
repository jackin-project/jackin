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
fn config_auth_rejects_help_subcommand() {
    let err = Cli::try_parse_from(["jackin", "config", "auth", "help"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
}

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

#[test]
fn config_auth_set_help_shows_examples() {
    let help = help_text(&["jackin", "config", "auth", "set", "--help"]);
    assert!(help.contains("Examples:"));
    assert!(help.contains("jackin config auth set sync"));
    assert!(help.contains("jackin config auth set oauth_token"));
    assert!(help.contains("jackin config auth set api_key"));
}

#[test]
fn config_auth_set_help_lists_token_as_accepted_mode() {
    let help = help_text(&["jackin", "config", "auth", "set", "--help"]);
    // Modes are listed in the subcommand doc comment.
    assert!(help.contains("sync"));
    assert!(help.contains("ignore"));
    assert!(
        help.contains("oauth_token"),
        "help text must advertise the oauth_token mode; got:\n{help}"
    );
    assert!(
        help.contains("api_key"),
        "help text must advertise the api_key mode; got:\n{help}"
    );
}

#[test]
fn config_auth_show_help_shows_examples() {
    let help = help_text(&["jackin", "config", "auth", "show", "--help"]);
    assert!(help.contains("Examples:"));
    assert!(help.contains("jackin config auth show"));
}

#[test]
fn parses_config_auth_set_global_defaults_to_claude() {
    let cli = Cli::try_parse_from(["jackin", "config", "auth", "set", "sync"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Config(ConfigCommand::Auth(AuthCommand::Set {
                    ref mode, ref agent,
                }))) if mode == "sync" && agent == "claude"
    ));
}

#[test]
fn parses_config_auth_set_oauth_token_global() {
    let cli = Cli::try_parse_from(["jackin", "config", "auth", "set", "oauth_token"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Config(ConfigCommand::Auth(AuthCommand::Set {
                    ref mode, ..
                }))) if mode == "oauth_token"
    ));
}

#[test]
fn parses_config_auth_set_with_agent_flag() {
    let cli = Cli::try_parse_from([
        "jackin", "config", "auth", "set", "api_key", "--agent", "codex",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Config(ConfigCommand::Auth(AuthCommand::Set {
                    ref mode, ref agent,
                }))) if mode == "api_key" && agent == "codex"
    ));
}

#[test]
fn parses_config_auth_show() {
    let cli = Cli::try_parse_from(["jackin", "config", "auth", "show"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Config(ConfigCommand::Auth(AuthCommand::Show)))
    ));
}

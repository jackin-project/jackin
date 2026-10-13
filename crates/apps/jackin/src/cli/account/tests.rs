// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::cli::{Cli, Command};
use clap::Parser;

#[test]
fn profile_and_key_sources_are_exclusive() {
    Cli::try_parse_from([
        "jackin",
        "account",
        "add",
        "work",
        "--directory",
        "/tmp/profile",
        "--agent",
        "claude",
        "--api-key",
        "--provider",
        "anthropic",
    ])
    .unwrap_err();
    Cli::try_parse_from([
        "jackin",
        "account",
        "add",
        "work",
        "--api-key",
        "--provider",
        "openai",
        "--stdin",
    ])
    .unwrap();
    Cli::try_parse_from([
        "jackin",
        "account",
        "add",
        "work",
        "--directory",
        "/tmp/profile",
        "--agent",
        "codex",
    ])
    .unwrap();
}

#[test]
fn selection_requires_account_or_clear() {
    Cli::try_parse_from([
        "jackin",
        "workspace",
        "account",
        "select",
        "app",
        "--agent",
        "codex",
    ])
    .unwrap_err();
    let parsed = Cli::try_parse_from([
        "jackin",
        "workspace",
        "account",
        "select",
        "app",
        "--agent",
        "codex",
        "--clear",
    ])
    .unwrap();
    assert!(matches!(
        parsed.command,
        Some(Command::Workspace(
            super::super::workspace::WorkspaceCommand::Account(WorkspaceAccountCommand::Select {
                clear: true,
                ..
            })
        ))
    ));
}

#[test]
fn removed_auth_commands_are_rejected() {
    Cli::try_parse_from(["jackin", "config", "auth", "show"]).unwrap_err();
    Cli::try_parse_from(["jackin", "workspace", "claude-token", "doctor", "app"]).unwrap_err();
}

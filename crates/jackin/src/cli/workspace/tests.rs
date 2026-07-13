// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `workspace`.
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
fn parses_workspace_create_command() {
    let cli = Cli::try_parse_from([
        "jackin",
        "workspace",
        "create",
        "big-monorepo",
        "--workdir",
        "/workspace/project",
        "--mount",
        "/tmp/project:/workspace/project",
        "--mount",
        "/tmp/cache:/workspace/cache:ro",
        "--allowed-role",
        "agent-smith",
        "--default-role",
        "agent-smith",
    ])
    .unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Workspace(WorkspaceCommand::Create { .. }))
    ));
}

#[test]
fn workspace_create_requires_explicit_mount() {
    let err = Cli::try_parse_from([
        "jackin",
        "workspace",
        "create",
        "my-app",
        "--workdir",
        "/tmp/my-app",
    ])
    .unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
}

#[test]
fn parses_workspace_edit_command() {
    let cli = Cli::try_parse_from([
        "jackin",
        "workspace",
        "edit",
        "big-monorepo",
        "--mount",
        "/tmp/new-cache:/workspace/cache:ro",
        "--remove-destination",
        "/workspace/shared",
    ])
    .unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Workspace(WorkspaceCommand::Edit { .. }))
    ));
}

#[test]
fn parses_workspace_edit_with_no_workdir_mount() {
    let cli = Cli::try_parse_from([
        "jackin",
        "workspace",
        "edit",
        "my-app",
        "--no-workdir-mount",
    ])
    .unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Workspace(WorkspaceCommand::Edit {
            no_workdir_mount: true,
            ..
        }))
    ));
}

#[test]
fn parses_workspace_create_with_keep_awake() {
    let cli = Cli::try_parse_from([
        "jackin",
        "workspace",
        "create",
        "my-app",
        "--workdir",
        "/tmp/my-app",
        "--mount",
        "/tmp/my-app",
        "--keep-awake",
    ])
    .unwrap();
    match cli.command {
        Some(Command::Workspace(WorkspaceCommand::Create { keep_awake, .. })) => {
            assert!(keep_awake, "--keep-awake should set the field to true");
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn parses_workspace_create_with_default_agent() {
    let cli = Cli::try_parse_from([
        "jackin",
        "workspace",
        "create",
        "my-app",
        "--workdir",
        "/tmp/my-app",
        "--mount",
        "/tmp/my-app",
        "--default-agent",
        "codex",
    ])
    .unwrap();
    match cli.command {
        Some(Command::Workspace(WorkspaceCommand::Create { default_agent, .. })) => {
            assert_eq!(default_agent, Some(jackin_core::Agent::Codex));
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn workspace_create_keep_awake_defaults_to_false() {
    let cli = Cli::try_parse_from([
        "jackin",
        "workspace",
        "create",
        "my-app",
        "--workdir",
        "/tmp/my-app",
        "--mount",
        "/tmp/my-app",
    ])
    .unwrap();
    match cli.command {
        Some(Command::Workspace(WorkspaceCommand::Create { keep_awake, .. })) => {
            assert!(!keep_awake, "absent --keep-awake should default to false");
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn parses_workspace_edit_with_keep_awake() {
    let cli =
        Cli::try_parse_from(["jackin", "workspace", "edit", "my-app", "--keep-awake"]).unwrap();
    match cli.command {
        Some(Command::Workspace(WorkspaceCommand::Edit {
            keep_awake,
            no_keep_awake,
            ..
        })) => {
            assert!(keep_awake);
            assert!(!no_keep_awake);
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn parses_workspace_edit_with_no_keep_awake() {
    let cli =
        Cli::try_parse_from(["jackin", "workspace", "edit", "my-app", "--no-keep-awake"]).unwrap();
    match cli.command {
        Some(Command::Workspace(WorkspaceCommand::Edit {
            keep_awake,
            no_keep_awake,
            ..
        })) => {
            assert!(!keep_awake);
            assert!(no_keep_awake);
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn parses_workspace_edit_with_default_agent() {
    let cli = Cli::try_parse_from([
        "jackin",
        "workspace",
        "edit",
        "my-app",
        "--default-agent",
        "codex",
    ])
    .unwrap();
    match cli.command {
        Some(Command::Workspace(WorkspaceCommand::Edit { default_agent, .. })) => {
            assert_eq!(default_agent, Some(jackin_core::Agent::Codex));
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn parses_workspace_edit_with_clear_default_agent() {
    let cli = Cli::try_parse_from([
        "jackin",
        "workspace",
        "edit",
        "my-app",
        "--clear-default-agent",
    ])
    .unwrap();
    match cli.command {
        Some(Command::Workspace(WorkspaceCommand::Edit {
            clear_default_agent,
            ..
        })) => {
            assert!(clear_default_agent);
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn rejects_conflicting_workspace_edit_default_agent_flags() {
    let err = Cli::try_parse_from([
        "jackin",
        "workspace",
        "edit",
        "my-app",
        "--default-agent",
        "codex",
        "--clear-default-agent",
    ])
    .unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn rejects_conflicting_workspace_edit_keep_awake_flags() {
    // Mutual exclusion at the CLI parser level — the user must not
    // be able to ask jackin to both opt in and opt out in one
    // invocation. clap enforces this via `conflicts_with`.
    let err = Cli::try_parse_from([
        "jackin",
        "workspace",
        "edit",
        "my-app",
        "--keep-awake",
        "--no-keep-awake",
    ])
    .unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn rejects_conflicting_workspace_edit_default_role_flags() {
    let err = Cli::try_parse_from([
        "jackin",
        "workspace",
        "edit",
        "big-monorepo",
        "--default-role",
        "agent-smith",
        "--clear-default-role",
    ])
    .unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn parses_workspace_edit_with_yes_flag() {
    let cli = Cli::try_parse_from([
        "jackin",
        "workspace",
        "edit",
        "proj-alpha",
        "--mount",
        "/tmp/proj-alpha",
        "--yes",
    ])
    .unwrap();
    match cli.command {
        Some(Command::Workspace(WorkspaceCommand::Edit { assume_yes, .. })) => {
            assert!(assume_yes);
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn parses_workspace_edit_with_prune_flag() {
    let cli =
        Cli::try_parse_from(["jackin", "workspace", "edit", "proj-alpha", "--prune"]).unwrap();
    match cli.command {
        Some(Command::Workspace(WorkspaceCommand::Edit { prune, .. })) => assert!(prune),
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn parses_workspace_edit_with_yes_short_form() {
    let cli = Cli::try_parse_from(["jackin", "workspace", "edit", "proj-alpha", "-y"]).unwrap();
    match cli.command {
        Some(Command::Workspace(WorkspaceCommand::Edit { assume_yes, .. })) => {
            assert!(assume_yes);
        }
        other => panic!("unexpected command {other:?}"),
    }
}

#[test]
fn parses_workspace_prune_command() {
    let cli = Cli::try_parse_from(["jackin", "workspace", "prune", "proj-alpha"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Workspace(WorkspaceCommand::Prune { .. }))
    ));
}

#[test]
fn parses_workspace_prune_with_yes() {
    let cli = Cli::try_parse_from(["jackin", "workspace", "prune", "proj-alpha", "--yes"]).unwrap();
    match cli.command {
        Some(Command::Workspace(WorkspaceCommand::Prune { assume_yes, .. })) => {
            assert!(assume_yes);
        }
        other => panic!("unexpected command {other:?}"),
    }
}

// ── help subcommand disabled ────────────────────────────────────────

#[test]
fn workspace_rejects_help_subcommand() {
    let err = Cli::try_parse_from(["jackin", "workspace", "help"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
}

// ── Workspace subcommand help ───────────────────────────────────────

#[test]
fn workspace_create_help_shows_explicit_mounts_and_examples() {
    let help = help_text(&["jackin", "workspace", "create", "--help"]);
    assert!(
        help.contains("not mounted implicitly"),
        "explicit mount behavior not documented"
    );
    assert!(help.contains("Examples:"));
    assert!(help.contains(
        "jackin workspace create my-app --workdir ~/Projects/my-app --mount ~/Projects/my-app"
    ));
}

#[test]
fn workspace_create_help_shows_mount_format() {
    let help = help_text(&["jackin", "workspace", "create", "--help"]);
    assert!(
        help.contains("path[:ro]") && help.contains("src:dst[:ro]"),
        "mount format missing"
    );
}

#[test]
fn workspace_edit_help_shows_examples() {
    let help = help_text(&["jackin", "workspace", "edit", "--help"]);
    assert!(help.contains("Modify an existing workspace"));
    assert!(help.contains("Examples:"));
    assert!(help.contains("jackin workspace edit my-app --workdir ~/new-dir"));
    assert!(help.contains("--clear-default-role"));
}

#[test]
fn workspace_edit_help_shows_mount_format() {
    let help = help_text(&["jackin", "workspace", "edit", "--help"]);
    assert!(
        help.contains("path[:ro]") && help.contains("src:dst[:ro]"),
        "mount format missing"
    );
}

#[test]
fn workspace_show_help_shows_examples() {
    let help = help_text(&["jackin", "workspace", "show", "--help"]);
    assert!(help.contains("jackin workspace show my-app"));
}

#[test]
fn workspace_remove_help_shows_examples() {
    let help = help_text(&["jackin", "workspace", "remove", "--help"]);
    assert!(help.contains("jackin workspace remove my-app"));
}

#[test]
fn parse_mount_isolation_accepts_worktree() {
    let (dst, mode) = parse_mount_isolation("/workspace/jackin=worktree").unwrap();
    assert_eq!(dst, "/workspace/jackin");
    assert_eq!(mode, MountIsolation::Worktree);
}

#[test]
fn parse_mount_isolation_accepts_clone() {
    let (dst, mode) = parse_mount_isolation("/workspace/jackin=clone").unwrap();
    assert_eq!(dst, "/workspace/jackin");
    assert_eq!(mode, MountIsolation::Clone);
}

#[test]
fn parse_mount_isolation_rejects_missing_equals() {
    let err = parse_mount_isolation("/workspace/jackin").unwrap_err();
    assert!(err.to_string().contains("expected DST=TYPE"));
}

#[test]
fn parse_mount_isolation_rejects_empty_dst() {
    let err = parse_mount_isolation("=worktree").unwrap_err();
    assert!(err.to_string().contains("destination cannot be empty"));
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `role`.
use crate::cli::{Cli, Command};
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
fn load_args_parses_agent_flag() {
    let cli = Cli::try_parse_from(["jackin", "load", "agent-smith", "--agent", "codex"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(super::LoadArgs {
            agent: Some(jackin_core::Agent::Codex),
            ..
        }))
    ));
}

#[test]
fn load_args_parses_amp_agent_flag() {
    let cli = Cli::try_parse_from(["jackin", "load", "agent-smith", "--agent", "amp"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(super::LoadArgs {
            agent: Some(jackin_core::Agent::Amp),
            ..
        }))
    ));
}

#[test]
fn load_args_rejects_unknown_agent() {
    let res = Cli::try_parse_from(["jackin", "load", "agent-smith", "--agent", "foo"]);
    assert!(res.is_err());
}

#[test]
fn load_args_agent_optional() {
    let cli = Cli::try_parse_from(["jackin", "load", "agent-smith"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(super::LoadArgs { agent: None, .. }))
    ));
}

#[test]
fn load_args_parses_branch_flag() {
    let cli = Cli::try_parse_from([
        "jackin",
        "load",
        "the-architect",
        "--role-branch",
        "feat/my-pr",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(super::LoadArgs {
            role_branch: Some(ref b),
            ..
        })) if b == "feat/my-pr"
    ));
}

#[test]
fn load_args_branch_optional() {
    let cli = Cli::try_parse_from(["jackin", "load", "the-architect"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(super::LoadArgs {
            role_branch: None,
            ..
        }))
    ));
}

#[test]
fn parses_load_command() {
    let cli = Cli::try_parse_from(["jackin", "load", "agent-smith"]).unwrap();
    // `debug` is omitted from the pattern: it is env-backed
    // (`JACKIN_DEBUG`), so its default depends on the runner's env.
    // `tests/cli_debug_env.rs` covers the env-driven behavior.
    assert!(matches!(
        cli.command,
        Some(Command::Load(super::LoadArgs {
            selector: Some(ref s),
            target: None,
            ..
        })) if s == "agent-smith"
    ));
}

#[test]
fn parses_load_without_selector() {
    let cli = Cli::try_parse_from(["jackin", "load"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(super::LoadArgs {
            selector: None,
            target: None,
            ..
        }))
    ));
}

#[test]
fn parses_load_rebuild_without_selector() {
    let cli = Cli::try_parse_from(["jackin", "load", "--rebuild"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(super::LoadArgs {
            selector: None,
            rebuild: true,
            ..
        }))
    ));
}

#[test]
fn parses_load_with_target_path() {
    let cli = Cli::try_parse_from(["jackin", "load", "agent-smith", "~/Projects/my-app"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(super::LoadArgs {
            target: Some(ref t),
            ..
        })) if t == "~/Projects/my-app"
    ));
}

#[test]
fn parses_load_with_target_and_mount() {
    let cli = Cli::try_parse_from([
        "jackin",
        "load",
        "agent-smith",
        "big-monorepo",
        "--mount",
        "/tmp/cache:/workspace/cache:ro",
    ])
    .unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Load(super::LoadArgs {
            target: Some(ref t),
            ref mounts,
            ..
        })) if t == "big-monorepo" && mounts.len() == 1
    ));
}

#[test]
fn parses_load_with_mount_only() {
    let cli = Cli::try_parse_from([
        "jackin",
        "load",
        "agent-smith",
        "--mount",
        "/tmp/project:/workspace/project",
        "--mount",
        "/tmp/cache:/workspace/cache:ro",
    ])
    .unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Load(super::LoadArgs {
            target: None,
            ref mounts,
            ..
        })) if mounts.len() == 2
    ));
}

#[test]
fn parses_console_command() {
    let cli = Cli::try_parse_from(["jackin", "console"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Console(super::ConsoleArgs { .. }))
    ));
}

#[test]
fn parses_console_with_debug() {
    let cli = Cli::try_parse_from(["jackin", "console", "--debug"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Console(super::ConsoleArgs { .. }))
    ));
    // --debug is global on Cli, not on ConsoleArgs.
    assert!(cli.debug);
}

#[test]
fn console_rejects_removed_flags() {
    // The console is always the full experience; the old --no-rain /
    // --no-tui / --intro / --outro toggles no longer exist.
    for flag in ["--no-rain", "--no-tui", "--intro", "--outro"] {
        assert!(
            Cli::try_parse_from(["jackin", "console", flag]).is_err(),
            "console should reject {flag}"
        );
    }
}

#[test]
fn load_rejects_removed_surface_flags() {
    for flag in ["--no-rain", "--no-tui", "--no-intro"] {
        assert!(
            Cli::try_parse_from(["jackin", "load", flag]).is_err(),
            "load should reject {flag}"
        );
    }
}

#[test]
fn parses_bare_jackin_as_no_subcommand() {
    let cli = Cli::try_parse_from(["jackin"]).unwrap();
    assert!(cli.command.is_none());
}

#[test]
fn parses_bare_jackin_with_top_level_debug() {
    let cli = Cli::try_parse_from(["jackin", "--debug"]).unwrap();
    assert!(cli.command.is_none());
    // CLI flag wins over env, so this assertion holds even when
    // `JACKIN_DEBUG=0` is set in the runner's env.
    assert!(cli.debug);
}

// ── Load help ───────────────────────────────────────────────────────

#[test]
fn load_help_shows_description_and_examples() {
    let help = help_text(&["jackin", "load", "--help"]);
    assert!(help.contains("Jack a role into an isolated container"));
    assert!(help.contains("Examples:"));
    assert!(help.contains("jackin load agent-smith"));
    assert!(help.contains("jackin load agent-smith big-monorepo"));
}

#[test]
fn load_help_shows_mount_format() {
    let help = help_text(&["jackin", "load", "--help"]);
    assert!(
        help.contains("path[:ro]") && help.contains("src:dst[:ro]"),
        "mount format missing"
    );
}

// ── Hardline help ───────────────────────────────────────────────────

#[test]
fn hardline_help_shows_examples() {
    let help = help_text(&["jackin", "hardline", "--help"]);
    assert!(help.contains("Reattach to a running role"));
    assert!(help.contains("jackin hardline agent-smith"));
    assert!(
        help.contains("jackin hardline ") && help.contains("auto-detect workspace"),
        "missing no-arg usage in hardline help: {help}"
    );
}

#[test]
fn parses_hardline_without_selector() {
    let cli = Cli::try_parse_from(["jackin", "hardline"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Hardline(super::HardlineArgs {
            selector: None,
            inspect: false,
            new: false,
            agent: None,
            shell: false,
        }))
    ));
}

#[test]
fn parses_hardline_with_selector() {
    let cli = Cli::try_parse_from(["jackin", "hardline", "agent-smith"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Hardline(super::HardlineArgs {
            selector: Some(ref s),
            inspect: false,
            new: false,
            agent: None,
            shell: false,
        })) if s == "agent-smith"
    ));
}

#[test]
fn parses_hardline_inspect_flag() {
    let cli = Cli::try_parse_from(["jackin", "hardline", "--inspect", "k7p9m2xq"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Hardline(super::HardlineArgs {
            selector: Some(ref s),
            inspect: true,
            new: false,
            agent: None,
            shell: false,
        })) if s == "k7p9m2xq"
    ));
}

#[test]
fn parses_hardline_new_agent_flags() {
    let cli = Cli::try_parse_from(["jackin", "hardline", "--new", "--agent", "codex"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Hardline(super::HardlineArgs {
            selector: None,
            inspect: false,
            new: true,
            agent: Some(jackin_core::Agent::Codex),
            shell: false,
        }))
    ));
}

#[test]
fn parses_hardline_shell_flag() {
    let cli = Cli::try_parse_from(["jackin", "hardline", "--shell"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Hardline(super::HardlineArgs {
            selector: None,
            inspect: false,
            new: false,
            agent: None,
            shell: true,
        }))
    ));
}

#[test]
fn rejects_hardline_shell_with_new() {
    let res = Cli::try_parse_from(["jackin", "hardline", "--shell", "--new"]);
    assert!(res.is_err());
}

#[test]
fn rejects_hardline_shell_with_inspect() {
    let res = Cli::try_parse_from(["jackin", "hardline", "--shell", "--inspect"]);
    assert!(res.is_err());
}

#[test]
fn rejects_hardline_agent_without_new() {
    let res = Cli::try_parse_from(["jackin", "hardline", "--agent", "codex"]);

    assert!(res.is_err());
}

#[test]
fn rejects_hardline_inspect_with_new() {
    let res = Cli::try_parse_from(["jackin", "hardline", "--inspect", "--new"]);

    assert!(res.is_err());
}

#[test]
fn parses_role_validate_with_default_path() {
    let cli = Cli::try_parse_from(["jackin", "role", "validate"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(super::RoleCommand::Validate(
            super::RoleRepoPathArgs { path: None }
        )))
    ));
}

#[test]
fn parses_role_migrate_with_path() {
    let cli = Cli::try_parse_from(["jackin", "role", "migrate", "/tmp/my-role"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(super::RoleCommand::Migrate(
            super::RoleRepoPathArgs { path: Some(ref path) }
        ))) if path == std::path::Path::new("/tmp/my-role")
    ));
}

#[test]
fn parses_role_construct_version_with_default_path() {
    let cli = Cli::try_parse_from(["jackin", "role", "construct-version"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(super::RoleCommand::ConstructVersion(
            super::RoleRepoPathArgs { path: None }
        )))
    ));
}

#[test]
fn parses_role_construct_version_with_path() {
    let cli = Cli::try_parse_from(["jackin", "role", "construct-version", "/tmp/my-role"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(super::RoleCommand::ConstructVersion(
            super::RoleRepoPathArgs { path: Some(ref p) }
        ))) if p == std::path::Path::new("/tmp/my-role")
    ));
}

#[test]
fn parses_role_published_image_repository_with_path() {
    let cli = Cli::try_parse_from([
        "jackin",
        "role",
        "published-image-repository",
        "/tmp/my-role",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(super::RoleCommand::PublishedImageRepository(
            super::RoleRepoPathArgs { path: Some(ref p) }
        ))) if p == std::path::Path::new("/tmp/my-role")
    ));
}

#[test]
fn parses_role_publish_labels_with_path() {
    let cli = Cli::try_parse_from([
        "jackin",
        "role",
        "publish-labels",
        "--role-git-sha",
        "abc123",
        "/tmp/my-role",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(super::RoleCommand::PublishLabels(
            super::RolePublishLabelsArgs {
                ref role_git_sha,
                path: Some(ref p),
            }
        ))) if role_git_sha == "abc123" && p == std::path::Path::new("/tmp/my-role")
    ));
}

#[test]
fn parses_role_create_with_projects_dir() {
    let cli = Cli::try_parse_from(["jackin", "role", "create", "ChainArgos/Backend", "."]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Role(super::RoleCommand::Create(
            super::RoleCreateArgs {
                ref role,
                projects_dir: Some(ref path),
            }
        ))) if role == "ChainArgos/Backend" && path == std::path::Path::new(".")
    ));
}

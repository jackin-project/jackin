// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parses_bare_usage() {
    let cli = Cli::try_parse_from(["jackin", "usage"]).unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Usage(ref args))
            if args.instance.is_none() && args.scope.is_none() && args.format == "human"
    ));
}

#[test]
fn parses_usage_cache_accounts_json() {
    let cli =
        Cli::try_parse_from(["jackin", "usage", "cache", "accounts", "--format", "json"]).unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Usage(ref args))
            if args.instance.as_deref() == Some("cache")
                && args.format == "json"
                && matches!(args.scope, Some(usage::UsageScope::Accounts(_)))
    ));
}

#[test]
fn parses_usage_verify() {
    let cli = Cli::try_parse_from(["jackin", "usage", "jk-demo-role", "verify"]).unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Usage(ref args))
            if args.instance.as_deref() == Some("jk-demo-role")
                && matches!(args.scope, Some(usage::UsageScope::Verify))
    ));
}

#[test]
fn parses_prewarm_agent_filters() {
    let cli =
        Cli::try_parse_from(["jackin", "prewarm", "--agent", "claude", "--agent", "kimi"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.agents == [jackin_core::Agent::Claude, jackin_core::Agent::Kimi]
    ));
}

#[test]
fn parses_prewarm_image_role_filters() {
    let cli = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--image",
        "--role",
        "agent-smith",
        "--role-git",
        "https://example.invalid/agent-smith.git",
        "--role-branch",
        "feat/launch-speed",
        "--agent",
        "codex",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.flags.image
                && args.role.as_deref() == Some("agent-smith")
                && args.role_git.as_deref() == Some("https://example.invalid/agent-smith.git")
                && args.role_branch.as_deref() == Some("feat/launch-speed")
                && args.agents == [jackin_core::Agent::Codex]
    ));
}

#[test]
fn parses_prewarm_roles_single_role_filter() {
    let cli = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--roles",
        "--role",
        "agent-smith",
        "--role-git",
        "https://example.invalid/agent-smith.git",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.flags.roles
                && !args.flags.image
                && args.role.as_deref() == Some("agent-smith")
                && args.role_git.as_deref() == Some("https://example.invalid/agent-smith.git")
    ));
}

#[test]
fn parses_prewarm_roles_workspace_filter() {
    let cli =
        Cli::try_parse_from(["jackin", "prewarm", "--roles", "--workspace", "jackin"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.flags.roles
                && !args.flags.image
                && args.workspace.as_deref() == Some("jackin")
                && args.role.is_none()
                && !args.flags.all_workspaces
    ));
}

#[test]
fn parses_prewarm_roles_all_workspaces_filter() {
    let cli = Cli::try_parse_from(["jackin", "prewarm", "--roles", "--all-workspaces"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.flags.roles
                && !args.flags.image
                && args.workspace.is_none()
                && args.role.is_none()
                && args.flags.all_workspaces
    ));
}

#[test]
fn parses_prewarm_sidecar_filter() {
    let cli = Cli::try_parse_from(["jackin", "prewarm", "--sidecar"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args)) if args.flags.sidecar && !args.flags.image && !args.flags.roles
    ));
}

#[test]
fn parses_prewarm_sidecar_container_keep_filter() {
    let cli = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--sidecar-container",
        "--keep-sidecar-container",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.flags.sidecar_container && args.flags.keep_sidecar_container
    ));
}

#[test]
fn parses_prewarm_daemon_filter() {
    let cli = Cli::try_parse_from(["jackin", "prewarm", "--daemon"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.flags.daemon && !args.flags.sidecar && !args.flags.sidecar_container && !args.flags.keep_sidecar_container
    ));
}

#[test]
fn parses_prewarm_image_workspace_filters() {
    let cli = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--image",
        "--workspace",
        "jackin",
        "--role-branch",
        "feat/launch-speed",
        "--agent",
        "claude",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.flags.image
                && args.workspace.as_deref() == Some("jackin")
                && !args.flags.all_workspaces
                && args.role.is_none()
                && args.role_git.is_none()
                && args.role_branch.as_deref() == Some("feat/launch-speed")
                && args.agents == [jackin_core::Agent::Claude]
    ));
}

#[test]
fn parses_prewarm_image_all_workspaces() {
    let cli = Cli::try_parse_from(["jackin", "prewarm", "--image", "--all-workspaces"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.flags.image
                && args.flags.all_workspaces
                && args.workspace.is_none()
                && args.role.is_none()
                && args.role_git.is_none()
    ));
}

#[test]
fn rejects_prewarm_image_workspace_with_role() {
    let err = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--image",
        "--workspace",
        "jackin",
        "--role",
        "the-architect",
    ])
    .unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn rejects_prewarm_image_all_workspaces_with_workspace() {
    let err = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--image",
        "--all-workspaces",
        "--workspace",
        "jackin",
    ])
    .unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn rejects_prewarm_image_workspace_with_role_git_override() {
    let err = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--image",
        "--workspace",
        "jackin",
        "--role-git",
        "https://example.invalid/role.git",
    ])
    .unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn rejects_prewarm_keep_sidecar_container_without_sidecar_container() {
    let err = Cli::try_parse_from(["jackin", "prewarm", "--keep-sidecar-container"]).unwrap_err();
    // `--keep-sidecar-container` requires `--sidecar-container`.
    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    let msg = strip_ansi(&err.to_string());
    assert!(
        msg.contains("--sidecar-container"),
        "error should name --sidecar-container: {msg:?}"
    );
}

#[test]
fn rejects_prewarm_role_with_all_workspaces() {
    let err = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--role",
        "architect",
        "--all-workspaces",
    ])
    .unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn rejects_prewarm_workspace_with_all_workspaces() {
    let err = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--workspace",
        "demo",
        "--all-workspaces",
    ])
    .unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn rejects_prewarm_role_with_all_roles() {
    let err = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--image",
        "--role",
        "architect",
        "--all-roles",
    ])
    .unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn rejects_prewarm_workspace_with_all_roles() {
    let err = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--image",
        "--workspace",
        "demo",
        "--all-roles",
    ])
    .unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn rejects_prewarm_role_git_with_all_workspaces() {
    let err = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--role",
        "architect",
        "--role-git",
        "https://example.invalid/role.git",
        "--all-workspaces",
    ])
    .unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn rejects_prewarm_all_roles_without_image() {
    let err = Cli::try_parse_from(["jackin", "prewarm", "--all-roles"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
}

#[test]
fn parses_prewarm_image_all_roles() {
    let cli = Cli::try_parse_from(["jackin", "prewarm", "--image", "--all-roles"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.flags.image && args.flags.all_roles
    ));
}

#[test]
fn parses_prewarm_image_role_git() {
    let cli = Cli::try_parse_from([
        "jackin",
        "prewarm",
        "--image",
        "--role",
        "architect",
        "--role-git",
        "https://example.invalid/role.git",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.role.as_deref() == Some("architect")
                && args.role_git.as_deref() == Some("https://example.invalid/role.git")
                && args.flags.image
    ));
}

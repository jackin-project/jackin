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
fn parses_usage_doctor_unattended() {
    let cli = Cli::try_parse_from([
        "jackin",
        "usage",
        "doctor",
        "--provider",
        "claude",
        "--unattended",
    ])
    .unwrap();
    assert!(matches!(cli.command, Some(Command::Usage(ref args))
        if args.instance.is_none()
            && matches!(args.scope, Some(usage::UsageScope::Doctor(ref doctor))
                if doctor.provider == usage::UsageProviderArg::Claude && doctor.unattended)));
}

#[test]
fn parses_usage_auth_prepare_with_keychain_service() {
    let cli = Cli::try_parse_from([
        "jackin",
        "usage",
        "auth",
        "prepare",
        "--provider",
        "claude",
        "--keychain-service",
        "Claude custom",
    ])
    .unwrap();
    assert!(matches!(cli.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Auth(ref auth))
            if matches!(auth.command, usage::UsageAuthCommand::Prepare(ref prepare)
                if prepare.provider == usage::UsageProviderArg::Claude
                    && prepare.keychain_service.as_deref() == Some("Claude custom")))));
}

#[test]
fn parses_usage_monitor_start_with_isolated_data_dir() {
    let cli = Cli::try_parse_from([
        "jackin",
        "usage",
        "--data-dir",
        "/tmp/usage-data",
        "monitor",
        "start",
        "--provider",
        "claude",
        "--account",
        "account-1",
        "--goal",
        "goal-1",
        "--session",
        "session-1",
        "--budget-sgd",
        "50.25",
    ])
    .unwrap();
    assert!(matches!(cli.command, Some(Command::Usage(ref args))
        if args.data_dir.as_deref() == Some(std::path::Path::new("/tmp/usage-data"))
            && matches!(args.scope, Some(usage::UsageScope::Monitor(ref monitor))
                if matches!(monitor.command, usage::UsageMonitorCommand::Start(ref start)
                    if start.provider == usage::UsageProviderArg::Claude
                        && start.account == "account-1"
                        && start.goal == "goal-1"
                        && start.session.as_deref() == Some("session-1")
                        && start.budget_sgd.as_deref() == Some("50.25")))));
}

#[test]
fn parses_usage_statusline_ingest() {
    let cli = Cli::try_parse_from([
        "jackin",
        "usage",
        "statusline",
        "ingest",
        "--account",
        "account-1",
        "--format",
        "json",
        "--data-dir",
        "/tmp/usage-data",
    ])
    .unwrap();
    assert!(matches!(cli.command, Some(Command::Usage(ref args))
        if args.format == "json"
            && args.data_dir.as_deref() == Some(std::path::Path::new("/tmp/usage-data"))
            && matches!(args.scope, Some(usage::UsageScope::Statusline(ref statusline))
                if matches!(statusline.command, usage::UsageStatuslineCommand::Ingest(ref ingest)
                    if ingest.account == "account-1"))));
}

#[test]
fn parses_usage_status_wait_and_required_condition() {
    let cli = Cli::try_parse_from([
        "jackin",
        "usage",
        "wait",
        "--monitor",
        "monitor-1",
        "--until",
        "runnable",
        "--timeout-secs",
        "1",
    ])
    .unwrap();
    assert!(matches!(cli.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Wait(ref wait))
            if wait.monitor == "monitor-1"
                && wait.until == usage::UsageWaitCondition::Runnable
                && wait.timeout_secs == 1)));

    let _error =
        Cli::try_parse_from(["jackin", "usage", "wait", "--monitor", "monitor-1"]).unwrap_err();
}

#[test]
fn parses_usage_top_level_status_and_refresh() {
    let status =
        Cli::try_parse_from(["jackin", "usage", "status", "--monitor", "monitor-1"]).unwrap();
    assert!(matches!(status.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Status(ref status))
            if status.monitor == "monitor-1")));

    let refresh =
        Cli::try_parse_from(["jackin", "usage", "refresh", "--monitor", "monitor-1"]).unwrap();
    assert!(matches!(refresh.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Refresh(ref refresh))
            if refresh.monitor == "monitor-1")));
}

#[test]
fn parses_usage_service_start() {
    let cli = Cli::try_parse_from(["jackin", "usage", "service", "start"]).unwrap();
    assert!(matches!(cli.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Service(ref service))
            if matches!(service.command, usage::UsageServiceCommand::Start))));
}

#[test]
fn parses_usage_watch_with_bounded_duration() {
    let cli = Cli::try_parse_from([
        "jackin",
        "usage",
        "watch",
        "--monitor",
        "monitor-1",
        "--timeout-secs",
        "1",
    ])
    .unwrap();
    assert!(matches!(cli.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Watch(ref watch))
            if watch.monitor == "monitor-1" && watch.timeout_secs == Some(1))));
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

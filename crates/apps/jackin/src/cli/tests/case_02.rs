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
        "--binding",
        "binding-1",
        "--binding-revision",
        "4",
        "--goal",
        "goal-1",
        "--policy-revision",
        "2",
        "--idempotency-key",
        "run-1",
        "--session",
        "session-1",
    ])
    .unwrap();
    assert!(matches!(cli.command, Some(Command::Usage(ref args))
        if args.data_dir.as_deref() == Some(std::path::Path::new("/tmp/usage-data"))
            && matches!(args.scope, Some(usage::UsageScope::Monitor(ref monitor))
                if matches!(monitor.command, usage::UsageMonitorCommand::Start(ref start)
                    if start.provider == usage::UsageProviderArg::Claude
                        && start.binding == "binding-1"
                        && start.binding_revision == 4
                        && start.goal == "goal-1"
                        && start.policy_revision == 2
                        && start.idempotency_key == "run-1"
                        && start.session.as_deref() == Some("session-1")
                        && start.expected_model.is_none()))));
}

#[test]
fn parses_usage_monitor_observe_session_and_bound_scopes() {
    let session = Cli::try_parse_from([
        "jackin",
        "usage",
        "monitor",
        "observe",
        "--provider",
        "claude",
        "--session",
        "session-1",
        "--idempotency-key",
        "observer-1",
    ])
    .unwrap();
    assert!(matches!(session.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Monitor(ref monitor))
            if matches!(monitor.command, usage::UsageMonitorCommand::Observe(ref observe)
                if observe.session.as_deref() == Some("session-1")
                    && observe.binding.is_none()
                    && observe.idempotency_key == "observer-1"))));

    let bound = Cli::try_parse_from([
        "jackin",
        "usage",
        "monitor",
        "observe",
        "--provider",
        "claude",
        "--binding",
        "binding-1",
        "--binding-revision",
        "4",
        "--session",
        "session-1",
        "--idempotency-key",
        "observer-2",
        "--expected-model",
        "claude-sonnet-4",
    ])
    .unwrap();
    assert!(matches!(bound.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Monitor(ref monitor))
            if matches!(monitor.command, usage::UsageMonitorCommand::Observe(ref observe)
                if observe.session.as_deref() == Some("session-1")
                    && observe.binding.as_deref() == Some("binding-1")
                    && observe.binding_revision == Some(4)
                    && observe.expected_model.as_deref() == Some("claude-sonnet-4")))));

    let account_wide = Cli::try_parse_from([
        "jackin",
        "usage",
        "monitor",
        "observe",
        "--provider",
        "claude",
        "--binding",
        "binding-1",
        "--binding-revision",
        "4",
        "--idempotency-key",
        "observer-3",
    ])
    .unwrap();
    assert!(
        matches!(account_wide.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Monitor(ref monitor))
            if matches!(monitor.command, usage::UsageMonitorCommand::Observe(ref observe)
                if observe.session.is_none()
                    && observe.binding.as_deref() == Some("binding-1")
                    && observe.binding_revision == Some(4))))
    );
}

#[test]
fn rejects_monitor_scope_conflicts_and_removed_start_aliases() {
    for args in [
        vec![
            "jackin",
            "usage",
            "monitor",
            "observe",
            "--provider",
            "claude",
            "--idempotency-key",
            "k",
        ],
        vec![
            "jackin",
            "usage",
            "monitor",
            "observe",
            "--provider",
            "claude",
            "--binding",
            "b",
            "--idempotency-key",
            "k",
        ],
        vec![
            "jackin",
            "usage",
            "monitor",
            "start",
            "--provider",
            "claude",
            "--account",
            "a",
            "--goal",
            "g",
            "--budget-sgd",
            "50",
        ],
        vec![
            "jackin",
            "usage",
            "monitor",
            "start",
            "--provider",
            "claude",
            "--binding",
            "b",
            "--binding-revision",
            "1",
            "--goal",
            "g",
            "--policy-revision",
            "1",
            "--idempotency-key",
            "k",
            "--account",
            "a",
        ],
        vec![
            "jackin",
            "usage",
            "monitor",
            "start",
            "--provider",
            "claude",
            "--binding",
            "b",
            "--binding-revision",
            "1",
            "--goal",
            "g",
            "--policy-revision",
            "1",
            "--idempotency-key",
            "k",
            "--budget-sgd",
            "50",
        ],
    ] {
        let _error = Cli::try_parse_from(args).unwrap_err();
    }
}

#[test]
fn parses_binding_confirmation_and_policy_approval() {
    let binding = Cli::try_parse_from([
        "jackin",
        "usage",
        "binding",
        "confirm",
        "--provider",
        "claude",
        "--account",
        "account-1",
        "--operator-label",
        "Work Claude",
        "--confirm",
    ])
    .unwrap();
    assert!(matches!(binding.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Binding(ref binding))
            if matches!(binding.command, usage::UsageBindingCommand::Confirm(ref confirm)
                if confirm.provider == usage::UsageProviderArg::Claude
                    && confirm.account == "account-1"
                    && confirm.operator_label == "Work Claude"
                    && confirm.confirm))));

    let policy = Cli::try_parse_from([
        "jackin",
        "usage",
        "policy",
        "approve",
        "--binding",
        "binding-1",
        "--binding-revision",
        "4",
        "--goal",
        "goal-1",
        "--policy",
        "strict-sgd",
        "--operator-label",
        "Cost reviewed",
        "--confirm",
    ])
    .unwrap();
    assert!(matches!(policy.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Policy(ref policy))
            if matches!(policy.command, usage::UsagePolicyCommand::Approve(ref approve)
                if approve.binding == "binding-1"
                    && approve.binding_revision == 4
                    && approve.goal == "goal-1"
                    && approve.policy == usage::UsageMonitorPolicyArg::StrictSgd
                    && approve.budget_sgd.is_none()
                    && approve.operator_label == "Cost reviewed"
                    && approve.confirm))));

    let quota_only = Cli::try_parse_from([
        "jackin",
        "usage",
        "policy",
        "approve",
        "--binding",
        "binding-1",
        "--binding-revision",
        "4",
        "--goal",
        "goal-1",
        "--policy",
        "quota-only",
        "--operator-label",
        "No SGD cap accepted",
        "--confirm",
        "--acknowledge-no-sgd-cap",
    ])
    .unwrap();
    assert!(matches!(quota_only.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Policy(ref policy))
            if matches!(policy.command, usage::UsagePolicyCommand::Approve(ref approve)
                if approve.policy == usage::UsageMonitorPolicyArg::QuotaOnly
                    && approve.acknowledge_no_sgd_cap))));
}

#[test]
fn rejects_zero_sgd_budget_during_policy_argument_parsing() {
    for budget in ["0", "0.00", "00.00"] {
        let parsed = Cli::try_parse_from([
            "jackin",
            "usage",
            "policy",
            "approve",
            "--binding",
            "binding-1",
            "--binding-revision",
            "4",
            "--goal",
            "goal-1",
            "--policy",
            "strict-sgd",
            "--budget-sgd",
            budget,
            "--operator-label",
            "Cost reviewed",
            "--confirm",
        ]);
        let _error = parsed.unwrap_err();
    }

    Cli::try_parse_from([
        "jackin",
        "usage",
        "policy",
        "approve",
        "--binding",
        "binding-1",
        "--binding-revision",
        "4",
        "--goal",
        "goal-1",
        "--policy",
        "strict-sgd",
        "--budget-sgd",
        "0.01",
        "--operator-label",
        "Cost reviewed",
        "--confirm",
    ])
    .unwrap();
}

#[test]
fn parses_usage_statusline_ingest_and_compose_scopes() {
    let cli = Cli::try_parse_from([
        "jackin",
        "usage",
        "statusline",
        "ingest",
        "--session-only",
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
                    if ingest.scope.session_only && ingest.scope.binding.is_none()))));

    let bound = Cli::try_parse_from([
        "jackin",
        "usage",
        "statusline",
        "compose",
        "--settings",
        "/tmp/settings.json",
        "--binding",
        "binding-1",
        "--binding-revision",
        "4",
    ])
    .unwrap();
    assert!(matches!(bound.command, Some(Command::Usage(ref args))
        if matches!(args.scope, Some(usage::UsageScope::Statusline(ref statusline))
            if matches!(statusline.command, usage::UsageStatuslineCommand::Compose(ref compose)
                if compose.settings.as_path() == std::path::Path::new("/tmp/settings.json")
                    && !compose.scope.session_only
                    && compose.scope.binding.as_deref() == Some("binding-1")
                    && compose.scope.binding_revision == Some(4)))));
}

#[test]
fn rejects_statusline_scope_conflicts_and_missing_confirm() {
    for args in [
        vec!["jackin", "usage", "statusline", "ingest"],
        vec!["jackin", "usage", "statusline", "ingest", "--binding", "b"],
        vec![
            "jackin",
            "usage",
            "statusline",
            "ingest",
            "--session-only",
            "--binding",
            "b",
            "--binding-revision",
            "1",
        ],
        vec![
            "jackin",
            "usage",
            "statusline",
            "ingest",
            "--session-only",
            "--account",
            "a",
        ],
        vec![
            "jackin",
            "usage",
            "statusline",
            "compose",
            "--settings",
            "/tmp/settings.json",
            "--session-only",
            "--binding",
            "b",
            "--binding-revision",
            "1",
        ],
        vec![
            "jackin",
            "usage",
            "binding",
            "confirm",
            "--provider",
            "claude",
            "--account",
            "a",
            "--operator-label",
            "L",
        ],
        vec![
            "jackin",
            "usage",
            "binding",
            "confirm",
            "--provider",
            "claude",
            "--account",
            "a",
            "--operator-label",
            "L",
            "--confirm",
            "--unattended",
        ],
        vec![
            "jackin",
            "usage",
            "policy",
            "approve",
            "--binding",
            "b",
            "--binding-revision",
            "1",
            "--goal",
            "g",
            "--policy",
            "quota-only",
            "--operator-label",
            "L",
        ],
    ] {
        let _error = Cli::try_parse_from(args).unwrap_err();
    }
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

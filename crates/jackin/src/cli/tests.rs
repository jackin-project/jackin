// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `cli`.
use super::*;

#[test]
fn telemetry_command_vocabulary_exactly_matches_live_cli_tree() {
    use clap::CommandFactory as _;
    use std::collections::BTreeSet;

    fn collect(command: &clap::Command, prefix: Option<&str>, names: &mut BTreeSet<String>) {
        for subcommand in command.get_subcommands() {
            let name = match prefix {
                Some(prefix) => format!("{prefix}.{}", subcommand.get_name()),
                None => subcommand.get_name().to_owned(),
            };
            names.insert(name.clone());
            collect(subcommand, Some(&name), names);
        }
    }

    let mut live = BTreeSet::new();
    collect(&Cli::command(), None, &mut live);
    let governed = jackin_telemetry::schema::enums::CliCommandName::ALL
        .iter()
        .map(|name| name.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(governed, live);
}

#[test]
fn telemetry_command_mapper_covers_every_nested_leaf() {
    use jackin_telemetry::schema::enums::CliCommandName as Name;

    let cases: &[(&[&str], Name)] = &[
        (&["prune", "roles"], Name::PruneRoles),
        (&["prune", "cache"], Name::PruneCache),
        (&["prune", "images"], Name::PruneImages),
        (&["prune", "instances"], Name::PruneInstances),
        (&["prune", "system"], Name::PruneSystem),
        (&["role", "validate"], Name::RoleValidate),
        (&["role", "migrate"], Name::RoleMigrate),
        (&["role", "create", "sample"], Name::RoleCreate),
        (&["role", "construct-version"], Name::RoleConstructVersion),
        (&["role", "published-image"], Name::RolePublishedImage),
        (
            &["role", "published-image-repository"],
            Name::RolePublishedImageRepository,
        ),
        (
            &["role", "publish-labels", "--role-git-sha", "abc"],
            Name::RolePublishLabels,
        ),
        (
            &[
                "workspace",
                "create",
                "sample",
                "--workdir",
                "/w",
                "--mount",
                "/w",
            ],
            Name::WorkspaceCreate,
        ),
        (&["workspace", "list"], Name::WorkspaceList),
        (&["workspace", "show", "sample"], Name::WorkspaceShow),
        (&["workspace", "edit", "sample"], Name::WorkspaceEdit),
        (&["workspace", "prune", "sample"], Name::WorkspacePrune),
        (&["workspace", "remove", "sample"], Name::WorkspaceRemove),
        (
            &["workspace", "env", "set", "sample", "KEY", "value"],
            Name::WorkspaceEnvSet,
        ),
        (
            &["workspace", "env", "unset", "sample", "KEY"],
            Name::WorkspaceEnvUnset,
        ),
        (
            &["workspace", "env", "list", "sample"],
            Name::WorkspaceEnvList,
        ),
        (
            &["workspace", "claude-token", "setup", "sample", "--plain"],
            Name::WorkspaceClaudeTokenSetup,
        ),
        (
            &["workspace", "claude-token", "rotate", "sample"],
            Name::WorkspaceClaudeTokenRotate,
        ),
        (
            &["workspace", "claude-token", "revoke", "sample"],
            Name::WorkspaceClaudeTokenRevoke,
        ),
        (
            &["workspace", "claude-token", "doctor", "sample"],
            Name::WorkspaceClaudeTokenDoctor,
        ),
        (
            &[
                "config", "mount", "add", "cache", "--src", "/a", "--dst", "/b",
            ],
            Name::ConfigMountAdd,
        ),
        (
            &["config", "mount", "remove", "cache"],
            Name::ConfigMountRemove,
        ),
        (&["config", "mount", "list"], Name::ConfigMountList),
        (
            &["config", "trust", "grant", "sample"],
            Name::ConfigTrustGrant,
        ),
        (
            &["config", "trust", "revoke", "sample"],
            Name::ConfigTrustRevoke,
        ),
        (&["config", "trust", "list"], Name::ConfigTrustList),
        (&["config", "auth", "set", "sync"], Name::ConfigAuthSet),
        (&["config", "auth", "show"], Name::ConfigAuthShow),
        (
            &["config", "env", "set", "KEY", "value"],
            Name::ConfigEnvSet,
        ),
        (&["config", "env", "unset", "KEY"], Name::ConfigEnvUnset),
        (&["config", "env", "list"], Name::ConfigEnvList),
        (
            &["config", "git", "coauthor-trailer", "enable"],
            Name::ConfigGitCoauthorTrailerEnable,
        ),
        (
            &["config", "git", "coauthor-trailer", "disable"],
            Name::ConfigGitCoauthorTrailerDisable,
        ),
        (
            &["config", "git", "dco", "enable"],
            Name::ConfigGitDcoEnable,
        ),
        (
            &["config", "git", "dco", "disable"],
            Name::ConfigGitDcoDisable,
        ),
        #[cfg(unix)]
        (&["daemon", "serve"], Name::DaemonServe),
        #[cfg(unix)]
        (&["daemon", "install"], Name::DaemonInstall),
        #[cfg(unix)]
        (&["daemon", "uninstall"], Name::DaemonUninstall),
        #[cfg(unix)]
        (&["daemon", "start"], Name::DaemonStart),
        #[cfg(unix)]
        (&["daemon", "stop"], Name::DaemonStop),
        #[cfg(unix)]
        (&["daemon", "restart"], Name::DaemonRestart),
        #[cfg(unix)]
        (&["daemon", "status"], Name::DaemonStatus),
        (&["diagnostics", "validate"], Name::DiagnosticsValidate),
        (&["usage", "target", "accounts"], Name::UsageAccounts),
        (&["usage", "target", "verify"], Name::UsageVerify),
        (&["usage", "host", "snapshot", "--agent", "claude"], Name::Usage),
    ];

    for (args, expected) in cases {
        let parsed = Cli::try_parse_from(std::iter::once("jackin").chain(args.iter().copied()))
            .unwrap_or_else(|error| panic!("failed to parse {args:?}: {error}"));
        let command = parsed.command.as_ref().expect("nested command");
        assert_eq!(command_name(command), *expected, "{args:?}");
    }
}

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

// ── Banner tests ────────────────────────────────────────────────────

#[test]
fn root_help_clap_render_has_no_before_help_pill() {
    // The root command intentionally carries no clap `before_help`: the binary
    // prints the brand mark (frozen-rain banner or pill) itself, so clap's own
    // root render leads with the about text, not the pill. (Subcommands keep
    // their pill — see `all_subcommand_help_pages_show_banner`.) The binary-level
    // brand mark is covered by the `root_help_leads_with_brand_mark` integration
    // test.
    let help = help_text(&["jackin", "--help"]);
    assert!(
        !help.trim_start().starts_with("jackin❯"),
        "root clap render should not embed the pill: {help:?}"
    );
}

#[test]
fn root_help_shows_all_commands() {
    let help = help_text(&["jackin", "--help"]);
    assert!(
        help.contains("Operator's CLI for orchestrating AI coding roles in isolated containers")
    );
    for cmd in [
        "load",
        "hardline",
        "eject",
        "exile",
        "purge",
        "prewarm",
        "prune",
        "console",
        "role",
        "workspace",
        "config",
        "usage",
    ] {
        assert!(help.contains(cmd), "missing command: {cmd}");
    }
}

#[test]
fn removed_local_artifact_commands_stay_out_of_help() {
    let root = help_text(&["jackin", "--help"]);
    assert!(
        !root.contains("\n  logs"),
        "root help revived `logs`: {root}"
    );

    let diagnostics = help_text(&["jackin", "diagnostics", "--help"]);
    assert!(diagnostics.contains("\n  validate"));
    for removed in ["summary", "compare", "follow", "reveal", "bundle"] {
        assert!(
            !diagnostics.contains(&format!("\n  {removed}")),
            "diagnostics help revived `{removed}`: {diagnostics}"
        );
    }

    #[cfg(unix)]
    {
        let daemon = help_text(&["jackin", "daemon", "--help"]);
        assert!(
            !daemon.contains("\n  logs"),
            "daemon help revived `logs`: {daemon}"
        );
    }
}

#[test]
fn removed_local_artifact_commands_stay_rejected_by_parser() {
    let mut removed = vec![
        vec!["jackin", "logs"],
        vec!["jackin", "diagnostics", "summary"],
        vec!["jackin", "diagnostics", "compare"],
        vec!["jackin", "diagnostics", "follow"],
        vec!["jackin", "diagnostics", "reveal"],
        vec!["jackin", "diagnostics", "bundle"],
    ];
    #[cfg(unix)]
    removed.push(vec!["jackin", "daemon", "logs"]);

    for args in removed {
        let error = Cli::try_parse_from(&args).expect_err("removed command must not parse");
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::InvalidSubcommand,
            "unexpected parser result for {args:?}: {error}"
        );
    }
}

// ── help subcommand disabled ────────────────────────────────────────

#[test]
fn root_help_lists_help_subcommand() {
    // Our explicit `help` command must appear in the top-level listing.
    let help = help_text(&["jackin", "--help"]);
    assert!(
        help.contains("\n  help "),
        "root `help` subcommand should be listed"
    );
}

#[test]
fn config_help_does_not_list_help_subcommand() {
    let help = help_text(&["jackin", "config", "--help"]);
    assert!(
        !help.contains("\n  help"),
        "`config help` subcommand should be disabled"
    );
}

#[test]
fn workspace_help_does_not_list_help_subcommand() {
    let help = help_text(&["jackin", "workspace", "--help"]);
    assert!(
        !help.contains("\n  help"),
        "`workspace help` subcommand should be disabled"
    );
}

// ── Help command ────────────────────────────────────────────────────

#[test]
fn parses_help_with_no_args() {
    let cli = Cli::try_parse_from(["jackin", "help"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Help { ref command }) if command.is_empty()
    ));
}

#[test]
fn parses_help_with_single_subcommand() {
    let cli = Cli::try_parse_from(["jackin", "help", "config"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Help { ref command }) if command == &["config"]
    ));
}

#[test]
fn parses_help_with_nested_subcommand() {
    let cli = Cli::try_parse_from(["jackin", "help", "config", "auth"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Help { ref command }) if command == &["config", "auth"]
    ));
}

// ── Subcommand banner consistency ───────────────────────────────────

#[test]
fn all_subcommand_help_pages_show_banner() {
    let subcommands = [
        vec!["jackin", "load", "--help"],
        vec!["jackin", "hardline", "--help"],
        vec!["jackin", "eject", "--help"],
        vec!["jackin", "exile", "--help"],
        vec!["jackin", "purge", "--help"],
        vec!["jackin", "prewarm", "--help"],
        vec!["jackin", "prune", "roles", "--help"],
        vec!["jackin", "prune", "cache", "--help"],
        vec!["jackin", "prune", "images", "--help"],
        vec!["jackin", "prune", "instances", "--help"],
        vec!["jackin", "prune", "system", "--help"],
        vec!["jackin", "console", "--help"],
        vec!["jackin", "workspace", "create", "--help"],
        vec!["jackin", "workspace", "list", "--help"],
        vec!["jackin", "workspace", "show", "--help"],
        vec!["jackin", "workspace", "edit", "--help"],
        vec!["jackin", "workspace", "remove", "--help"],
        vec!["jackin", "config", "mount", "add", "--help"],
        vec!["jackin", "config", "mount", "remove", "--help"],
        vec!["jackin", "config", "mount", "list", "--help"],
        vec!["jackin", "config", "auth", "set", "--help"],
        vec!["jackin", "config", "auth", "show", "--help"],
        vec!["jackin", "usage", "--help"],
        vec!["jackin", "usage", "cache", "accounts", "--help"],
        vec!["jackin", "usage", "jk-demo-role", "accounts", "--help"],
        vec!["jackin", "usage", "jk-demo-role", "verify", "--help"],
    ];
    for args in &subcommands {
        let help = help_text(args);
        assert!(
            help.contains("jackin❯"),
            "brand pill missing in: {}",
            args.join(" ")
        );
    }
}

#[test]
fn parses_usage_cache_accounts_json() {
    let cli =
        Cli::try_parse_from(["jackin", "usage", "cache", "accounts", "--format", "json"]).unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Usage(ref args))
            if args.instance == "cache"
                && args.format == "json"
                && matches!(args.scope, usage::UsageScope::Accounts(_))
    ));
}

#[test]
fn parses_usage_verify() {
    let cli = Cli::try_parse_from(["jackin", "usage", "jk-demo-role", "verify"]).unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Usage(ref args))
            if args.instance == "jk-demo-role"
                && matches!(args.scope, usage::UsageScope::Verify)
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

// ── prewarm clap invariant regression tests ─────────────────────────
//
// These pin the relationships the prewarm refactor dropped when the prewarm
// flags moved into the flattened `PrewarmFlags` struct. Each test maps
// to one `#[arg]` constraint restored in `prewarm.rs`. Compare against the
// pre-refactor baseline in `origin/main:crates/jackin/src/cli/prewarm.rs`.

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

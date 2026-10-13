// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_telemetry::schema::enums::CliCommandName as Name;

type TelemetryCommandCase = (&'static [&'static str], Name);

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
    let cases: &[TelemetryCommandCase] = &[
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
    ];

    assert_command_names(cases);
    assert_command_names(usage_read_command_cases());
    assert_command_names(usage_service_and_monitor_command_cases());
    assert_command_names(usage_operator_command_cases());
    assert_command_names(usage_integration_command_cases());
}

fn usage_read_command_cases() -> &'static [TelemetryCommandCase] {
    &[
        (
            &["usage", "cache", "accounts", "--format", "json"],
            Name::UsageAccounts,
        ),
        (&["usage", "jk-demo-role", "verify"], Name::UsageVerify),
        (
            &["usage", "doctor", "--provider", "claude", "--unattended"],
            Name::UsageDoctor,
        ),
        (
            &["usage", "status", "--monitor", "monitor-1"],
            Name::UsageStatus,
        ),
        (
            &["usage", "refresh", "--monitor", "monitor-1"],
            Name::UsageRefresh,
        ),
        (
            &["usage", "watch", "--monitor", "monitor-1"],
            Name::UsageWatch,
        ),
        (
            &[
                "usage",
                "wait",
                "--monitor",
                "monitor-1",
                "--until",
                "runnable",
            ],
            Name::UsageWait,
        ),
    ]
}

fn usage_service_and_monitor_command_cases() -> &'static [TelemetryCommandCase] {
    &[
        (&["usage", "service", "start"], Name::UsageServiceStart),
        (&["usage", "service", "stop"], Name::UsageServiceStop),
        (&["usage", "service", "status"], Name::UsageServiceStatus),
        (
            &[
                "usage",
                "monitor",
                "observe",
                "--provider",
                "claude",
                "--session",
                "session-1",
                "--idempotency-key",
                "observe-1",
            ],
            Name::UsageMonitorObserve,
        ),
        (
            &[
                "usage",
                "monitor",
                "start",
                "--provider",
                "claude",
                "--binding",
                "binding-1",
                "--binding-revision",
                "1",
                "--goal",
                "goal-1",
                "--policy-revision",
                "1",
                "--idempotency-key",
                "start-1",
            ],
            Name::UsageMonitorStart,
        ),
        (
            &["usage", "monitor", "stop", "--monitor", "monitor-1"],
            Name::UsageMonitorStop,
        ),
    ]
}

fn usage_operator_command_cases() -> &'static [TelemetryCommandCase] {
    &[
        (
            &[
                "usage",
                "binding",
                "confirm",
                "--provider",
                "claude",
                "--account",
                "account-1",
                "--operator-label",
                "Operator",
                "--confirm",
            ],
            Name::UsageBindingConfirm,
        ),
        (
            &[
                "usage",
                "policy",
                "approve",
                "--binding",
                "binding-1",
                "--binding-revision",
                "1",
                "--goal",
                "goal-1",
                "--policy",
                "strict-sgd",
                "--operator-label",
                "Operator",
                "--confirm",
            ],
            Name::UsagePolicyApprove,
        ),
    ]
}

fn usage_integration_command_cases() -> &'static [TelemetryCommandCase] {
    &[
        (
            &["usage", "statusline", "ingest", "--session-only"],
            Name::UsageStatuslineIngest,
        ),
        (
            &[
                "usage",
                "statusline",
                "compose",
                "--settings",
                "/tmp/settings.json",
                "--session-only",
            ],
            Name::UsageStatuslineCompose,
        ),
        (
            &[
                "usage",
                "spend",
                "record",
                "--account",
                "account-1",
                "--file",
                "/tmp/spend.json",
            ],
            Name::UsageSpendRecord,
        ),
        (
            &["usage", "auth", "prepare", "--provider", "claude"],
            Name::UsageAuthPrepare,
        ),
    ]
}

#[test]
fn telemetry_command_mapper_covers_account_leaves() {
    let cases: &[TelemetryCommandCase] = &[
        (&["account", "list"], Name::AccountList),
        (&["account", "scan"], Name::AccountScan),
        (
            &[
                "account",
                "add",
                "work",
                "--provider",
                "openai",
                "--api-key",
                "--stdin",
            ],
            Name::AccountAdd,
        ),
        (&["account", "remove", "work"], Name::AccountRemove),
        (&["account", "enable", "work"], Name::AccountEnable),
        (&["account", "disable", "work"], Name::AccountDisable),
        (
            &["account", "default", "work", "--agent", "claude"],
            Name::AccountDefault,
        ),
        (
            &["workspace", "account", "list", "sample"],
            Name::WorkspaceAccountList,
        ),
        (
            &["workspace", "account", "assign", "sample", "work"],
            Name::WorkspaceAccountAssign,
        ),
        (
            &["workspace", "account", "unassign", "sample", "work"],
            Name::WorkspaceAccountUnassign,
        ),
        (
            &[
                "workspace",
                "account",
                "select",
                "sample",
                "work",
                "--agent",
                "codex",
            ],
            Name::WorkspaceAccountSelect,
        ),
    ];
    assert_command_names(cases);
}

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
fn usage_help_describes_broker_owned_cache_and_durable_monitors() {
    let root = help_text(&["jackin", "--help"]);
    let root_words = root.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        root_words.contains("broker-owned cached usage"),
        "root help should identify the broker-owned usage cache: {root}"
    );
    assert!(
        root_words.contains("durable usage monitors"),
        "root help should mention durable usage monitors: {root}"
    );

    for help_flag in ["-h", "--help"] {
        let usage = help_text(&["jackin", "usage", help_flag]);
        let usage_words = usage.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            usage_words.contains("broker-owned cached usage"),
            "usage {help_flag} should describe its broker-owned cache: {usage}"
        );
        assert!(
            usage_words.contains("durable usage monitors"),
            "usage {help_flag} should describe durable monitors: {usage}"
        );
        for command in ["monitor", "status", "watch", "wait"] {
            assert!(
                usage.contains(&format!("\n  {command} ")),
                "usage {help_flag} should list `{command}`: {usage}"
            );
        }
    }
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
    let cli = Cli::try_parse_from(["jackin", "help", "workspace", "account"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Help { ref command }) if command == &["workspace", "account"]
    ));
}

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
        vec!["jackin", "account", "add", "--help"],
        vec!["jackin", "account", "list", "--help"],
        vec!["jackin", "account", "scan", "--help"],
        vec!["jackin", "account", "remove", "--help"],
        vec!["jackin", "account", "enable", "--help"],
        vec!["jackin", "account", "disable", "--help"],
        vec!["jackin", "account", "default", "--help"],
        vec!["jackin", "workspace", "account", "list", "--help"],
        vec!["jackin", "workspace", "account", "unassign", "--help"],
        vec!["jackin", "workspace", "account", "select", "--help"],
        vec!["jackin", "workspace", "account", "assign", "--help"],
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

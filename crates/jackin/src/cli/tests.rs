//! Tests for `cli`.
use super::*;

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
            if args.image
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
            if args.roles
                && !args.image
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
            if args.roles
                && !args.image
                && args.workspace.as_deref() == Some("jackin")
                && args.role.is_none()
                && !args.all_workspaces
    ));
}

#[test]
fn parses_prewarm_roles_all_workspaces_filter() {
    let cli = Cli::try_parse_from(["jackin", "prewarm", "--roles", "--all-workspaces"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.roles
                && !args.image
                && args.workspace.is_none()
                && args.role.is_none()
                && args.all_workspaces
    ));
}

#[test]
fn parses_prewarm_sidecar_filter() {
    let cli = Cli::try_parse_from(["jackin", "prewarm", "--sidecar"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args)) if args.sidecar && !args.image && !args.roles
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
            if args.sidecar_container && args.keep_sidecar_container
    ));
}

#[test]
fn parses_prewarm_daemon_filter() {
    let cli = Cli::try_parse_from(["jackin", "prewarm", "--daemon"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.daemon && !args.sidecar && !args.sidecar_container && !args.keep_sidecar_container
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
            if args.image
                && args.workspace.as_deref() == Some("jackin")
                && !args.all_workspaces
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
            if args.image
                && args.all_workspaces
                && args.workspace.is_none()
                && args.role.is_none()
                && args.role_git.is_none()
    ));
}

#[test]
fn parses_diagnostics_compare_labels() {
    let cli = Cli::try_parse_from([
        "jackin",
        "diagnostics",
        "compare",
        "jk-run-cold",
        "jk-run-warm",
        "--label",
        "cold-before",
        "--label",
        "warm-after",
        "--format",
        "json",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Diagnostics(DiagnosticsCommand::Compare(ref args)))
            if args.labels == ["cold-before", "warm-after"]
                && args.format == diagnostics::DiagnosticsCompareFormat::Json
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

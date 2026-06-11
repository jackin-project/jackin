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
fn root_help_shows_banner_pill() {
    let help = help_text(&["jackin", "--help"]);
    // The banner is the ` jackin' ` brand pill at the top of the help.
    assert!(
        help.trim_start().starts_with("jackin'"),
        "brand pill should lead the help: {help:?}"
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
    ];
    for args in &subcommands {
        let help = help_text(args);
        assert!(
            help.contains("jackin'"),
            "brand pill missing in: {}",
            args.join(" ")
        );
    }
}

#[test]
fn parses_prewarm_agent_filters() {
    let cli =
        Cli::try_parse_from(["jackin", "prewarm", "--agent", "claude", "--agent", "kimi"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Prewarm(ref args))
            if args.agents == [crate::agent::Agent::Claude, crate::agent::Agent::Kimi]
    ));
}

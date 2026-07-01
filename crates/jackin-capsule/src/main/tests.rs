#[cfg(test)]
use super::*;

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn parse_focus_flag_no_subcommand_finds_global_flag() {
    // Bare client mode: `jackin-capsule --focus 5` must resolve to
    // session 5 — the original use case the flag was added for.
    assert_eq!(
        parse_focus_flag(&args(&["jackin-capsule", "--focus", "5"])),
        Some(5)
    );
    assert_eq!(
        parse_focus_flag(&args(&["jackin-capsule", "--focus=7"])),
        Some(7)
    );
}

#[test]
fn parse_focus_flag_new_with_agent_finds_trailing_focus() {
    // `new <agent> --focus N` is a legitimate combination — spawn
    // the agent AND switch focus to N once the daemon answers.
    assert_eq!(
        parse_focus_flag(&args(&["jackin-capsule", "new", "claude", "--focus", "9"])),
        Some(9)
    );
}

#[test]
fn parse_focus_flag_new_without_agent_ignores_focus() {
    // `new --focus 5` is the typo this regression guards against.
    // Without scoping, --focus at index 2 (where the agent slug
    // would belong) would silently route the operator to session 5
    // AND spawn a default Shell because validate_agent_slug rejects
    // "--focus" as an agent. After the scope fix, --focus at index
    // 2 is treated as a malformed agent argument; focus stays None.
    assert_eq!(
        parse_focus_flag(&args(&["jackin-capsule", "new", "--focus", "5"])),
        None
    );
    assert_eq!(
        parse_focus_flag(&args(&["jackin-capsule", "new", "--focus=5"])),
        None
    );
}

#[test]
fn parse_focus_flag_other_subcommands_ignore_focus_positional() {
    // status/snapshot/runtime-setup take no arguments at all; any
    // --focus after them is residual.
    assert_eq!(
        parse_focus_flag(&args(&["jackin-capsule", "status", "--focus", "5"])),
        None
    );
    assert_eq!(
        parse_focus_flag(&args(&[
            "jackin-capsule",
            "usage",
            "session",
            "5",
            "--focus",
            "7",
        ])),
        None
    );
}

#[test]
fn parse_provider_flag_extracts_label_after_agent() {
    assert_eq!(
        parse_provider_flag(&args(&[
            "jackin-capsule",
            "new",
            "claude",
            "--provider=Z.AI"
        ])),
        Some("Z.AI".to_owned())
    );
}

#[test]
fn parse_provider_flag_absent_or_no_agent_is_none() {
    assert_eq!(
        parse_provider_flag(&args(&["jackin-capsule", "new", "claude"])),
        None
    );
    // No agent positional → nothing at index 3+ to scan.
    assert_eq!(parse_provider_flag(&args(&["jackin-capsule", "new"])), None);
}

#[test]
fn parse_provider_flag_empty_value_is_empty_label() {
    // The daemon treats an empty label as an unknown provider (no redirect).
    assert_eq!(
        parse_provider_flag(&args(&["jackin-capsule", "new", "claude", "--provider="])),
        Some(String::new())
    );
}

#[test]
fn force_daemon_only_captures_the_agent_slug_entrypoint() {
    // The entrypoint is invoked with the initial agent slug → daemon mode.
    assert!(is_daemon_entrypoint_args(&args(&[
        "jackin-capsule",
        "claude"
    ])));
    // Every client form must stay client even though `container exec`
    // children inherit JACKIN_CAPSULE_FORCE_DAEMON in the apple-container VM.
    assert!(!is_daemon_entrypoint_args(&args(&["jackin-capsule"]))); // bare attach
    assert!(!is_daemon_entrypoint_args(&args(&[
        "jackin-capsule",
        "--focus",
        "5"
    ])));
    for sub in [
        "status",
        "snapshot",
        "agents",
        "usage",
        "mcp-server",
        "runtime-setup",
        "new",
    ] {
        assert!(
            !is_daemon_entrypoint_args(&args(&["jackin-capsule", sub])),
            "{sub} must stay client under FORCE_DAEMON"
        );
    }
}

#[test]
fn hook_invocation_detects_symlink_name() {
    assert!(invoked_as_prepare_commit_msg_hook(&args(&[
        "/jackin/state/git-hooks/prepare-commit-msg",
        ".git/COMMIT_EDITMSG",
    ])));
    assert!(!invoked_as_prepare_commit_msg_hook(&args(&[
        "/jackin/runtime/jackin-capsule",
        "prepare-commit-msg",
        ".git/COMMIT_EDITMSG",
    ])));
}
}

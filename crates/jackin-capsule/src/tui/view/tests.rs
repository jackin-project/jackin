//! Tests for `view`.
use super::{
    pane_limit_failure_message, spawn_failure_agent_label, spawn_failure_banner,
    spawn_failure_message, spawn_request_failure_message, tab_limit_failure_message,
};

#[test]
fn spawn_failure_message_prefixes_visible_agent_label() {
    assert_eq!(
        spawn_failure_message("claude", "missing binary"),
        "claude: missing binary"
    );
    assert_eq!(spawn_failure_agent_label(Some("claude")), "claude");
    assert_eq!(spawn_failure_agent_label(None), "shell");
    assert_eq!(
        spawn_request_failure_message("codex", "missing binary"),
        "spawn codex failed: missing binary"
    );
}

#[test]
fn spawn_failure_banner_writes_top_row_and_restores_cursor() {
    let banner = String::from_utf8(spawn_failure_banner("shell: cap hit")).unwrap();
    assert!(banner.starts_with("\x1b7\x1b[1;1H"));
    assert!(banner.contains("jackin: shell: cap hit"));
    assert!(banner.ends_with("\x1b8"));
}

#[test]
fn spawn_capacity_messages_report_visible_limits() {
    assert_eq!(
        tab_limit_failure_message(32),
        "tab limit reached (32); close one before spawning another"
    );
    assert_eq!(
        pane_limit_failure_message(64),
        "pane limit reached (64); close some panes before opening more"
    );
}

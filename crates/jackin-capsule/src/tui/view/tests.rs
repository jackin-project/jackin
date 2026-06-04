//! Tests for `view`.
use super::{
    CapsuleBottomChrome, pane_limit_failure_message, render_capsule_bottom_chrome,
    spawn_failure_agent_label, spawn_failure_banner, spawn_failure_message,
    spawn_request_failure_message, tab_limit_failure_message,
};
use crate::tui::app::HoverTarget;

fn debug_chrome(hover: Option<HoverTarget>) -> Vec<u8> {
    let mut buf = Vec::new();
    render_capsule_bottom_chrome(
        &mut buf,
        CapsuleBottomChrome {
            term_rows: 24,
            term_cols: 80,
            branch: Some("main"),
            pull_request: None,
            pull_request_loading: false,
            instance_id_label: "jk-test",
            hover_target: hover,
            scrollback_active: false,
            debug_run_id: Some("jk-run-test"),
        },
    );
    buf
}

#[test]
fn debug_run_id_chip_renders_danger_red_without_panicking() {
    // Regression: the chip routed DANGER_RED through the panicking `const`
    // `rgb_bg`/`rgb_fg` allowlist, crashing the capsule on the first frame
    // under `--debug`. It must now emit the truecolor SGR for DANGER_RED
    // (255,94,122) on both the idle and hovered paths.
    let idle = debug_chrome(None);
    assert!(
        idle.windows(b"48;2;255;94;122m".len())
            .any(|w| w == b"48;2;255;94;122m"),
        "idle debug chip must paint DANGER_RED background"
    );
    let hovered = debug_chrome(Some(HoverTarget::DebugChip));
    assert!(
        hovered
            .windows(b"38;2;255;94;122m".len())
            .any(|w| w == b"38;2;255;94;122m"),
        "hovered debug chip must paint DANGER_RED foreground"
    );
}

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

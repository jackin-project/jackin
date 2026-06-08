//! Tests for `view`.
use super::{
    CapsuleBottomChrome, CapsuleDialogBottomChrome, CapsuleRatatuiFrame,
    pane_limit_failure_message, render_capsule_bottom_chrome, render_capsule_dialog_bottom_chrome,
    render_capsule_ratatui_frame, spawn_failure_agent_label, spawn_failure_banner,
    spawn_failure_message, spawn_request_failure_message, tab_limit_failure_message,
};
use crate::tui::app::HoverTarget;
use crate::tui::components::dialog_widgets::DialogRatatuiSnapshot;
use crate::tui::components::status_bar::{PrefixMode, STATUS_BAR_ROWS};
use crate::tui::layout::Tab;
use crate::tui::layout::available_content_rows;
use ratatui::{Terminal, backend::TestBackend};

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
            scroll_axes: jackin_tui::components::ScrollAxes::default(),
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
fn dialog_bottom_chrome_blank_background_suppresses_context_bar() {
    let hints = [
        jackin_tui::HintSpan::Key("Esc"),
        jackin_tui::HintSpan::Text("dismiss"),
    ];
    let mut buf = Vec::new();

    render_capsule_dialog_bottom_chrome(
        &mut buf,
        CapsuleDialogBottomChrome {
            term_rows: 24,
            term_cols: 100,
            branch: Some("feature/context"),
            pull_request: None,
            pull_request_loading: false,
            instance_id_label: "jk-test",
            hint_spans: Some(&hints),
            blank_background: true,
        },
    );

    let rendered = String::from_utf8(buf).unwrap();
    assert!(rendered.contains("Esc"));
    assert!(rendered.contains("dismiss"));
    assert!(!rendered.contains("feature/context"));
    assert!(!rendered.contains("jk-test"));
}

#[test]
fn dialog_bottom_chrome_nonblank_background_keeps_context_bar() {
    let mut buf = Vec::new();

    render_capsule_dialog_bottom_chrome(
        &mut buf,
        CapsuleDialogBottomChrome {
            term_rows: 24,
            term_cols: 100,
            branch: Some("feature/context"),
            pull_request: None,
            pull_request_loading: false,
            instance_id_label: "jk-test",
            hint_spans: None,
            blank_background: false,
        },
    );

    let rendered = String::from_utf8(buf).unwrap();
    assert!(rendered.contains("feature/context"));
    assert!(rendered.contains("jk-test"));
}

#[test]
fn debug_dialog_keeps_status_bar_visible() {
    let tabs = [Tab::new_single("Codex", 1, "codex")];
    let state = jackin_tui::components::DebugInfo {
        jackin_version: Some("0.6.0-dev".to_owned()),
        capsule_version: Some("0.6.0-dev".to_owned()),
        container_id: Some("jk-test-thearchitect".to_owned()),
        role: Some("the-architect".to_owned()),
        agent: Some("Codex".to_owned()),
        target: None,
        run_id: Some("jk-run-test".to_owned()),
        diagnostics_log_path: Some(
            "/home/agent/.jackin/data/diagnostics/runs/jk-run-test.jsonl".to_owned(),
        ),
    }
    .into_state();
    let snapshot = (DialogRatatuiSnapshot::DebugInfo(state), (3, 8, 10, 64));
    let backend = TestBackend::new(90, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render_capsule_ratatui_frame(
                frame,
                CapsuleRatatuiFrame {
                    tabs: &tabs,
                    active_tab: 0,
                    term_cols: 90,
                    term_rows: 24,
                    panes: &[],
                    pane_titles: &[],
                    focus_owner: jackin_tui::components::FocusOwner::Content(1),
                    zoomed: false,
                    dialog_open: true,
                    dialog_snapshot: Some(&snapshot),
                    pane_screens: &[],
                    sessions_state: &[],
                    prefix_mode: PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                    selection: None,
                    selection_copied: false,
                    scrollbars: &[],
                },
            );
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let row0: String = (0..30).map(|x| buf[(x, 0)].symbol().to_owned()).collect();
    assert!(row0.contains("jackin'"), "status brand missing: {row0:?}");
    assert!(row0.contains("Codex"), "status tab missing: {row0:?}");
    let row1: String = (0..30).map(|x| buf[(x, 1)].symbol().to_owned()).collect();
    assert!(
        row1.contains("━"),
        "status underline row must remain above dialog backdrop: {row1:?}"
    );
    let dialog_title: String = (8..28).map(|x| buf[(x, 3)].symbol().to_owned()).collect();
    assert!(
        dialog_title.contains("Debug info"),
        "debug dialog missing: {dialog_title:?}"
    );
}

#[test]
fn selection_copy_toast_keeps_status_and_bottom_chrome_rows_free() {
    let tabs = [Tab::new_single("Codex", 1, "codex")];
    let backend = TestBackend::new(90, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render_capsule_ratatui_frame(
                frame,
                CapsuleRatatuiFrame {
                    tabs: &tabs,
                    active_tab: 0,
                    term_cols: 90,
                    term_rows: 24,
                    panes: &[],
                    pane_titles: &[],
                    focus_owner: jackin_tui::components::FocusOwner::Content(1),
                    zoomed: false,
                    dialog_open: false,
                    dialog_snapshot: None,
                    pane_screens: &[],
                    sessions_state: &[],
                    prefix_mode: PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                    selection: None,
                    selection_copied: true,
                    scrollbars: &[],
                },
            );
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let row = |y: u16| -> String { (0..90).map(|x| buf[(x, y)].symbol().to_owned()).collect() };
    let all_rows: Vec<String> = (0..24).map(row).collect();
    assert!(
        all_rows.iter().any(|row| row.contains("Selection copied")),
        "selection copy toast should be visible: {all_rows:?}"
    );
    assert!(
        !all_rows[..usize::from(STATUS_BAR_ROWS)]
            .iter()
            .any(|row| row.contains("Selection copied")),
        "selection copy toast must not draw over status rows: {all_rows:?}"
    );
    let content_bottom = STATUS_BAR_ROWS + available_content_rows(24);
    assert!(
        !all_rows[usize::from(content_bottom)..]
            .iter()
            .any(|row| row.contains("Selection copied")),
        "selection copy toast must not draw over hint/spacer/footer rows: {all_rows:?}"
    );
    assert!(
        all_rows[0].contains("jackin'"),
        "status brand missing: {:?}",
        all_rows[0]
    );
    assert!(
        all_rows[1].contains("━"),
        "status underline row must remain clear of the toast: {:?}",
        all_rows[1]
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

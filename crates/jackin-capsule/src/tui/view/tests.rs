//! Tests for `view`.
use super::{
    CapsuleRatatuiFrame, pane_limit_failure_message, render_capsule_ratatui_frame,
    spawn_failure_agent_label, spawn_failure_message, spawn_request_failure_message,
    tab_limit_failure_message,
};
use crate::tui::app::HoverTarget;
use crate::tui::components::dialog_widgets::DialogRatatuiSnapshot;
use crate::tui::components::status_bar::{PrefixMode, STATUS_BAR_ROWS};
use crate::tui::layout::Tab;
use crate::tui::layout::available_content_rows;
use ratatui::{Terminal, backend::TestBackend};

/// Render one main-view frame at 24x80 with the given chrome inputs and
/// return the terminal buffer for row-level assertions.
fn chrome_frame(
    hover: Option<HoverTarget>,
    debug_run_id: Option<&str>,
    spawn_failure: Option<&str>,
    clipboard_image_notice: Option<&str>,
    link_hover_notice: Option<&str>,
) -> ratatui::buffer::Buffer {
    let tabs = [Tab::new_single("Codex", 1, "codex")];
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let status_plan =
        crate::tui::components::status_bar::status_bar_plan(80, &tabs, 0, &[], PrefixMode::Idle);
    terminal
        .draw(|frame| {
            render_capsule_ratatui_frame(
                frame,
                CapsuleRatatuiFrame {
                    tabs: &tabs,
                    status_plan: &status_plan,
                    term_cols: 80,
                    term_rows: 24,
                    panes: &[],
                    pane_titles: &[],
                    focus_owner: jackin_tui::components::FocusOwner::Content(1),
                    zoomed: false,
                    dialog_open: false,
                    dialog_snapshot: None,
                    pane_screens: &[],
                    prefix_mode: PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                    selection: None,
                    selection_copied: false,
                    scrollbars: &[],
                    branch: Some("main"),
                    usage_status_label: None,
                    pull_request: None,
                    pull_request_loading: false,
                    instance_id_label: "jk-test",
                    hover_target: hover,
                    scrollback_active: false,
                    main_scroll_axes: jackin_tui::components::ScrollAxes::default(),
                    debug_run_id,
                    dialog_hint_spans: None,
                    spawn_failure,
                    palette_key: 0x1C,
                    clipboard_image_notice,
                    link_hover_notice,
                },
            );
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

/// Column of the run-id chip's first glyph; symbol-indexed so the `·`
/// separators on the bar cannot skew a byte-based search.
fn chip_start_col(row: &str) -> u16 {
    let byte = row.find("jk-run-test").expect("chip start");
    row[..byte].chars().count() as u16
}

fn row_text(buf: &ratatui::buffer::Buffer, y: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol().to_owned())
        .collect()
}

#[test]
fn bottom_chrome_widget_paints_branch_bar_and_hint_row() {
    let buf = chrome_frame(None, None, None, None, None);
    let bar = row_text(&buf, 23);
    assert!(bar.contains("Branch · main"), "branch bar missing: {bar:?}");
    assert!(bar.contains("jk-test"), "container chunk missing: {bar:?}");
    let hint = row_text(&buf, 24 - 3);
    assert!(
        hint.contains("focus pane"),
        "main hint row missing: {hint:?}"
    );
}

#[test]
fn debug_run_id_chip_renders_danger_red_on_the_bar_row() {
    let buf = chrome_frame(None, Some("jk-run-test"), None, None, None);
    let bar = row_text(&buf, 23);
    assert!(bar.contains("jk-run-test"), "chip missing: {bar:?}");
    let chip_x = chip_start_col(&bar);
    assert_eq!(
        buf[(chip_x, 23)].bg,
        ratatui::style::Color::Rgb(255, 94, 122),
        "idle debug chip must paint DANGER_RED background"
    );
    let hovered = chrome_frame(
        Some(HoverTarget::DebugChip),
        Some("jk-run-test"),
        None,
        None,
        None,
    );
    let chip_x = chip_start_col(&row_text(&hovered, 23));
    assert_eq!(
        hovered[(chip_x, 23)].fg,
        ratatui::style::Color::Rgb(255, 94, 122),
        "hovered debug chip must paint DANGER_RED foreground"
    );
}

#[test]
fn spawn_failure_banner_widget_paints_top_row_notice() {
    let buf = chrome_frame(None, None, Some("shell: cap hit"), None, None);
    let row0 = row_text(&buf, 0);
    assert!(
        row0.contains("jackin: shell: cap hit"),
        "banner missing: {row0:?}"
    );
}

#[test]
fn clipboard_image_notice_keeps_status_and_bottom_chrome_rows_free() {
    let buf = chrome_frame(
        None,
        None,
        None,
        Some("Image staged: /jackin/run/clipboard/clipboard-test.png"),
        None,
    );
    let row = |y: u16| -> String { (0..80).map(|x| buf[(x, y)].symbol().to_owned()).collect() };
    let all_rows: Vec<String> = (0..24).map(row).collect();
    assert!(
        all_rows.iter().any(|row| row.contains("Image staged:")),
        "clipboard image notice should be visible: {all_rows:?}"
    );
    assert!(
        !all_rows[..usize::from(STATUS_BAR_ROWS)]
            .iter()
            .any(|row| row.contains("Image staged:")),
        "clipboard image notice must not draw over status rows: {all_rows:?}"
    );
    let content_bottom = STATUS_BAR_ROWS + available_content_rows(24);
    assert!(
        !all_rows[usize::from(content_bottom)..]
            .iter()
            .any(|row| row.contains("Image staged:")),
        "clipboard image notice must not draw over hint/spacer/footer rows: {all_rows:?}"
    );
}

#[test]
fn non_debug_dialog_hides_bottom_status_bar() {
    let tabs = [Tab::new_single("Codex", 1, "codex")];
    let state = jackin_tui::components::DebugInfo {
        jackin_version: Some("0.6.0-dev".to_owned()),
        capsule_version: Some("0.6.0-dev".to_owned()),
        container_id: Some("jk-test-thearchitect".to_owned()),
        role: Some("the-architect".to_owned()),
        agent: Some("Codex".to_owned()),
        target: None,
        run_id: None,
        diagnostics_log_path: None,
    }
    .into_state();
    let snapshot = (DialogRatatuiSnapshot::DebugInfo(state), (3, 8, 10, 64));
    let hints = [
        jackin_tui::HintSpan::Key("Esc"),
        jackin_tui::HintSpan::Text("dismiss"),
    ];
    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let status_plan =
        crate::tui::components::status_bar::status_bar_plan(120, &tabs, 0, &[], PrefixMode::Idle);

    terminal
        .draw(|frame| {
            render_capsule_ratatui_frame(
                frame,
                CapsuleRatatuiFrame {
                    tabs: &tabs,
                    status_plan: &status_plan,
                    term_cols: 120,
                    term_rows: 24,
                    panes: &[],
                    pane_titles: &[],
                    focus_owner: jackin_tui::components::FocusOwner::Content(1),
                    zoomed: false,
                    dialog_open: true,
                    dialog_snapshot: Some(&snapshot),
                    pane_screens: &[],
                    prefix_mode: PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                    selection: None,
                    selection_copied: false,
                    scrollbars: &[],
                    branch: Some("feature/status"),
                    usage_status_label: Some("Session 99%"),
                    pull_request: None,
                    pull_request_loading: false,
                    instance_id_label: "jk-test",
                    hover_target: None,
                    scrollback_active: false,
                    main_scroll_axes: jackin_tui::components::ScrollAxes::default(),
                    debug_run_id: None,
                    dialog_hint_spans: Some(&hints),
                    spawn_failure: None,
                    palette_key: 0x1C,
                    clipboard_image_notice: None,
                    link_hover_notice: None,
                },
            );
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let row0: String = (0..30).map(|x| buf[(x, 0)].symbol().to_owned()).collect();
    assert!(row0.contains("jackin❯"), "status brand missing: {row0:?}");
    let hint = row_text(buf, 21);
    assert!(hint.contains("dismiss"), "dialog hint missing: {hint:?}");
    let footer = row_text(buf, 23);
    assert!(
        !footer.contains("Session 99%")
            && !footer.contains("jk-test")
            && !footer.contains("feature/status"),
        "non-debug dialog must not render the bottom status bar: {footer:?}"
    );
}

#[test]
fn link_hover_notice_keeps_status_and_bottom_chrome_rows_free() {
    let buf = chrome_frame(
        None,
        None,
        None,
        None,
        Some("Open link: https://example.com/visible"),
    );
    let row = |y: u16| -> String { (0..80).map(|x| buf[(x, y)].symbol().to_owned()).collect() };
    let all_rows: Vec<String> = (0..24).map(row).collect();
    assert!(
        all_rows
            .iter()
            .any(|row| row.contains("Open link: https://example.com/visible")),
        "link hover notice should be visible: {all_rows:?}"
    );
    assert!(
        !all_rows[..usize::from(STATUS_BAR_ROWS)]
            .iter()
            .any(|row| row.contains("Open link:")),
        "link hover notice must not draw over status rows: {all_rows:?}"
    );
    let content_bottom = STATUS_BAR_ROWS + available_content_rows(24);
    assert!(
        !all_rows[usize::from(content_bottom)..]
            .iter()
            .any(|row| row.contains("Open link:")),
        "link hover notice must not draw over hint/spacer/footer rows: {all_rows:?}"
    );
}

#[test]
fn clipboard_image_notice_takes_priority_over_link_hover_notice() {
    let buf = chrome_frame(
        None,
        None,
        None,
        Some("Image staged: /jackin/run/clipboard/clipboard-test.png"),
        Some("Open link: https://example.com/visible"),
    );
    let all_rows: Vec<String> = (0..24)
        .map(|y| (0..80).map(|x| buf[(x, y)].symbol().to_owned()).collect())
        .collect();
    assert!(
        all_rows.iter().any(|row| row.contains("Image staged:")),
        "clipboard image notice should be visible: {all_rows:?}"
    );
    assert!(
        !all_rows.iter().any(|row| row.contains("Open link:")),
        "clipboard image notice should suppress link hover notice: {all_rows:?}"
    );
}

#[test]
fn clipboard_image_notice_takes_priority_over_selection_copy_toast() {
    let tabs = [Tab::new_single("Codex", 1, "codex")];
    let backend = TestBackend::new(90, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let status_plan =
        crate::tui::components::status_bar::status_bar_plan(90, &tabs, 0, &[], PrefixMode::Idle);

    terminal
        .draw(|frame| {
            render_capsule_ratatui_frame(
                frame,
                CapsuleRatatuiFrame {
                    tabs: &tabs,
                    status_plan: &status_plan,
                    term_cols: 90,
                    term_rows: 24,
                    panes: &[],
                    pane_titles: &[],
                    focus_owner: jackin_tui::components::FocusOwner::Content(1),
                    zoomed: false,
                    dialog_open: false,
                    dialog_snapshot: None,
                    pane_screens: &[],
                    prefix_mode: PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                    selection: None,
                    selection_copied: true,
                    scrollbars: &[],
                    branch: None,
                    usage_status_label: None,
                    pull_request: None,
                    pull_request_loading: false,
                    instance_id_label: "jk-test",
                    hover_target: None,
                    scrollback_active: false,
                    main_scroll_axes: jackin_tui::components::ScrollAxes::default(),
                    debug_run_id: None,
                    dialog_hint_spans: None,
                    spawn_failure: None,
                    palette_key: 0x1C,
                    clipboard_image_notice: Some("Image staged: /jackin/run/clipboard/test.png"),
                    link_hover_notice: None,
                },
            );
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let all_rows: Vec<String> = (0..24)
        .map(|y| (0..90).map(|x| buf[(x, y)].symbol().to_owned()).collect())
        .collect();
    assert!(
        all_rows.iter().any(|row| row.contains("Image staged:")),
        "clipboard image notice should be visible: {all_rows:?}"
    );
    assert!(
        !all_rows.iter().any(|row| row.contains("Selection copied")),
        "clipboard image notice should replace selection copy toast: {all_rows:?}"
    );
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
    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let status_plan =
        crate::tui::components::status_bar::status_bar_plan(120, &tabs, 0, &[], PrefixMode::Idle);

    terminal
        .draw(|frame| {
            render_capsule_ratatui_frame(
                frame,
                CapsuleRatatuiFrame {
                    tabs: &tabs,
                    status_plan: &status_plan,
                    term_cols: 120,
                    term_rows: 24,
                    panes: &[],
                    pane_titles: &[],
                    focus_owner: jackin_tui::components::FocusOwner::Content(1),
                    zoomed: false,
                    dialog_open: true,
                    dialog_snapshot: Some(&snapshot),
                    pane_screens: &[],
                    prefix_mode: PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                    selection: None,
                    selection_copied: false,
                    scrollbars: &[],
                    branch: None,
                    usage_status_label: Some("Session 99%"),
                    pull_request: None,
                    pull_request_loading: false,
                    instance_id_label: "jk-test",
                    hover_target: None,
                    scrollback_active: false,
                    main_scroll_axes: jackin_tui::components::ScrollAxes::default(),
                    debug_run_id: Some("jk-run-test"),
                    dialog_hint_spans: None,
                    spawn_failure: None,
                    palette_key: 0x1C,
                    clipboard_image_notice: None,
                    link_hover_notice: None,
                },
            );
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let row0: String = (0..30).map(|x| buf[(x, 0)].symbol().to_owned()).collect();
    assert!(row0.contains("jackin❯"), "status brand missing: {row0:?}");
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
    let footer = row_text(buf, 23);
    let usage_col = footer
        .find("Session 99%")
        .unwrap_or_else(|| panic!("usage status missing from footer: {footer:?}"));
    let container_col = footer
        .find("jk-test")
        .unwrap_or_else(|| panic!("container id missing from footer: {footer:?}"));
    let run_id_col = footer
        .find("jk-run-test")
        .unwrap_or_else(|| panic!("run id missing from footer: {footer:?}"));
    assert!(
        usage_col < container_col && container_col < run_id_col,
        "footer right group must be usage, container, run ID: {footer:?}"
    );
}

#[test]
fn selection_copy_toast_keeps_status_and_bottom_chrome_rows_free() {
    let tabs = [Tab::new_single("Codex", 1, "codex")];
    let backend = TestBackend::new(90, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let status_plan =
        crate::tui::components::status_bar::status_bar_plan(90, &tabs, 0, &[], PrefixMode::Idle);

    terminal
        .draw(|frame| {
            render_capsule_ratatui_frame(
                frame,
                CapsuleRatatuiFrame {
                    tabs: &tabs,
                    status_plan: &status_plan,
                    term_cols: 90,
                    term_rows: 24,
                    panes: &[],
                    pane_titles: &[],
                    focus_owner: jackin_tui::components::FocusOwner::Content(1),
                    zoomed: false,
                    dialog_open: false,
                    dialog_snapshot: None,
                    pane_screens: &[],
                    prefix_mode: PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                    selection: None,
                    selection_copied: true,
                    scrollbars: &[],
                    branch: None,
                    usage_status_label: None,
                    pull_request: None,
                    pull_request_loading: false,
                    instance_id_label: "jk-test",
                    hover_target: None,
                    scrollback_active: false,
                    main_scroll_axes: jackin_tui::components::ScrollAxes::default(),
                    debug_run_id: None,
                    dialog_hint_spans: None,
                    spawn_failure: None,
                    palette_key: 0x1C,
                    clipboard_image_notice: None,
                    link_hover_notice: None,
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
        all_rows[0].contains("jackin❯"),
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

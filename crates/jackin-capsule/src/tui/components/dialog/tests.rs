//! Tests for `jackin-capsule` dialog components.
#![allow(clippy::too_many_lines)]
use super::*;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

fn picker(agents: Vec<&str>) -> Dialog {
    // Mirror the daemon's construction site: `Dialog::new_agent_picker`
    // computes the initial `selected` past the leading `"agents"`
    // section row. Tests that explicitly want a different starting
    // selection construct `Dialog::AgentPicker { … }` inline.
    Dialog::new_agent_picker(
        agents.into_iter().map(String::from).collect(),
        PickerIntent::NewTab,
    )
}

fn palette_with(selected: usize, filter: impl Into<String>) -> Dialog {
    Dialog::CommandPalette {
        selected,
        filter: filter.into(),
        close_label: PaletteCloseLabel::ChooseTarget,
    }
}

fn palette() -> Dialog {
    palette_with(0, String::new())
}

#[test]
fn esc_dismisses_palette() {
    let mut d = palette();
    assert_eq!(d.handle_key(b"\x1b", None), DialogAction::Dismiss);
}

#[test]
fn ctrl_c_dismisses_palette() {
    let mut d = palette();
    assert_eq!(d.handle_key(b"\x03", None), DialogAction::Dismiss);
}

#[test]
fn arrow_down_advances_palette_selection() {
    let mut d = palette();
    assert_eq!(d.handle_key(b"\x1b[B", None), DialogAction::Redraw);
    let Dialog::CommandPalette { selected, .. } = d else {
        unreachable!()
    };
    assert_eq!(selected, 1);
}

#[test]
fn arrow_down_clamps_palette_at_last_item() {
    let mut d = palette_with(PALETTE_ITEMS.len() - 1, String::new());
    d.handle_key(b"\x1b[B", None);
    let Dialog::CommandPalette { selected, .. } = d else {
        unreachable!()
    };
    assert_eq!(selected, PALETTE_ITEMS.len() - 1);
}

#[test]
fn enter_on_palette_emits_command() {
    let mut d = palette();
    match d.handle_key(b"\r", None) {
        DialogAction::Command(cmd) => assert_eq!(cmd, PALETTE_ITEMS[0].0),
        other => panic!("expected Command, got {other:?}"),
    }
}

#[test]
fn enter_on_agent_picker_emits_spawn() {
    let mut d = picker(vec!["claude", "codex"]);
    match d.handle_key(b"\r", None) {
        DialogAction::SpawnAgent { agent, intent } => {
            assert_eq!(agent.as_deref(), Some("claude"));
            assert_eq!(intent, PickerIntent::NewTab);
        }
        other => panic!("expected SpawnAgent, got {other:?}"),
    }
}

#[test]
fn agent_picker_shell_slot_emits_none_agent() {
    // Layout for `picker(vec!["claude"])` is:
    //   0: Section("agents")    — non-selectable
    //   1: Agent(claude)        ← initial selected (skipped past Section)
    //   2: Section("shells")    — non-selectable
    //   3: Shell                ← Enter emits agent=None
    // Arrow Down from index 1 must skip the Section at index 2 and
    // land directly on the Shell row at index 3.
    let mut d = picker(vec!["claude"]);
    d.handle_key(b"\x1b[B", None);
    match d.handle_key(b"\r", None) {
        DialogAction::SpawnAgent { agent, .. } => assert!(agent.is_none()),
        other => panic!("expected SpawnAgent, got {other:?}"),
    }
}

#[test]
fn picker_arrow_down_skips_section_label() {
    // Direct check: from the last-agent index, Down lands on the
    // first selectable past the "shells" section header, not on
    // the header itself.
    let mut d = picker(vec!["claude", "codex"]);
    // Walk past both agents (selected 1 → 2 → expected 4 = Shell).
    d.handle_key(b"\x1b[B", None); // 1 → 2
    d.handle_key(b"\x1b[B", None); // 2 → 4 (skips Section at 3)
    let Dialog::AgentPicker { selected, .. } = &d else {
        unreachable!()
    };
    assert_eq!(*selected, 4, "Down must skip the shells section label");
}

#[test]
fn picker_enter_on_section_label_is_noop() {
    // Defensive: an out-of-band selected value pointing at a
    // Section row must not synthesise a SpawnAgent. Real flows
    // can't get there (arrows step past sections, click on a
    // section returns Consume), but a stale `selected` after a
    // filter pass that left only sections behind must degrade
    // to Redraw.
    let mut d = Dialog::AgentPicker {
        agents: vec!["claude".to_owned()],
        selected: 0, // points at Section("agents")
        intent: PickerIntent::NewTab,
        filter: String::new(),
    };
    assert_eq!(d.handle_key(b"\r", None), DialogAction::Redraw);
}

#[test]
fn click_outside_dialog_dismisses() {
    let mut d = palette();
    // Click in the top-left corner is reliably outside the centred
    // box even on tiny terminals.
    assert_eq!(d.handle_click(0, 0, 40, 100, None), DialogAction::Dismiss);
}

#[test]
fn clickable_at_reports_container_info_copy_target() {
    let d = container_info_fixture();
    let (row, col, _, _) = d.box_rect(40, 100);
    // Click the value column (the cyan link), not the label: the shared
    // component's hit-zone is the value text. Value starts past the widest
    // label ("jackin-capsule").
    assert!(d.clickable_at(row + 2, col + 22, 40, 100, None));
    assert!(!d.clickable_at(row + 3, col + 22, 40, 100, None));
    assert!(!d.clickable_at(0, 0, 40, 100, None));
}

#[test]
fn clickable_at_skips_agent_picker_section_labels() {
    let d = picker(vec!["claude"]);
    let (row, col, _, _) = d.box_rect(40, 100);
    let first_item_row = row + 3;
    assert!(
        !d.clickable_at(first_item_row, col + 2, 40, 100, None),
        "section label must not advertise as clickable"
    );
    assert!(
        d.clickable_at(first_item_row + 1, col + 2, 40, 100, None),
        "agent row should advertise as clickable"
    );
}

#[test]
fn palette_typing_filters_items_and_resets_selection() {
    let mut d = palette_with(3, String::new());
    // Type "split" — narrows to the single "Split pane" item +
    // resets selection to 0. The directional choice lives in the
    // SplitDirectionPicker sub-dialog opened on confirm.
    for &c in b"split" {
        d.handle_key(&[c], None);
    }
    let Dialog::CommandPalette {
        selected, filter, ..
    } = &d
    else {
        unreachable!()
    };
    assert_eq!(filter, "split");
    assert_eq!(*selected, 0, "filter input must reset selection to 0");
    assert_eq!(
        palette_filtered_indices(filter, PaletteCloseLabel::ChooseTarget).len(),
        1,
        "exactly one PALETTE_ITEM matches 'split' after the collapse"
    );
}

#[test]
fn palette_split_opens_split_direction_picker_via_dialog_action() {
    // Confirming "Split pane" in the menu produces
    // `DialogAction::Command(PaletteCommand::Split)` — the daemon
    // turns that into a new SplitDirectionPicker dialog. Lock the
    // action shape so a refactor that flips the chain inadvertently
    // (e.g. directly emitting SplitDirection) gets caught.
    let mut d = palette();
    for &c in b"split" {
        d.handle_key(&[c], None);
    }
    match d.handle_key(b"\r", None) {
        DialogAction::Command(cmd) => assert_eq!(cmd, PaletteCommand::Split),
        other => panic!("expected Command(Split), got {other:?}"),
    }
}

#[test]
fn split_direction_picker_enter_emits_split_direction() {
    let mut d = Dialog::SplitDirectionPicker {
        selected: 0,
        filter: String::new(),
    };
    // selected = 0 → first item = Right
    match d.handle_key(b"\r", None) {
        DialogAction::SplitDirection(dir) => assert_eq!(dir, SplitDirection::Right),
        other => panic!("expected SplitDirection(Right), got {other:?}"),
    }
}

#[test]
fn split_direction_picker_orders_default_directions_and_arrow_prefixes() {
    assert_eq!(
        SPLIT_DIRECTION_ITEMS
            .iter()
            .map(|direction| direction.label())
            .collect::<Vec<_>>(),
        vec!["→ Right", "← Left", "↓ Below", "↑ Above"]
    );
}

#[test]
fn split_direction_picker_typing_belo_narrows_to_below() {
    let mut d = Dialog::SplitDirectionPicker {
        selected: 0,
        filter: String::new(),
    };
    for &c in b"belo" {
        d.handle_key(&[c], None);
    }
    match d.handle_key(b"\r", None) {
        DialogAction::SplitDirection(dir) => assert_eq!(dir, SplitDirection::Below),
        other => panic!("expected SplitDirection(Below), got {other:?}"),
    }
}

#[test]
fn palette_enter_after_filter_emits_matching_command() {
    let mut d = palette();
    for &c in b"close" {
        d.handle_key(&[c], None);
    }
    // "close" matches the top-level Close command; the daemon
    // decides whether to confirm directly or open the target
    // picker based on the active tab's pane count.
    match d.handle_key(b"\r", None) {
        DialogAction::Command(cmd) => assert_eq!(cmd, PaletteCommand::Close),
        other => panic!("expected Close, got {other:?}"),
    }
}

#[test]
fn palette_close_label_derives_from_pane_count() {
    assert_eq!(
        PaletteCloseLabel::for_pane_count(1),
        PaletteCloseLabel::CloseTab
    );
    assert_eq!(
        PaletteCloseLabel::for_pane_count(2),
        PaletteCloseLabel::ChooseTarget
    );
}

#[test]
fn palette_clear_filter_emits_clear_pane() {
    let mut d = palette();
    for &c in b"clear" {
        d.handle_key(&[c], None);
    }
    match d.handle_key(b"\r", None) {
        DialogAction::Command(cmd) => assert_eq!(cmd, PaletteCommand::ClearPane),
        other => panic!("expected ClearPane, got {other:?}"),
    }
}

#[test]
fn palette_backspace_pops_filter_char_and_resets_selection() {
    let mut d = palette_with(0, "split");
    d.handle_key(b"\x7f", None);
    let Dialog::CommandPalette { filter, .. } = &d else {
        unreachable!()
    };
    assert_eq!(filter, "spli");
}

#[test]
fn palette_q_types_into_filter_does_not_dismiss() {
    // Pre-filter dialogs dismissed on `q`; now `q` is a filter
    // character because the dialog is type-to-filter. Esc remains
    // the dismiss key.
    let mut d = palette();
    assert_eq!(d.handle_key(b"q", None), DialogAction::Redraw);
    let Dialog::CommandPalette { filter, .. } = &d else {
        unreachable!()
    };
    assert_eq!(filter, "q");
}

#[test]
fn picker_typing_sh_narrows_to_shells_section_plus_shell_row() {
    // Filter "sh" excludes every agent label but keeps the literal
    // "shell" word — so the rendered list collapses to just the
    // shells section header + the Shell row. The shells header
    // stays visible so the operator's eye reads "this is a Shell,
    // not a stray agent."
    let mut d = picker(vec!["claude", "codex", "kimi"]);
    for &c in b"sh" {
        d.handle_key(&[c], None);
    }
    let Dialog::AgentPicker { agents, filter, .. } = &d else {
        unreachable!()
    };
    let visible = picker_filtered_rows(agents, filter);
    assert_eq!(
        visible,
        vec![PickerRow::Section("shells"), PickerRow::Shell]
    );
}

#[test]
fn picker_typing_cla_filters_to_claude() {
    let mut d = picker(vec!["claude", "codex", "kimi"]);
    for &c in b"cla" {
        d.handle_key(&[c], None);
    }
    // Enter on filtered list[0] = claude
    match d.handle_key(b"\r", None) {
        DialogAction::SpawnAgent { agent, .. } => {
            assert_eq!(agent.as_deref(), Some("claude"));
        }
        other => panic!("expected SpawnAgent(claude), got {other:?}"),
    }
}

#[test]
fn picker_enter_with_empty_filtered_list_is_redraw_noop() {
    let mut d = picker(vec!["claude", "codex"]);
    for &c in b"zzz" {
        d.handle_key(&[c], None);
    }
    assert_eq!(
        d.handle_key(b"\r", None),
        DialogAction::Redraw,
        "Enter with no matches must not synthesise a SpawnAgent"
    );
}

#[test]
fn rename_tab_empty_input_clears_label() {
    let mut d = Dialog::RenameTab {
        tab_idx: 3,
        input: jackin_tui::TextField::new("").with_allow_empty(true),
    };
    match d.handle_key(b"\r", None) {
        DialogAction::RenameTab { tab_idx, label } => {
            assert_eq!(tab_idx, 3);
            assert_eq!(label, "");
        }
        other => panic!("expected RenameTab, got {other:?}"),
    }
}

#[test]
fn rename_tab_backspace_removes_last_char() {
    let mut d = Dialog::RenameTab {
        tab_idx: 0,
        input: jackin_tui::TextField::new("abc"),
    };
    assert_eq!(d.handle_key(b"\x7f", None), DialogAction::Redraw);
    let Dialog::RenameTab { input, .. } = d else {
        unreachable!()
    };
    assert_eq!(input.value(), "ab");
}

#[test]
fn rename_tab_esc_dismisses() {
    let mut d = Dialog::RenameTab {
        tab_idx: 0,
        input: jackin_tui::TextField::new("abc"),
    };
    assert_eq!(d.handle_key(b"\x1b", None), DialogAction::Dismiss);
}

#[test]
fn rename_tab_consumes_q_as_input_not_dismiss() {
    // `q` is a dismiss key for list-style dialogs but must be
    // accepted as input inside the rename-tab buffer — otherwise
    // operators can't type the letter into their tab name.
    let mut d = Dialog::RenameTab {
        tab_idx: 0,
        input: jackin_tui::TextField::new("a"),
    };
    assert_eq!(d.handle_key(b"q", None), DialogAction::Redraw);
    let Dialog::RenameTab { input, .. } = d else {
        unreachable!()
    };
    assert_eq!(input.value(), "aq");
}

fn container_info_fixture() -> Dialog {
    Dialog::ContainerInfo {
        container_name: "jk-abc123-thearchitect".to_owned(),
        role: "the-architect".to_owned(),
        focused_agent: Some("claude".to_owned()),
        workdir: "/workspace/jackin".to_owned(),
        diagnostics: ContainerInfoDiagnostics::default(),
        copied_row: None,
        hovered_row: None,
        scroll: jackin_tui::components::DialogBodyScroll::new(),
    }
}

fn container_info_with_diagnostics_fixture() -> Dialog {
    Dialog::ContainerInfo {
        container_name: "jk-abc123-thearchitect".to_owned(),
        role: "the-architect".to_owned(),
        focused_agent: Some("claude".to_owned()),
        workdir: "/workspace/jackin".to_owned(),
        diagnostics: ContainerInfoDiagnostics {
            host_version: "0.6.0-test".to_owned(),
            run_id: "jk-run-b93735".to_owned(),
            run_log_display: "~/.jackin/data/diagnostics/runs/jk-run-b93735.jsonl".to_owned(),
            run_log_href: Some(
                "file:///home/agent/.jackin/data/diagnostics/runs/jk-run-b93735.jsonl".to_owned(),
            ),
        },
        copied_row: None,
        hovered_row: None,
        scroll: jackin_tui::components::DialogBodyScroll::new(),
    }
}

fn visible_cell_for_value(
    state: &jackin_tui::components::ContainerInfoState,
    term_rows: u16,
    term_cols: u16,
    area: Rect,
    needle: &str,
) -> (u16, u16) {
    let backend = TestBackend::new(term_cols, term_rows);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            jackin_tui::components::render_container_info(frame, area, state);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let needle_chars: Vec<char> = needle.chars().collect();
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if needle_chars.iter().enumerate().all(|(offset, ch)| {
                let Ok(offset) = u16::try_from(offset) else {
                    return false;
                };
                x.saturating_add(offset) < area.x.saturating_add(area.width)
                    && buf[(x.saturating_add(offset), y)].symbol() == ch.to_string()
            }) {
                return (y, x);
            }
        }
    }
    let mut rows = Vec::new();
    for y in area.y..area.y.saturating_add(area.height) {
        let row_text = (area.x..area.x.saturating_add(area.width))
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>();
        rows.push(row_text);
    }
    panic!(
        "visible value {needle:?} not found in rendered container info:\n{}",
        rows.join("\n")
    );
}

fn pull_request_fixture() -> PullRequestInfo {
    PullRequestInfo {
        number: 123,
        title: "Surface PR context in Capsule".to_owned(),
        url: "https://github.com/jackin-project/jackin/pull/123".to_owned(),
        is_draft: false,
        checks: None,
    }
}

#[test]
fn container_info_state_keeps_run_id_bare_and_log_path_separate() {
    let d = container_info_with_diagnostics_fixture();
    let state = d
        .container_info_state_with_debug(true)
        .expect("container info state should be available");
    let rows = state.rows();
    assert_eq!(
        rows.first()
            .map(jackin_tui::components::ContainerInfoRow::value),
        Some("jk-run-b93735"),
        "Run ID must stay the first Debug info row even when capsule knows container/session facts"
    );

    let run_row = rows
        .iter()
        .find(|row| row.value() == "jk-run-b93735")
        .expect("bare run id row present");
    assert!(run_row.is_copyable());
    assert!(
        !run_row.value().contains(".jsonl"),
        "Run ID row must not contain diagnostics path"
    );

    let log_row = rows
        .iter()
        .find(|row| row.value().contains("jk-run-b93735.jsonl"))
        .expect("diagnostics log row present");
    assert!(log_row.is_copyable());
    assert_eq!(
        log_row.href(),
        Some("file:///home/agent/.jackin/data/diagnostics/runs/jk-run-b93735.jsonl")
    );
}

#[test]
fn container_info_enter_flips_copied_flag_for_render_feedback() {
    let mut d = container_info_fixture();
    drop(d.handle_key(b"\r", None));
    let Dialog::ContainerInfo { copied_row, .. } = d else {
        unreachable!()
    };
    assert_eq!(
        copied_row,
        Some(0),
        "Enter must mark the container-id row copied so the next render shows the Copied! indicator"
    );
}

#[test]
fn container_info_enter_does_not_dismiss_dialog() {
    // Operator copies once and expects to read the badge before
    // dismissing themselves — handle_key must NOT return Dismiss
    // for Enter.
    let mut d = container_info_fixture();
    let action = d.handle_key(b"\r", None);
    assert!(
        matches!(action, DialogAction::CopyToClipboard(_)),
        "Enter must request a copy, not dismiss; got {action:?}"
    );
}

#[test]
fn container_info_enter_copies_container_name() {
    let mut d = container_info_fixture();
    match d.handle_key(b"\r", None) {
        DialogAction::CopyToClipboard(payload) => {
            assert_eq!(payload, "jk-abc123-thearchitect");
        }
        other => panic!("Enter must request clipboard copy, got {other:?}"),
    }
}

#[test]
fn container_info_click_on_id_row_copies_container_name() {
    let mut d = container_info_fixture();
    let (row, col, _, _) = d.box_rect(40, 100);
    // Click the value (the cyan link), not the label column.
    match d.handle_click(row + 2, col + 22, 40, 100, None) {
        DialogAction::CopyToClipboard(payload) => {
            assert_eq!(payload, "jk-abc123-thearchitect");
        }
        other => panic!("Container ID row click must request clipboard copy, got {other:?}"),
    }
    let Dialog::ContainerInfo { copied_row, .. } = d else {
        unreachable!()
    };
    assert_eq!(copied_row, Some(0), "ID row click must show copy feedback");
}

#[test]
fn container_info_visible_debug_rows_map_to_shared_hit_targets() {
    let term_rows = 60;
    let term_cols = 100;
    let source = container_info_with_diagnostics_fixture();
    let state = source
        .container_info_state_with_debug(true)
        .expect("container info state should be available");
    let (_, col, _, width) = source.box_rect(term_rows, term_cols);
    let height = jackin_tui::components::container_info_required_height(&state);
    let area = Rect {
        x: col,
        y: 4,
        width,
        height,
    };
    let cases = [
        ("jk-run-b93735", "jk-run-b93735"),
        ("jk-abc123-thearchitect", "jk-abc123-thearchitect"),
        (
            "/home/agent",
            "/home/agent/.jackin/data/diagnostics/runs/jk-run-b93735.jsonl",
        ),
    ];

    for (visible_text, expected_payload) in cases {
        let (screen_row, screen_col) =
            visible_cell_for_value(&state, term_rows, term_cols, area, visible_text);
        let expected_row = state
            .rows()
            .iter()
            .position(|row| row.value() == expected_payload)
            .expect("expected payload should be in Debug-info state");
        assert_eq!(
            jackin_tui::components::container_info_copy_payload_at(
                area, &state, screen_col, screen_row
            ),
            Some((expected_row, expected_payload.to_owned())),
            "visible {visible_text:?} should hit its matching shared Debug-info row"
        );
    }
}

#[test]
fn container_info_visible_container_row_maps_to_dialog_hover_and_copy_target() {
    let term_rows = 60;
    let term_cols = 100;
    let source = container_info_with_diagnostics_fixture();
    let (row, col, height, width) = source.box_rect(term_rows, term_cols);
    let area = Rect {
        x: col,
        y: row,
        width,
        height,
    };
    let state = source
        .container_info_state()
        .expect("container info state should be available");
    let (screen_row, screen_col) =
        visible_cell_for_value(&state, term_rows, term_cols, area, "jk-abc123-thearchitect");

    let mut hover_dialog = source.clone();
    assert!(
        hover_dialog.set_container_info_hover(screen_row, screen_col, term_rows, term_cols),
        "hovering visible container id should update row hover"
    );
    let Dialog::ContainerInfo { hovered_row, .. } = hover_dialog else {
        unreachable!()
    };
    assert_eq!(
        hovered_row,
        Some(0),
        "visible container id hover should target matching row"
    );

    let mut click_dialog = source;
    match click_dialog.handle_click(screen_row, screen_col, term_rows, term_cols, None) {
        DialogAction::CopyToClipboard(payload) => assert_eq!(payload, "jk-abc123-thearchitect"),
        other => panic!("visible container id click must copy payload, got {other:?}"),
    }
    let Dialog::ContainerInfo { copied_row, .. } = click_dialog else {
        unreachable!()
    };
    assert_eq!(
        copied_row,
        Some(0),
        "visible container id click should show copied feedback on matching row"
    );
}

#[test]
fn container_info_click_on_other_rows_does_not_copy() {
    let mut d = container_info_fixture();
    let (row, col, _, _) = d.box_rect(40, 100);
    assert_eq!(
        d.handle_click(row + 3, col + 2, 40, 100, None),
        DialogAction::Consume
    );
    let Dialog::ContainerInfo { copied_row, .. } = d else {
        unreachable!()
    };
    assert!(
        copied_row.is_none(),
        "non-copyable rows must not show copy feedback"
    );
}

#[test]
fn container_info_clear_copy_feedback_hides_badge() {
    let mut d = Dialog::ContainerInfo {
        container_name: "jk-abc123-thearchitect".to_owned(),
        role: "the-architect".to_owned(),
        focused_agent: Some("claude".to_owned()),
        workdir: "/workspace/jackin".to_owned(),
        diagnostics: ContainerInfoDiagnostics::default(),
        copied_row: Some(0),
        hovered_row: None,
        scroll: jackin_tui::components::DialogBodyScroll::new(),
    };
    assert!(d.clear_copy_feedback());
    let Dialog::ContainerInfo { copied_row, .. } = d else {
        unreachable!()
    };
    assert!(copied_row.is_none());
}

const GITHUB_FIXTURE_BRANCH: &str = "feature/container-info";

fn github_view_for_fixture(pr: &PullRequestInfo) -> GithubContextView<'_> {
    GithubContextView {
        branch: Some(GITHUB_FIXTURE_BRANCH),
        status: PullRequestStatus::Loaded(pr),
    }
}

#[test]
fn github_context_enter_copies_pr_url_and_shows_feedback() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: jackin_tui::components::DialogBodyScroll::new(),
    };

    match d.handle_key(b"\r", Some(&view)) {
        DialogAction::CopyToClipboard(payload) => {
            assert_eq!(payload, "https://github.com/jackin-project/jackin/pull/123");
        }
        other => panic!("Enter must request PR URL copy, got {other:?}"),
    }
    assert!(d.has_copy_feedback());
}

#[test]
fn github_context_url_click_copies_pr_url() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: jackin_tui::components::DialogBodyScroll::new(),
    };
    let (row, col, _, _) = d.box_rect(40, 120);

    assert!(d.clickable_at(row + 5, col + 18, 40, 120, Some(&view)));
    match d.handle_click(row + 5, col + 18, 40, 120, Some(&view)) {
        DialogAction::CopyToClipboard(payload) => {
            assert_eq!(payload, "https://github.com/jackin-project/jackin/pull/123");
        }
        other => panic!("GitHub URL row click must request clipboard copy, got {other:?}"),
    }
    assert!(d.has_copy_feedback());
}

#[test]
fn github_context_uses_shared_focused_info_dialog() {
    let pr = pull_request_fixture();
    let d = Dialog::GitHubContext {
        copied: false,
        scroll: jackin_tui::components::DialogBodyScroll::new(),
    };

    let view = github_view_for_fixture(&pr);
    let snapshot = d.to_ratatui_snapshot(Some(&view));
    let crate::tui::components::dialog_widgets::DialogRatatuiSnapshot::DebugInfo(state) = snapshot
    else {
        panic!("GitHub context must use the shared ContainerInfoState renderer");
    };

    assert_eq!(
        state.rows()[3].value(),
        "https://github.com/jackin-project/jackin/pull/123"
    );
    assert!(
        state.rows()[3].is_copyable(),
        "GitHub URL should be the copyable shared info row"
    );
}

#[test]
fn container_info_esc_dismisses() {
    let mut d = container_info_fixture();
    assert_eq!(d.handle_key(b"\x1b", None), DialogAction::Dismiss);
}

#[test]
fn container_info_q_dismisses() {
    // ContainerInfo has no editable input, so `q` is also a valid
    // dismiss key (same as the list-style dialogs).
    let mut d = container_info_fixture();
    assert_eq!(d.handle_key(b"q", None), DialogAction::Dismiss);
}

#[test]
fn container_info_arrow_keys_are_redraw_noops() {
    // Read-only modal, no navigation. Arrow keys must neither
    // dismiss the dialog nor produce a Command-like action — a
    // bare Redraw keeps the box on screen and waits for Enter /
    // Esc.
    let mut d = container_info_fixture();
    assert_eq!(d.handle_key(b"\x1b[A", None), DialogAction::Redraw);
    assert_eq!(d.handle_key(b"\x1b[B", None), DialogAction::Redraw);
    assert_eq!(d.handle_key(b"\x1b[C", None), DialogAction::Redraw);
    assert_eq!(d.handle_key(b"\x1b[D", None), DialogAction::Redraw);
}

#[test]
fn container_info_left_and_right_keys_scroll_horizontally() {
    let mut d = container_info_fixture();

    assert_eq!(d.handle_key(b"\x1b[C", None), DialogAction::Redraw);
    let Dialog::ContainerInfo { scroll, .. } = &d else {
        unreachable!()
    };
    assert_eq!(scroll.scroll_x, 1);

    assert_eq!(d.handle_key(b"\x1b[D", None), DialogAction::Redraw);
    let Dialog::ContainerInfo { scroll, .. } = &d else {
        unreachable!()
    };
    assert_eq!(scroll.scroll_x, 0);
}

#[test]
fn container_info_clamp_body_scroll_reduces_overscroll() {
    let mut d = container_info_fixture();
    let Dialog::ContainerInfo { scroll, .. } = &mut d else {
        unreachable!()
    };
    scroll.scroll_x = u16::MAX;
    scroll.scroll_y = u16::MAX;

    d.clamp_body_scroll(40, 100, None);

    let Dialog::ContainerInfo { scroll, .. } = &d else {
        unreachable!()
    };
    assert_ne!(scroll.scroll_x, u16::MAX);
    assert_ne!(scroll.scroll_y, u16::MAX);
}

#[test]
fn github_context_clamp_body_scroll_reduces_overscroll() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: jackin_tui::components::DialogBodyScroll {
            scroll_x: u16::MAX,
            scroll_y: u16::MAX,
        },
    };

    d.clamp_body_scroll(12, 40, Some(&view));

    let Dialog::GitHubContext { scroll, .. } = &d else {
        unreachable!()
    };
    assert_ne!(scroll.scroll_x, u16::MAX);
    assert_ne!(scroll.scroll_y, u16::MAX);
}

#[test]
fn agent_picker_section_labels_are_bare_not_dash_padded() {
    // Defect 28 regression: section labels must be bare text ("agents", "shells")
    // not "── agents ──". render_separator adds the surrounding dashes; if the
    // label already contains them, the output doubles.
    let d = picker(vec!["claude"]);
    let snapshot = d.to_ratatui_snapshot(None);
    use crate::tui::components::dialog_widgets::{DialogRatatuiSnapshot, PickerItem};
    if let DialogRatatuiSnapshot::FilterPicker { items, .. } = snapshot {
        for item in &items {
            if let PickerItem::Section(label) = item {
                assert!(
                    !label.contains("──"),
                    "section label must be bare text, not dash-padded: {label:?}"
                );
                assert!(!label.is_empty(), "section label must not be empty");
            }
        }
    } else {
        panic!("expected FilterPicker snapshot");
    }
}

#[test]
fn exit_dirty_enter_routes_each_row() {
    let expected = [
        ExitDirtyRow::StartNewAgent,
        ExitDirtyRow::Inspect,
        ExitDirtyRow::Keep,
        ExitDirtyRow::Discard,
    ];
    for (steps, want) in expected.iter().enumerate() {
        let mut d = Dialog::new_exit_dirty(vec!["jackin   1 changed".to_owned()]);
        for _ in 0..steps {
            d.handle_key(b"\x1b[B", None);
        }
        match d.handle_key(b"\r", None) {
            DialogAction::ExitDirty(row) => assert_eq!(row, *want),
            other => panic!("row {steps}: expected ExitDirty, got {other:?}"),
        }
    }
}

#[test]
fn exit_dirty_esc_is_ignored_forced_choice() {
    let mut d = Dialog::new_exit_dirty(vec!["x".to_owned()]);
    assert_eq!(d.handle_key(b"\x1b", None), DialogAction::Consume);
}

#[test]
fn exit_dirty_navigation_clamps_at_ends() {
    // Up at the top stays on the first row.
    let mut top = Dialog::new_exit_dirty(vec!["x".to_owned()]);
    top.handle_key(b"\x1b[A", None);
    assert!(matches!(
        top.handle_key(b"\r", None),
        DialogAction::ExitDirty(ExitDirtyRow::StartNewAgent)
    ));
    // Down past the end clamps to the last row.
    let mut bottom = Dialog::new_exit_dirty(vec!["x".to_owned()]);
    for _ in 0..10 {
        bottom.handle_key(b"\x1b[B", None);
    }
    assert!(matches!(
        bottom.handle_key(b"\r", None),
        DialogAction::ExitDirty(ExitDirtyRow::Discard)
    ));
}

#[test]
fn exit_inspect_esc_walks_back() {
    let mut d = Dialog::new_exit_inspect(vec![
        InspectRow::Repo("jackin".to_owned()),
        InspectRow::File("M a.rs".to_owned()),
    ]);
    assert_eq!(d.handle_key(b"\x1b", None), DialogAction::Dismiss);
}

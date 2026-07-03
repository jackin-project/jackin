//! Tests for `jackin-capsule` dialog components.
#![allow(clippy::too_many_lines)]
use std::sync::Arc;

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
fn spawn_failure_popup_uses_error_popup_hints_and_dismiss_keys() {
    let mut dialog = Dialog::SpawnFailure(jackin_tui::components::ErrorPopupState::new(
        "Spawn failed",
        "shell: cap hit",
    ));
    assert_eq!(
        dialog.footer_hint_spans(None, jackin_tui::components::ScrollAxes::none()),
        jackin_tui::components::error_popup_hint_spans()
    );
    assert_eq!(dialog.handle_key(b"x", None), DialogAction::Redraw);
    assert_eq!(dialog.handle_key(b"\x1b", None), DialogAction::Dismiss);
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
            run_log_display: "/Users/operator/.jackin/data/diagnostics/runs/jk-run-b93735.jsonl"
                .to_owned(),
            run_log_href: Some(
                "file:///Users/operator/.jackin/data/diagnostics/runs/jk-run-b93735.jsonl"
                    .to_owned(),
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
        Some("file:///Users/operator/.jackin/data/diagnostics/runs/jk-run-b93735.jsonl")
    );
    let reveal_row = rows
        .iter()
        .find(|row| row.href().is_some() && !row.is_copyable())
        .expect("diagnostics reveal row present");
    assert_eq!(
        reveal_row.href(),
        Some("file:///Users/operator/.jackin/data/diagnostics/runs/jk-run-b93735.jsonl")
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
            "/Users/operator",
            "/Users/operator/.jackin/data/diagnostics/runs/jk-run-b93735.jsonl",
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
fn container_info_r_reveals_host_diagnostics_log_path() {
    let mut d = container_info_with_diagnostics_fixture();
    match d.handle_key(b"r", None) {
        DialogAction::RevealHostPath(path) => {
            assert_eq!(
                path,
                "/Users/operator/.jackin/data/diagnostics/runs/jk-run-b93735.jsonl"
            );
        }
        other => panic!("R must request host diagnostics reveal, got {other:?}"),
    }
}

#[test]
fn container_info_o_reveals_host_diagnostics_log_path() {
    let mut d = container_info_with_diagnostics_fixture();
    match d.handle_key(b"o", None) {
        DialogAction::RevealHostPath(path) => {
            assert_eq!(
                path,
                "/Users/operator/.jackin/data/diagnostics/runs/jk-run-b93735.jsonl"
            );
        }
        other => panic!("O must request host diagnostics reveal, got {other:?}"),
    }
}

#[test]
fn container_info_o_does_not_open_github_context_url() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = container_info_with_diagnostics_fixture();
    match d.handle_key(b"o", Some(&view)) {
        DialogAction::RevealHostPath(path) => {
            assert_eq!(
                path,
                "/Users/operator/.jackin/data/diagnostics/runs/jk-run-b93735.jsonl"
            );
        }
        other => panic!("ContainerInfo O must stay diagnostics reveal, got {other:?}"),
    }
}

#[test]
fn container_info_r_without_diagnostics_log_redraws() {
    let mut d = container_info_fixture();
    assert_eq!(d.handle_key(b"r", None), DialogAction::Redraw);
}

#[test]
fn container_info_o_without_diagnostics_log_redraws() {
    let mut d = container_info_fixture();
    assert_eq!(d.handle_key(b"o", None), DialogAction::Redraw);
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
fn github_context_o_opens_pr_url() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: jackin_tui::components::DialogBodyScroll::new(),
    };

    match d.handle_key(b"o", Some(&view)) {
        DialogAction::OpenHostUrl(url) => {
            assert_eq!(url, "https://github.com/jackin-project/jackin/pull/123");
        }
        other => panic!("O must request host PR open, got {other:?}"),
    }
}

#[test]
fn github_context_c_opens_ci_url_when_available() {
    let mut pr = pull_request_fixture();
    pr.checks = Some(
        crate::pull_request::PullRequestChecks::from_buckets(["fail"]).with_ci_url(Some(
            "https://github.com/jackin-project/jackin/actions/runs/1/job/2".to_owned(),
        )),
    );
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: jackin_tui::components::DialogBodyScroll::new(),
    };

    match d.handle_key(b"c", Some(&view)) {
        DialogAction::OpenHostUrl(url) => {
            assert_eq!(
                url,
                "https://github.com/jackin-project/jackin/actions/runs/1/job/2"
            );
        }
        other => panic!("C must request host CI open, got {other:?}"),
    }
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
fn github_context_open_rows_click_open_urls() {
    let mut pr = pull_request_fixture();
    pr.checks = Some(
        crate::pull_request::PullRequestChecks::from_buckets(["fail"]).with_ci_url(Some(
            "https://github.com/jackin-project/jackin/actions/runs/1/job/2".to_owned(),
        )),
    );
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: jackin_tui::components::DialogBodyScroll::new(),
    };
    let (row, col, _, _) = d.box_rect(40, 120);

    assert!(d.clickable_at(row + 7, col + 18, 40, 120, Some(&view)));
    match d.handle_click(row + 7, col + 18, 40, 120, Some(&view)) {
        DialogAction::OpenHostUrl(url) => {
            assert_eq!(url, "https://github.com/jackin-project/jackin/pull/123");
        }
        other => panic!("Open PR row click must request host open, got {other:?}"),
    }

    assert!(d.clickable_at(row + 8, col + 18, 40, 120, Some(&view)));
    match d.handle_click(row + 8, col + 18, 40, 120, Some(&view)) {
        DialogAction::OpenHostUrl(url) => {
            assert_eq!(
                url,
                "https://github.com/jackin-project/jackin/actions/runs/1/job/2"
            );
        }
        other => panic!("Open CI row click must request host open, got {other:?}"),
    }
}

#[test]
fn github_context_unavailable_ci_row_is_not_clickable() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: jackin_tui::components::DialogBodyScroll::new(),
    };
    let (row, col, _, _) = d.box_rect(40, 120);

    assert!(
        !d.clickable_at(row + 8, col + 18, 40, 120, Some(&view)),
        "unavailable CI row must not advertise a clickable host-open target"
    );
    assert_eq!(
        d.handle_click(row + 8, col + 18, 40, 120, Some(&view)),
        DialogAction::Consume,
        "clicking unavailable CI should be consumed inside the dialog"
    );
    assert_eq!(
        d.handle_key(b"c", Some(&view)),
        DialogAction::Redraw,
        "C shortcut should not open a host URL without a CI target"
    );
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

fn usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    jackin_protocol::control::FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some("OpenAI".to_owned()),
        account: jackin_protocol::control::FocusedAccountHeader {
            provider_label: "OpenAI / Codex".to_owned(),
            account_label: "alexey@example.com".to_owned(),
            username: None,
            plan_label: Some("Pro 20x".to_owned()),
            credential_origin: None,
        },
        buckets: vec![
            jackin_protocol::control::QuotaBucketView {
                used_money: None,
                limit_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Session".to_owned(),
                used_label: Some("63% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(37),
                reset_label: Some("Resets 15:07".to_owned()),
                resets_at: None,
                status_slot: None,
                pace_label: Some("10% in reserve".to_owned()),
                status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
            },
            jackin_protocol::control::QuotaBucketView {
                used_money: None,
                limit_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Credits".to_owned(),
                used_label: None,
                limit_label: None,
                remaining_percent: None,
                reset_label: None,
                resets_at: None,
                status_slot: None,
                pace_label: Some("ACP billing unavailable".to_owned()),
                status: jackin_protocol::control::UsageSnapshotStatus::Unsupported,
            },
        ],
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        source: jackin_protocol::control::UsageSource::Cli,
        confidence: jackin_protocol::control::UsageConfidence::Authoritative,
        fetched_at_epoch: 1_781_185_560,
        updated_label: "Updated just now".to_owned(),
        status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
        tabs: vec![
            jackin_protocol::control::UsageProviderTab {
                label: "Codex".to_owned(),
                status_label: "37% left · Resets in 1h 21m (Jun 17, 23:15)".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: Some("Pro 20x".to_owned()),
                source_label: Some("fresh · provider".to_owned()),
                active: true,
            },
            jackin_protocol::control::UsageProviderTab {
                label: "Claude".to_owned(),
                status_label: "16% left · Resets in 46m (Jun 17, 22:40)".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: Some("Max".to_owned()),
                source_label: Some("stale · provider".to_owned()),
                active: false,
            },
            jackin_protocol::control::UsageProviderTab {
                label: "Amp".to_owned(),
                status_label: "unsupported".to_owned(),
                account_label: "account unavailable".to_owned(),
                plan_label: None,
                source_label: None,
                active: false,
            },
            jackin_protocol::control::UsageProviderTab {
                label: "Grok Build".to_owned(),
                status_label: "needs login".to_owned(),
                account_label: "account unavailable".to_owned(),
                plan_label: None,
                source_label: Some("needs-login · provider".to_owned()),
                active: false,
            },
            jackin_protocol::control::UsageProviderTab {
                label: "GLM / Z.AI".to_owned(),
                status_label: "88% left · Resets in 4d (Jun 21, 00:00)".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: Some("GLM Coding".to_owned()),
                source_label: Some("fresh · provider".to_owned()),
                active: false,
            },
            jackin_protocol::control::UsageProviderTab {
                label: "Kimi".to_owned(),
                status_label: "72% left · Resets in 13h (Jun 18, 11:00)".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: Some("Moonshot".to_owned()),
                source_label: Some("fresh · provider".to_owned()),
                active: false,
            },
            jackin_protocol::control::UsageProviderTab {
                label: "MiniMax".to_owned(),
                status_label: "100% left".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: Some("M1 Coding".to_owned()),
                source_label: Some("fresh · provider".to_owned()),
                active: false,
            },
        ],
        last_error: None,
    }
}

fn usage_status_bucket(
    label: &str,
    status: jackin_protocol::control::UsageSnapshotStatus,
) -> jackin_protocol::control::QuotaBucketView {
    jackin_protocol::control::QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: jackin_protocol::control::UsageSeverity::default(),
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status,
    }
}

fn quota_bucket(
    label: &str,
    remaining_percent: u8,
    reset_label: Option<&str>,
    pace_label: Option<&str>,
) -> jackin_protocol::control::QuotaBucketView {
    jackin_protocol::control::QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: jackin_protocol::control::UsageSeverity::default(),
        label: label.to_owned(),
        used_label: Some(format!("{}% used", 100u8.saturating_sub(remaining_percent))),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(remaining_percent),
        reset_label: reset_label.map(str::to_owned),
        resets_at: None,
        status_slot: None,
        pace_label: pace_label.map(str::to_owned),
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
    }
}

fn text_bucket(label: &str, value: &str) -> jackin_protocol::control::QuotaBucketView {
    jackin_protocol::control::QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: jackin_protocol::control::UsageSeverity::default(),
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: Some(value.to_owned()),
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
    }
}

fn provider_usage_view_fixture(
    tab_label: &str,
    provider_label: &str,
    account_label: &str,
    plan_label: Option<&str>,
    updated_label: &str,
    buckets: Vec<jackin_protocol::control::QuotaBucketView>,
) -> jackin_protocol::control::FocusedUsageView {
    let mut view = usage_view_fixture();
    view.focused_provider = Some(provider_label.to_owned());
    view.account = jackin_protocol::control::FocusedAccountHeader {
        provider_label: provider_label.to_owned(),
        account_label: account_label.to_owned(),
        username: None,
        plan_label: plan_label.map(str::to_owned),
        credential_origin: None,
    };
    view.updated_label = updated_label.to_owned();
    view.buckets = buckets;
    for tab in &mut view.tabs {
        tab.active = tab.label == tab_label;
    }
    view
}

fn openai_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    let mut credits = quota_bucket("Credits", 0, None, None);
    credits.used_label = None;
    credits.limit_label = Some("1K tokens".to_owned());
    provider_usage_view_fixture(
        "Codex",
        "OpenAI",
        "alexey@chainargos.com",
        Some("Pro 20x"),
        "Updated 1m ago",
        vec![
            quota_bucket("Session", 97, Some("Resets 19:45"), Some("33% in reserve")),
            quota_bucket(
                "Weekly",
                19,
                Some("Resets tomorrow, 04:18"),
                Some("12% in reserve"),
            ),
            quota_bucket("Codex Spark 5-hour", 100, Some("Resets 21:31"), None),
            quota_bucket(
                "Codex Spark Weekly",
                100,
                Some("Resets Jul 1 at 16:31"),
                None,
            ),
            text_bucket(
                "Limit Reset Credits",
                "2 manual resets available · Next expires Jul 12 at 08:14",
            ),
            credits,
        ],
    )
}

fn anthropic_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "Claude",
        "Anthropic",
        "alexey@chainargos.com",
        Some("Max"),
        "Updated 2m ago",
        vec![
            quota_bucket(
                "Session",
                89,
                Some("Resets in 2h 12m (Jun 17, 19:19)"),
                Some("34% in reserve"),
            ),
            // limits-array shape: weekly_all is labelled "All models", and a
            // model-scoped window (Fable) renders as its own non-headline row.
            quota_bucket(
                "All models",
                55,
                Some("Resets in 1w 1d (Jun 26, 13:59)"),
                Some("28% in reserve"),
            ),
            quota_bucket("Fable", 57, Some("Resets in 1w 1d (Jun 26, 13:59)"), None),
            quota_bucket("Sonnet", 85, Some("Resets in 1w 1d (Jun 26, 13:59)"), None),
        ],
    )
}

fn amp_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "Amp",
        "Amp",
        "alexey@zhokhov.com",
        Some("Amp Free"),
        "Updated just now",
        vec![
            jackin_protocol::control::QuotaBucketView {
                used_money: None,
                limit_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Amp Free".to_owned(),
                used_label: Some("$9.60".to_owned()),
                limit_label: Some("$10".to_owned()),
                remaining_percent: Some(4),
                reset_label: Some("Resets in 22h 40m".to_owned()),
                resets_at: None,
                status_slot: None,
                pace_label: None,
                status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
            },
            jackin_protocol::control::QuotaBucketView {
                used_money: None,
                limit_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Individual credits".to_owned(),
                used_label: None,
                limit_label: Some("$4.76".to_owned()),
                remaining_percent: None,
                reset_label: None,
                resets_at: None,
                status_slot: None,
                pace_label: Some("Individual credits: $4.76".to_owned()),
                status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
            },
        ],
    )
}

fn xai_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "Grok Build",
        "xAI",
        "alexey@chainargos.com",
        Some("SuperGrok"),
        "Updated 4m ago",
        vec![quota_bucket(
            "Weekly",
            18,
            Some("Resets Jul 1 at 07:00"),
            None,
        )],
    )
}

fn zai_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "GLM / Z.AI",
        "Z.AI",
        "alexey@chainargos.com",
        None,
        "Updated 2m ago",
        vec![
            quota_bucket("Tokens", 99, Some("Resets Jun 27 at 15:27"), None),
            quota_bucket(
                "MCP",
                100,
                Some("Resets Jul 13 at 15:27"),
                Some("0 / 100 (100 remaining)"),
            ),
            quota_bucket("5-hour", 100, Some("Resets 5 hours window"), None),
        ],
    )
}

fn kimi_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "Kimi",
        "Kimi",
        "alexey@chainargos.com",
        None,
        "Updated 4m ago",
        vec![
            quota_bucket("Weekly", 100, Some("Resets Jul 1 at 15:17"), None),
            quota_bucket(
                "Rate Limit",
                100,
                Some("Resets 17:17"),
                Some("86% in reserve"),
            ),
        ],
    )
}

fn minimax_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "MiniMax",
        "MiniMax",
        "alexey@chainargos.com",
        None,
        "Updated 3m ago",
        vec![
            quota_bucket(
                "General · 5h",
                100,
                Some("Resets 28m"),
                Some("Usage: 0 / 100"),
            ),
            quota_bucket(
                "General · Weekly",
                99,
                Some("Resets 4d"),
                Some("Usage: 1 / 100"),
            ),
            quota_bucket("Video", 100, Some("Resets 14h"), Some("Usage: 0 / 100")),
        ],
    )
}

fn render_usage_dialog_snapshot(width: u16, height: u16, tab: UsageDialogTab) -> String {
    render_usage_dialog_snapshot_for_view(width, height, tab, usage_view_fixture())
}

fn render_usage_dialog_snapshot_for_view(
    width: u16,
    height: u16,
    tab: UsageDialogTab,
    view: jackin_protocol::control::FocusedUsageView,
) -> String {
    let d = Dialog::new_usage_with_tab(view, tab);
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(height, width);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn usage_tab_text_position(d: &Dialog, height: u16, width: u16, label: &str) -> (u16, u16) {
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(height, width);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    for y in 0..height {
        let line = (0..width).map(|x| buf[(x, y)].symbol()).collect::<String>();
        if let Some(x) = line.find(label) {
            return (y, u16::try_from(x).expect("tab label column fits u16"));
        }
    }
    panic!("usage tab label {label:?} not rendered");
}

#[test]
fn usage_dialog_renders_auth_source_and_omits_blank_email() {
    // P1: credential_origin renders as its own "Auth:" line (the source, never
    // the secret); an empty account_label shows neither an email nor the
    // "account unavailable" placeholder; username and plan share line 2.
    let mut view = usage_view_fixture();
    view.account = jackin_protocol::control::FocusedAccountHeader {
        provider_label: "Z.AI".to_owned(),
        account_label: String::new(),
        username: Some("donbeave".to_owned()),
        plan_label: Some("GLM Coding".to_owned()),
        credential_origin: Some("API token \u{b7} env ZAI_API_KEY".to_owned()),
    };
    let snapshot = render_usage_dialog_snapshot_for_view(120, 40, UsageDialogTab::Provider, view);
    assert!(
        snapshot.contains("Auth: API token \u{b7} env ZAI_API_KEY"),
        "auth source line missing:\n{snapshot}"
    );
    assert!(
        snapshot.contains("donbeave \u{b7} GLM Coding"),
        "username \u{b7} plan line missing:\n{snapshot}"
    );
    assert!(
        !snapshot.contains("account unavailable"),
        "blank email must be omitted, not labelled unavailable:\n{snapshot}"
    );
}

#[test]
fn usage_dialog_renders_usage_status_rows_for_error_and_stale_states() {
    let mut values = Vec::new();
    for status in [
        jackin_protocol::control::UsageSnapshotStatus::NeedsLogin,
        jackin_protocol::control::UsageSnapshotStatus::Stale,
        jackin_protocol::control::UsageSnapshotStatus::Unsupported,
        jackin_protocol::control::UsageSnapshotStatus::Error,
    ] {
        let mut view = usage_view_fixture();
        view.status = status;
        let d = Dialog::new_usage(view);
        values.extend(
            d.usage_state()
                .expect("usage state")
                .rows()
                .iter()
                .map(|row| row.value().to_owned()),
        );
    }

    assert!(values.iter().any(|value| value == "needs login"));
    assert!(values.iter().any(|value| value == "stale"));
    assert!(values.iter().any(|value| value == "unsupported"));
    assert!(values.iter().any(|value| value == "error"));
}

#[test]
fn usage_dialog_renders_bucket_status_rows_for_error_states() {
    let mut view = usage_view_fixture();
    view.buckets = vec![
        usage_status_bucket(
            "Tokens",
            jackin_protocol::control::UsageSnapshotStatus::NeedsLogin,
        ),
        usage_status_bucket(
            "Weekly",
            jackin_protocol::control::UsageSnapshotStatus::Stale,
        ),
        usage_status_bucket(
            "Credits",
            jackin_protocol::control::UsageSnapshotStatus::Unsupported,
        ),
        usage_status_bucket(
            "Detail",
            jackin_protocol::control::UsageSnapshotStatus::Error,
        ),
    ];
    let d = Dialog::new_usage(view);
    let state = d.usage_state().expect("usage state");
    let values: Vec<&str> = state
        .rows()
        .iter()
        .map(jackin_tui::components::ContainerInfoRow::value)
        .collect();

    assert!(values.iter().any(|value| value.contains("needs login")));
    assert!(values.iter().any(|value| value.contains("stale")));
    assert!(values.iter().any(|value| value.contains("unsupported")));
    assert!(values.iter().any(|value| value.contains("error")));
}

#[test]
fn usage_dialog_rows_render_provider_quota_snapshot() {
    let d = Dialog::new_usage(usage_view_fixture());
    let state = d.usage_state().expect("usage state");
    let values: Vec<&str> = state
        .rows()
        .iter()
        .map(jackin_tui::components::ContainerInfoRow::value)
        .collect();

    assert_eq!(state.rows()[0].label(), "Focused");
    assert!(values.contains(&"codex · OpenAI · alexey@example.com"));
    assert!(values.iter().any(|value| {
        value.starts_with("████")
            && value.contains("37% left")
            && value.contains("10% in reserve")
            && value.contains("Resets 15:07")
            && !value.contains("used / 100%")
    }));
    assert!(values.contains(&"ACP billing unavailable · unsupported"));
    assert!(values.contains(&"fresh"));
    assert!(values.contains(&"Updated just now"));
    let rows_debug = format!("{:?}", state.rows());
    assert!(!rows_debug.contains("Account availability"));
    assert!(rows_debug.contains("Header"));
    assert!(!rows_debug.contains("Instance"));
    assert!(!values.contains(&"local diagnostic detail"));
}

#[test]
fn usage_dialog_renders_shared_provider_tab_strip_labels() {
    let d = Dialog::new_usage(usage_view_fixture());
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(32, 120);
    let backend = TestBackend::new(120, 32);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..32)
        .map(|y| (0..120).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Overview"), "{rendered}");
    assert!(rendered.contains("OpenAI"), "{rendered}");
    assert!(rendered.contains("Anthropic"), "{rendered}");
    assert!(rendered.contains("Amp"), "{rendered}");
}

#[test]
fn usage_dialog_provider_tabs_are_clickable() {
    let mut d = Dialog::new_usage_with_tab(usage_view_fixture(), UsageDialogTab::Overview);
    let (tab_row, tab_col) = usage_tab_text_position(&d, 32, 120, "Anthropic");

    assert!(d.clickable_at(tab_row, tab_col, 32, 120, None));
    match d.handle_click(tab_row, tab_col, 32, 120, None) {
        DialogAction::SwitchUsageProvider { provider_label } => {
            assert_eq!(provider_label, "Claude");
        }
        other => panic!("expected provider switch, got {other:?}"),
    }
}

#[test]
fn usage_dialog_provider_tab_hover_uses_shared_tab_hover_color() {
    let mut d = Dialog::new_usage_with_tab(usage_view_fixture(), UsageDialogTab::Overview);
    let (tab_row, tab_col) = usage_tab_text_position(&d, 32, 120, "Anthropic");

    assert!(d.set_usage_tab_hover(tab_row, tab_col, 32, 120));

    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(32, 120);
    let backend = TestBackend::new(120, 32);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    assert_eq!(
        terminal.backend().buffer()[(tab_col, tab_row)].bg,
        jackin_tui::theme::TAB_BG_INACTIVE_HOVER
    );
}

#[test]
fn usage_dialog_overview_tab_click_selects_overview() {
    let mut d = Dialog::new_usage(usage_view_fixture());
    let (tab_row, tab_col) = usage_tab_text_position(&d, 32, 120, "Overview");

    assert_eq!(
        d.handle_click(tab_row, tab_col, 32, 120, None),
        DialogAction::Redraw
    );
    assert_eq!(d.usage_selected_tab(), Some(UsageDialogTab::Overview));
}

#[test]
fn usage_dialog_renders_deficit_and_runout_quota_labels() {
    let mut view = usage_view_fixture();
    view.buckets
        .push(jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Weekly".to_owned(),
            used_label: Some("40% used".to_owned()),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(60),
            reset_label: Some("Resets Jun 17 at 23:15".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("31% in deficit · Runs out in 21h 45m".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        });
    let d = Dialog::new_usage(view);
    let state = d.usage_state().expect("usage state");
    let values: Vec<&str> = state
        .rows()
        .iter()
        .map(jackin_tui::components::ContainerInfoRow::value)
        .collect();

    assert!(values.iter().any(|value| {
        value.contains("60% left")
            && value.contains("31% in deficit")
            && value.contains("Runs out in 21h 45m")
            && value.contains("Resets Jun 17 at 23:15")
    }));

    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(40, 100);
    let backend = TestBackend::new(100, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..40)
        .map(|y| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Weekly"), "{rendered}");
    assert!(rendered.contains("31% in deficit"), "{rendered}");
    assert!(rendered.contains("Runs out in 21h 45m"), "{rendered}");
    assert!(rendered.contains("Lasts until reset"), "{rendered}");
}

#[test]
fn usage_dialog_renders_dynamic_provider_quota_bucket_meters() {
    let mut view = usage_view_fixture();
    view.buckets = vec![
        jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Tokens".to_owned(),
            used_label: Some("400M".to_owned()),
            limit_label: Some("1B".to_owned()),
            remaining_percent: Some(60),
            reset_label: Some("Resets Jun 17 at 23:15".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("31% in deficit · Runs out in 21h 45m".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        },
        jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "MCP".to_owned(),
            used_label: Some("2h".to_owned()),
            limit_label: Some("5h".to_owned()),
            remaining_percent: Some(60),
            reset_label: Some("Resets 18:00".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("5 hours window".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        },
        jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Amp Free".to_owned(),
            used_label: Some("$12.00".to_owned()),
            limit_label: Some("$25.00".to_owned()),
            remaining_percent: Some(52),
            reset_label: None,
            resets_at: None,
            status_slot: None,
            pace_label: Some("replenishes +$1.00/hour".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        },
        jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "MiniMax M1 Coding plan".to_owned(),
            used_label: Some("12K".to_owned()),
            limit_label: Some("100K".to_owned()),
            remaining_percent: Some(88),
            reset_label: Some("Resets tomorrow, 02:00".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("Coding plan".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        },
    ];

    let d = Dialog::new_usage(view);
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(40, 120);
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..40)
        .map(|y| (0..120).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Tokens"), "{rendered}");
    assert!(rendered.contains("60% left"), "{rendered}");
    assert!(rendered.contains("31% in deficit"), "{rendered}");
    assert!(rendered.contains("MCP"), "{rendered}");
    assert!(rendered.contains("5 hours window"), "{rendered}");
    assert!(rendered.contains("Amp Free"), "{rendered}");
    assert!(rendered.contains("replenishes +$1.00/hour"), "{rendered}");
    assert!(rendered.contains("MiniMax M1 Coding plan"), "{rendered}");
    assert!(rendered.contains("88% left"), "{rendered}");
    assert!(rendered.contains("████"), "{rendered}");
    assert!(
        rendered
            .lines()
            .any(|line| line.chars().filter(|ch| matches!(*ch, '█' | '·')).count() >= 70),
        "quota meters must span the available dialog width: {rendered}"
    );
}

#[test]
fn usage_dialog_renders_extra_usage_monthly_cap() {
    let mut view = usage_view_fixture();
    view.buckets
        .push(jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Extra usage".to_owned(),
            used_label: Some("SGD 78.49".to_owned()),
            limit_label: Some("SGD 260.00".to_owned()),
            remaining_percent: Some(70),
            reset_label: None,
            resets_at: None,
            status_slot: Some(jackin_protocol::control::StatusSlot::Spend),
            pace_label: None,
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        });
    let d = Dialog::new_usage(view);
    let state = d.usage_state().expect("usage state");
    let values: Vec<&str> = state
        .rows()
        .iter()
        .map(jackin_tui::components::ContainerInfoRow::value)
        .collect();

    assert!(values.iter().any(|value| {
        value.contains("30% used") && value.contains("Monthly cap: SGD 78.49 / SGD 260.00")
    }));

    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(40, 100);
    let backend = TestBackend::new(100, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..40)
        .map(|y| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Extra usage"), "{rendered}");
    assert!(rendered.contains("30% used"), "{rendered}");
    assert!(
        rendered.contains("Monthly cap: SGD 78.49 / SGD 260.00"),
        "{rendered}"
    );
    let monthly = rendered
        .find("Monthly cap: SGD 78.49 / SGD 260.00")
        .expect("monthly cap");
    let used = rendered.find("30% used").expect("used percent");
    assert!(used < monthly, "{rendered}");
}

/// Bug 7: a dollar-bearing window that is NOT the spend slot (a Claude codename
/// budget such as `amber_ladder`, the enterprise contractual budget) must show
/// its used/limit dollars in the dialog — driven by the bucket's `used_money`/
/// `limit_money`, not by a `"Extra usage"` label match.
#[test]
fn usage_dialog_renders_dollar_budget_window() {
    let mut view = usage_view_fixture();
    view.buckets
        .push(jackin_protocol::control::QuotaBucketView {
            used_money: Some(jackin_protocol::control::Money::new(0, "USD", 2)),
            limit_money: Some(jackin_protocol::control::Money::new(2_500_000, "USD", 2)),
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Amber Ladder".to_owned(),
            used_label: Some("$0.00 spent".to_owned()),
            limit_label: Some("$25,000.00".to_owned()),
            remaining_percent: Some(100),
            reset_label: Some("Resets in 66d".to_owned()),
            resets_at: Some(1_788_000_000),
            status_slot: None,
            pace_label: Some("0% used".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        });
    let d = Dialog::new_usage(view);
    let state = d.usage_state().expect("usage state");
    let values: Vec<&str> = state
        .rows()
        .iter()
        .map(jackin_tui::components::ContainerInfoRow::value)
        .collect();
    assert!(
        values
            .iter()
            .any(|value| value.contains("Budget: $0.00 spent / $25,000.00")),
        "dollar-window cap must render: {values:?}"
    );
}

#[test]
fn usage_dialog_overview_tab_renders_cross_provider_summary() {
    let d = Dialog::new_usage_with_tab(usage_view_fixture(), UsageDialogTab::Overview);
    let state = d.usage_state().expect("usage state");
    let values: Vec<&str> = state
        .rows()
        .iter()
        .map(jackin_tui::components::ContainerInfoRow::value)
        .collect();
    let rows_debug = format!("{:?}", state.rows());

    assert!(!rows_debug.contains("Focused agent"));
    assert!(!rows_debug.contains("Focused account"));
    assert!(rows_debug.contains("OpenAI"));
    assert!(rows_debug.contains("Anthropic"));
    assert!(rows_debug.contains("xAI"));
    assert!(rows_debug.contains("Z.AI"));
    assert!(values.contains(&"37% left · Resets in 1h 21m (Jun 17, 23:15)"));
    assert!(values.contains(&"16% left · Resets in 46m (Jun 17, 22:40)"));
    assert!(values.contains(&"unsupported"));
    assert!(!rows_debug.contains("fresh · provider"));
    assert!(!rows_debug.contains("stale · provider"));

    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(32, 100);
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..32)
        .map(|y| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("OpenAI      37% left"), "{rendered}");
    assert!(rendered.contains("Anthropic   16% left"), "{rendered}");
    assert!(rendered.contains("Resets in 1h 21m"), "{rendered}");
    assert!(rendered.contains("(Jun 17, 23:15)"), "{rendered}");
    assert!(rendered.contains("xAI        needs login"), "{rendered}");
    assert!(!rendered.contains("alexey@example.com"), "{rendered}");
    assert!(!rendered.contains("Pro 20x"), "{rendered}");
    assert!(!rendered.contains("fresh"), "{rendered}");
    assert!(rendered.contains("unsupported"), "{rendered}");
}

#[test]
fn usage_dialog_renders_amp_individual_credits_as_credits_section() {
    let d = Dialog::new_usage(amp_usage_view_fixture());
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(32, 100);
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..32)
        .map(|y| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Amp"), "{rendered}");
    assert!(rendered.contains("alexey@zhokhov.com"), "{rendered}");
    assert!(rendered.contains("Amp Free"), "{rendered}");
    assert!(rendered.contains("4% left"), "{rendered}");
    assert!(rendered.contains("Resets in 22h 40m"), "{rendered}");
    assert!(rendered.contains("Credits"), "{rendered}");
    assert!(rendered.contains("Individual credits: $4.76"), "{rendered}");
    assert!(
        !rendered.contains("Individual credits  remaining"),
        "{rendered}"
    );
}

#[test]
fn usage_dialog_right_arrow_switches_to_next_provider() {
    let mut d = Dialog::new_usage(usage_view_fixture());

    assert_eq!(
        d.handle_key(b"\x1b[C", None),
        DialogAction::SwitchUsageProvider {
            provider_label: "Claude".to_owned()
        }
    );
}

#[test]
fn usage_dialog_tab_key_moves_focus_to_content() {
    let mut d = Dialog::new_usage(usage_view_fixture());

    assert_eq!(d.handle_key(b"\t", None), DialogAction::Redraw);
    let Dialog::Usage {
        tab_bar_focused, ..
    } = d
    else {
        panic!("usage dialog");
    };
    assert!(!tab_bar_focused);
}

#[test]
fn usage_dialog_left_arrow_from_first_provider_switches_to_overview() {
    let mut d = Dialog::new_usage(usage_view_fixture());

    assert_eq!(d.handle_key(b"\x1b[D", None), DialogAction::Redraw);
    let state = d.usage_state().expect("usage state");
    assert_eq!(state.rows()[0].label(), "OpenAI");
    assert_eq!(
        state.rows()[0].value(),
        "37% left · Resets in 1h 21m (Jun 17, 23:15)"
    );
}

#[test]
fn usage_dialog_renders_inside_narrow_terminal() {
    let d = Dialog::new_usage(usage_view_fixture());
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(18, 60);
    let backend = TestBackend::new(60, 18);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..18)
        .map(|y| (0..60).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Usage"), "{rendered}");
    assert!(rendered.contains("OpenAI"), "{rendered}");
    assert!(!rendered.contains("OpenAI / Codex"), "{rendered}");
    assert!(rendered.contains("alexey@example.com"), "{rendered}");
    assert!(rendered.contains("Pro 20x"), "{rendered}");
    assert!(rendered.contains("Updated just now"), "{rendered}");
    assert!(!rendered.contains("Account availability"), "{rendered}");
    assert!(!rendered.contains("2 buckets"), "{rendered}");
    assert!(!rendered.contains("Overview  Codex"), "{rendered}");
    assert!(!rendered.contains("████"), "{rendered}");
    assert!(rendered.contains("Session  37% left"), "{rendered}");
    assert!(!rendered.contains("Focused :"), "{rendered}");
    assert!(
        rendered.contains("┃") || rendered.contains("·"),
        "{rendered}"
    );
}

#[test]
fn usage_dialog_stays_above_bottom_chrome_on_default_terminal() {
    let d = Dialog::new_usage_with_tab(zai_usage_view_fixture(), UsageDialogTab::Provider);
    let (row, _, height, _) = d.box_rect(24, 80);
    let content_bottom = crate::tui::components::status_bar::STATUS_BAR_ROWS
        + crate::tui::layout::available_content_rows(24);

    assert!(
        row + height <= content_bottom,
        "usage dialog must not overlap hint/footer chrome: row={row} height={height} content_bottom={content_bottom}"
    );
}

#[test]
fn snapshot_usage_dialog_narrow_60x18() {
    insta::assert_snapshot!(
        "usage_dialog_narrow_60x18",
        render_usage_dialog_snapshot(60, 18, UsageDialogTab::Provider)
    );
}

#[test]
fn snapshot_usage_dialog_medium_100x32_overview() {
    insta::assert_snapshot!(
        "usage_dialog_medium_100x32_overview",
        render_usage_dialog_snapshot(100, 32, UsageDialogTab::Overview)
    );
}

#[test]
fn snapshot_usage_dialog_wide_120x40() {
    insta::assert_snapshot!(
        "usage_dialog_wide_120x40",
        render_usage_dialog_snapshot(120, 40, UsageDialogTab::Provider)
    );
}

#[test]
fn snapshot_usage_dialog_openai_provider_120x48() {
    insta::assert_snapshot!(
        "usage_dialog_openai_provider_120x48",
        render_usage_dialog_snapshot_for_view(
            120,
            48,
            UsageDialogTab::Provider,
            openai_usage_view_fixture()
        )
    );
}

#[test]
fn snapshot_usage_dialog_anthropic_provider_120x42() {
    insta::assert_snapshot!(
        "usage_dialog_anthropic_provider_120x42",
        render_usage_dialog_snapshot_for_view(
            120,
            42,
            UsageDialogTab::Provider,
            anthropic_usage_view_fixture()
        )
    );
}

#[test]
fn snapshot_usage_dialog_amp_wide_100x32() {
    insta::assert_snapshot!(
        "usage_dialog_amp_wide_100x32",
        render_usage_dialog_snapshot_for_view(
            100,
            32,
            UsageDialogTab::Provider,
            amp_usage_view_fixture()
        )
    );
}

#[test]
fn snapshot_usage_dialog_xai_provider_100x28() {
    insta::assert_snapshot!(
        "usage_dialog_xai_provider_100x28",
        render_usage_dialog_snapshot_for_view(
            100,
            28,
            UsageDialogTab::Provider,
            xai_usage_view_fixture()
        )
    );
}

#[test]
fn snapshot_usage_dialog_zai_provider_100x34() {
    insta::assert_snapshot!(
        "usage_dialog_zai_provider_100x34",
        render_usage_dialog_snapshot_for_view(
            100,
            34,
            UsageDialogTab::Provider,
            zai_usage_view_fixture()
        )
    );
}

#[test]
fn snapshot_usage_dialog_kimi_provider_100x30() {
    insta::assert_snapshot!(
        "usage_dialog_kimi_provider_100x30",
        render_usage_dialog_snapshot_for_view(
            100,
            30,
            UsageDialogTab::Provider,
            kimi_usage_view_fixture()
        )
    );
}

#[test]
fn snapshot_usage_dialog_minimax_provider_100x32() {
    insta::assert_snapshot!(
        "usage_dialog_minimax_provider_100x32",
        render_usage_dialog_snapshot_for_view(
            100,
            32,
            UsageDialogTab::Provider,
            minimax_usage_view_fixture()
        )
    );
}

#[test]
fn usage_dialog_geometry_counts_rendered_section_lines() {
    let mut view = usage_view_fixture();
    view.buckets.extend([
        jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Tokens".to_owned(),
            used_label: Some("100K".to_owned()),
            limit_label: Some("1M".to_owned()),
            remaining_percent: Some(90),
            reset_label: Some("Resets Jun 17 at 14:00".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("20% in reserve".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        },
        jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "MCP".to_owned(),
            used_label: Some("2".to_owned()),
            limit_label: Some("100".to_owned()),
            remaining_percent: Some(98),
            reset_label: Some("Resets Jul 1 at 14:00".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("2 / 100 (98 remaining)".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        },
    ]);
    let d = Dialog::new_usage(view);
    let state = d.usage_state().expect("usage state");
    let usage_height = crate::tui::components::dialog_widgets::usage_info_required_height(&state);

    assert!(usage_height >= 7);
    assert_eq!(d.box_rect(50, 120).2, usage_height);
    // Bug 2: the scroll bound now uses the same width-wrapped line set and body
    // viewport (box − border − tab strip) the renderer uses. Assert overflow at a
    // wide-but-short terminal (≥64 cols → wide layout matching `usage_height`;
    // few rows → the box clamps below the content) so the dialog scrolls. A tall
    // terminal must NOT advertise vertical scroll — the content fits its body.
    // Bug 2: the scroll bound now uses the same width-wrapped line set and body
    // viewport (box − border − tab strip) the renderer uses. At a wide terminal
    // (≥64 cols → wide layout matching `usage_height`) the content overflows a
    // short box and scrolls, and fits — no vertical scroll — at a tall one.
    assert!(d.body_scroll_axes(18, 120, None).vertical);
    assert!(!d.body_scroll_axes(50, 120, None).vertical);
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
fn exec_picker_space_toggles_enter_confirms_esc_cancels() {
    use crate::exec::ExecPickerState;
    let bindings = vec![
        jackin_protocol::ExecBinding {
            name: "GH_TOKEN".into(),
            kind: jackin_protocol::ExecKind::Env,
            source: "$GH_TOKEN".into(),
        },
        jackin_protocol::ExecBinding {
            name: "API_KEY".into(),
            kind: jackin_protocol::ExecKind::Op,
            source: "op://v/i/f".into(),
        },
    ];
    let state = ExecPickerState::from_bindings("ssh".into(), vec!["sentry".into()], &bindings);
    // Two unselected rows, cursor at the top.
    assert_eq!(state.items.len(), 2);
    assert!(state.items.iter().all(|i| !i.selected));

    let mut dialog = Dialog::ExecPicker(state);
    // Space toggles the row under the cursor (GH_TOKEN) on.
    assert_eq!(dialog.handle_key(b" ", None), DialogAction::Redraw);
    // Enter confirms, carrying the command + only the selected credential.
    let action = dialog.handle_key(b"\r", None);
    let DialogAction::ExecConfirm {
        command,
        args,
        selected,
    } = action
    else {
        panic!("expected ExecConfirm, got {action:?}");
    };
    assert_eq!(command, "ssh");
    assert_eq!(args, vec!["sentry".to_owned()]);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "GH_TOKEN");
    assert_eq!(selected[0].kind, jackin_protocol::ExecKind::Env);
    assert_eq!(selected[0].source, "$GH_TOKEN");

    // Esc cancels with no command run.
    let mut cancel = Dialog::ExecPicker(ExecPickerState::from_bindings(
        "deploy".into(),
        vec![],
        &bindings,
    ));
    assert_eq!(cancel.handle_key(b"\x1b", None), DialogAction::ExecCancel);
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
        let mut d = Dialog::new_exit_dirty(vec!["jackin   1 changed".to_owned()], Arc::from([]));
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
fn exit_dirty_esc_and_ctrl_c_keep_and_exit() {
    // Reuses the shared FilterListAction::Dismiss path like every other dialog,
    // mapping dismiss to keep-and-exit so the operator never loses work and the
    // global Ctrl+C contract is preserved (no swallowed keys).
    let mut esc = Dialog::new_exit_dirty(vec!["x".to_owned()], Arc::from([]));
    assert_eq!(
        esc.handle_key(b"\x1b", None),
        DialogAction::ExitDirty(ExitDirtyRow::Keep)
    );
    let mut ctrl_c = Dialog::new_exit_dirty(vec!["x".to_owned()], Arc::from([]));
    assert_eq!(
        ctrl_c.handle_key(b"\x03", None),
        DialogAction::ExitDirty(ExitDirtyRow::Keep)
    );
}

#[test]
fn exit_dirty_navigation_clamps_at_ends() {
    // Up at the top stays on the first row.
    let mut top = Dialog::new_exit_dirty(vec!["x".to_owned()], Arc::from([]));
    top.handle_key(b"\x1b[A", None);
    assert!(matches!(
        top.handle_key(b"\r", None),
        DialogAction::ExitDirty(ExitDirtyRow::StartNewAgent)
    ));
    // Down past the end clamps to the last row.
    let mut bottom = Dialog::new_exit_dirty(vec!["x".to_owned()], Arc::from([]));
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
    let mut d = Dialog::new_exit_inspect(Arc::from([
        InspectRow::Repo("jackin".to_owned()),
        InspectRow::File("M a.rs".to_owned()),
    ]));
    assert_eq!(d.handle_key(b"\x1b", None), DialogAction::Dismiss);
}

#[test]
fn exit_dirty_selection_marker_moves_on_down_arrow() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn marker_row(d: &Dialog) -> Option<u16> {
        let backend = TestBackend::new(60, 20);
        let mut term = Terminal::new(backend).expect("backend");
        term.draw(|f| {
            let snap = d.to_ratatui_snapshot(None);
            let rect = d.box_rect(20, 60);
            crate::tui::components::dialog_widgets::render_dialog_ratatui(f, rect, &snap);
        })
        .expect("draw");
        let buf = term.backend().buffer().clone();
        (0..buf.area.height).find(|&y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_owned())
                .any(|s| s == "▸")
        })
    }

    let mut d = Dialog::new_exit_dirty(vec!["holla   1 changed".to_owned()], Arc::from([]));
    let before = marker_row(&d).expect("marker visible initially");
    assert_eq!(d.handle_key(b"\x1b[B", None), DialogAction::Redraw);
    let after = marker_row(&d).expect("marker visible after down");
    assert!(
        after > before,
        "down-arrow must move the ▸ marker down: before row {before}, after row {after}"
    );
}

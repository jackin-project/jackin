// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `jackin-capsule` dialog components.
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
    let mut dialog = Dialog::SpawnFailure(SpawnFailureState::new("Spawn failed", "shell: cap hit"));
    assert_eq!(
        dialog.footer_hint_spans(None, termrock::scroll::ScrollAxes::none()),
        vec![
            termrock::widgets::HintSpan::Key("↵/Esc"),
            termrock::widgets::HintSpan::Text("dismiss"),
        ]
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
fn coalesced_typing_builds_palette_filter() {
    // Scripted input arrives as one multi-byte `Data` chunk — every
    // printable byte must land in the filter, not drop as a no-op.
    let mut d = palette();
    assert_eq!(d.handle_key(b"spl", None), DialogAction::Redraw);
    let Dialog::CommandPalette {
        filter, selected, ..
    } = &d
    else {
        unreachable!()
    };
    assert_eq!(filter, "spl");
    assert_eq!(*selected, 0);
}

#[test]
fn coalesced_filter_then_confirm_emits_matching_command() {
    let mut d = palette();
    match d.handle_key(b"split\r", None) {
        DialogAction::Command(cmd) => assert_eq!(cmd, PaletteCommand::Split),
        other => panic!("expected Command(Split), got {other:?}"),
    }
}

#[test]
fn coalesced_escape_chunk_stays_noop() {
    // Chunks holding ESC keep whole-chunk dispatch so escape sequences
    // stay atomic: filter untouched, no confirm, no dismiss.
    let mut d = palette();
    assert_eq!(d.handle_key(b"ab\x1b[Z", None), DialogAction::Redraw);
    let Dialog::CommandPalette {
        filter, selected, ..
    } = &d
    else {
        unreachable!()
    };
    assert!(filter.is_empty());
    assert_eq!(*selected, 0);
}

#[test]
fn coalesced_direction_filter_confirms() {
    let mut d = Dialog::SplitDirectionPicker {
        selected: 0,
        filter: String::new(),
    };
    assert_eq!(
        d.handle_key(b"below\r", None),
        DialogAction::SplitDirection(SplitDirection::Below)
    );
}

#[test]
fn coalesced_agent_filter_confirms_spawn() {
    let mut d = picker(vec!["cx-b-inst"]);
    match d.handle_key(b"cx-b-inst\r", None) {
        DialogAction::SpawnAgent { agent, intent } => {
            assert_eq!(agent.as_deref(), Some("cx-b-inst"));
            assert_eq!(intent, PickerIntent::NewTab);
        }
        other => panic!("expected SpawnAgent, got {other:?}"),
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
        input: termrock::widgets::TextInputState::new("").with_allow_empty(true),
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
        input: termrock::widgets::TextInputState::new("abc"),
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
        input: termrock::widgets::TextInputState::new("abc"),
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
        input: termrock::widgets::TextInputState::new("a"),
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
        scroll: termrock::scroll::DialogScroll::new(),
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
            invocation_id: "jk-inv-b93735".to_owned(),
        },
        copied_row: None,
        hovered_row: None,
        scroll: termrock::scroll::DialogScroll::new(),
    }
}

fn visible_cell_for_value(
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
    term_rows: u16,
    term_cols: u16,
    area: Rect,
    needle: &str,
) -> (u16, u16) {
    let backend = TestBackend::new(term_cols, term_rows);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            crate::tui::components::container_info_surface::render_container_info(
                frame, area, state,
            );
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
fn container_info_state_shows_invocation_identity_without_local_artifacts() {
    let d = container_info_with_diagnostics_fixture();
    let state = d
        .container_info_state_with_debug(true)
        .expect("container info state should be available");
    let rows = state.rows();
    assert_eq!(
        rows.first()
            .map(crate::tui::components::container_info_surface::ContainerInfoRow::value),
        Some("jk-inv-b93735"),
        "invocation identity must be the first Debug info row"
    );

    let invocation_row = rows
        .iter()
        .find(|row| row.value() == "jk-inv-b93735")
        .expect("invocation identity row present");
    assert!(invocation_row.is_copyable());
    assert!(rows.iter().all(|row| row.href().is_none()));
}

#[test]
fn container_info_state_without_invocation_omits_identity_row() {
    let d = Dialog::ContainerInfo {
        container_name: "jk-abc123-thearchitect".to_owned(),
        role: "the-architect".to_owned(),
        focused_agent: Some("claude".to_owned()),
        workdir: "/workspace/jackin".to_owned(),
        diagnostics: ContainerInfoDiagnostics {
            host_version: "0.6.0-test".to_owned(),
            invocation_id: String::new(),
        },
        copied_row: None,
        hovered_row: None,
        scroll: termrock::scroll::DialogScroll::new(),
    };
    let state = d
        .container_info_state_with_debug(true)
        .expect("container info state should be available");
    let rows = state.rows();

    assert!(
        rows.iter()
            .all(|row| row.label() != "Invocation ID" && row.href().is_none()),
        "missing invocation identity must not fabricate a row or local artifact"
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
        "Enter must mark the container-id row copied so the next render shows the copied affordance"
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
    let height =
        crate::tui::components::container_info_surface::container_info_required_height(&state);
    let area = Rect {
        x: col,
        y: 4,
        width,
        height,
    };
    let cases = [
        ("jk-inv-b93735", "jk-inv-b93735"),
        ("jk-abc123-thearchitect", "jk-abc123-thearchitect"),
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
            crate::tui::components::container_info_surface::container_info_copy_payload_at(
                area, &state, screen_col, screen_row
            ),
            Some((expected_row, expected_payload.to_owned())),
            "visible {visible_text:?} should hit its matching shared Debug-info row"
        );
    }
}

#[test]
fn container_info_r_does_not_reveal_local_telemetry_artifacts() {
    let mut d = container_info_with_diagnostics_fixture();
    assert_eq!(d.handle_key(b"r", None), DialogAction::Redraw);
}

#[test]
fn container_info_o_does_not_reveal_local_telemetry_artifacts() {
    let mut d = container_info_with_diagnostics_fixture();
    assert_eq!(d.handle_key(b"o", None), DialogAction::Redraw);
}

#[test]
fn container_info_o_does_not_open_github_context_url() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = container_info_with_diagnostics_fixture();
    assert_eq!(d.handle_key(b"o", Some(&view)), DialogAction::Redraw);
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
        scroll: termrock::scroll::DialogScroll::new(),
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
        scroll: termrock::scroll::DialogScroll::new(),
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
        scroll: termrock::scroll::DialogScroll::new(),
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
        scroll: termrock::scroll::DialogScroll::new(),
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
        scroll: termrock::scroll::DialogScroll::new(),
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
        scroll: termrock::scroll::DialogScroll::new(),
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
        scroll: termrock::scroll::DialogScroll::new(),
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
        scroll: termrock::scroll::DialogScroll::new(),
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

use jackin_protocol::control::{CountQuota, Money};
use jackin_protocol::usage_broker::*;

const USAGE_FIXTURE_TIME: i64 = 1_781_185_560;

fn usage_freshness() -> UsageFreshnessV2 {
    UsageFreshnessV2 {
        generation: 7,
        phase: UsageFreshnessPhaseV2::Current,
        last_good_at_epoch: Some(USAGE_FIXTURE_TIME),
        retry_at_epoch: None,
        is_stale: false,
    }
}

fn quota_window(
    id: &str,
    rank: u32,
    label: &str,
    remaining: u8,
    reset: &str,
    pace: Option<&str>,
) -> UsageLimitWindowV2 {
    UsageLimitWindowV2 {
        window_id: id.into(),
        rank,
        category: UsageWindowCategoryV2::Session,
        label: label.into(),
        value_label: format!("{remaining}% left"),
        reset_label: reset.into(),
        remaining_percent: Some(UsagePercent::new(remaining).unwrap()),
        remaining_raw_percent: Some(i32::from(remaining)),
        used_percent: None,
        used_raw_percent: None,
        reset_at_epoch: None,
        quota_state: if remaining == 0 {
            UsageQuotaStateV2::Exhausted
        } else {
            UsageQuotaStateV2::Available
        },
        count_quota: None,
        pace_label: pace.map(str::to_owned),
        runs_out_label: None,
    }
}

fn usage_account(
    id: &str,
    rank: u32,
    label: &str,
    plan: Option<&str>,
    windows: Vec<UsageLimitWindowV2>,
) -> UsageAccountV2 {
    UsageAccountV2 {
        canonical_account_id: id.into(),
        identity_kind: UsageIdentityKindV2::ProviderAccountId,
        rank,
        display_label: label.into(),
        plan_label: plan.map(str::to_owned),
        username: None,
        auth_origin: None,
        refresh_capabilities: vec![],
        status_label: None,
        lifecycle: UsageLifecycleV2::Available,
        freshness: usage_freshness(),
        provenance_count: 1,
        windows,
        metric_groups: vec![],
        credential_expires_at_epoch: None,
        issues: vec![],
    }
}

fn usage_provider(
    id: &str,
    rank: u32,
    name: &str,
    accounts: Vec<UsageAccountV2>,
) -> UsageProviderV2 {
    UsageProviderV2 {
        provider_id: id.into(),
        display_name: name.into(),
        rank,
        membership_state: UsageMembershipStateV2::Current,
        freshness: usage_freshness(),
        accounts,
        issues: vec![],
    }
}

fn projection_with(providers: Vec<UsageProviderV2>) -> UsageProjectionV2 {
    UsageProjectionV2 {
        schema_version: UsageProjectionSchemaV2,
        projection_id: "publication-fixture-7".into(),
        generated_at_epoch: USAGE_FIXTURE_TIME,
        discovery_revision: "inventory-fixture".into(),
        broker_instance_id: "broker-fixture".into(),
        broker_generation: 7,
        refresh_state: UsageProjectionRefreshStateV2::Idle,
        providers,
        unresolved: vec![],
        unresolved_grants: vec![],
        issues: vec![],
    }
}

fn usage_view_fixture() -> UsageProjectionV2 {
    let mut codex = usage_account(
        "account-openai-a",
        0,
        "alexey@example.com",
        Some("Pro 20x"),
        vec![quota_window(
            "window-session",
            0,
            "Session",
            37,
            "Resets 15:07",
            Some("10% in reserve"),
        )],
    );
    let mut credits = quota_window(
        "window-credits",
        1,
        "Credits",
        0,
        "",
        Some("ACP billing unavailable"),
    );
    credits.quota_state = UsageQuotaStateV2::Unsupported;
    credits.remaining_percent = None;
    credits.remaining_raw_percent = None;
    credits.used_percent = None;
    credits.used_raw_percent = None;
    credits.value_label = "unsupported".into();
    codex.windows.push(credits);
    projection_with(vec![
        usage_provider("openai", 0, "OpenAI", vec![codex]),
        usage_provider(
            "anthropic",
            1,
            "Anthropic",
            vec![usage_account(
                "account-anthropic-a",
                0,
                "alexey@example.com",
                Some("Max"),
                vec![quota_window(
                    "window-weekly",
                    0,
                    "Weekly",
                    16,
                    "Resets in 46m (Jun 17, 22:40)",
                    None,
                )],
            )],
        ),
        usage_provider(
            "amp",
            2,
            "Amp",
            vec![usage_account(
                "account-amp-a",
                0,
                "account@personal.test",
                None,
                vec![],
            )],
        ),
        usage_provider(
            "xai",
            3,
            "xAI",
            vec![{
                let mut a = usage_account("account-xai-a", 0, "grok@work.test", None, vec![]);
                a.lifecycle = UsageLifecycleV2::NeedsLogin;
                a
            }],
        ),
        usage_provider(
            "zai",
            4,
            "Z.AI",
            vec![usage_account(
                "account-zai-a",
                0,
                "zai@work.test",
                Some("GLM Coding"),
                vec![quota_window(
                    "window-zai",
                    0,
                    "Tokens",
                    88,
                    "Resets in 4d (Jun 21, 00:00)",
                    None,
                )],
            )],
        ),
        usage_provider(
            "kimi",
            5,
            "Kimi",
            vec![usage_account(
                "account-kimi-a",
                0,
                "kimi@work.test",
                Some("Moonshot"),
                vec![quota_window(
                    "window-kimi",
                    0,
                    "Weekly",
                    72,
                    "Resets in 13h (Jun 18, 11:00)",
                    None,
                )],
            )],
        ),
        usage_provider(
            "minimax",
            6,
            "MiniMax",
            vec![usage_account(
                "account-minimax-a",
                0,
                "minimax@work.test",
                Some("M1 Coding"),
                vec![quota_window("window-minimax", 0, "Requests", 100, "", None)],
            )],
        ),
    ])
}

fn first_account(view: &mut UsageProjectionV2) -> &mut UsageAccountV2 {
    &mut view.providers[0].accounts[0]
}

fn usage_group(
    id: &str,
    rank: u32,
    label: &str,
    kind: UsageMetricGroupKindV2,
    value: UsageMetricValueV2,
) -> UsageMetricGroupV2 {
    UsageMetricGroupV2 {
        group_id: id.into(),
        rank,
        kind,
        label: label.into(),
        scope: UsageMetricScopeV2::default(),
        observed_at_epoch: Some(USAGE_FIXTURE_TIME),
        fetched_at_epoch: USAGE_FIXTURE_TIME,
        last_success_at_epoch: Some(USAGE_FIXTURE_TIME),
        phase: UsageFreshnessPhaseV2::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV2::NotApplicable,
        value,
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: vec![],
    }
}

fn provider_projection(
    id: &str,
    name: &str,
    account_id: &str,
    plan: &str,
    windows: Vec<UsageLimitWindowV2>,
) -> UsageProjectionV2 {
    projection_with(vec![usage_provider(
        id,
        0,
        name,
        vec![usage_account(
            account_id,
            0,
            "account@work.test",
            Some(plan),
            windows,
        )],
    )])
}

fn openai_usage_view_fixture() -> UsageProjectionV2 {
    let mut p = provider_projection(
        "openai",
        "OpenAI",
        "account-openai-a",
        "Pro 20x",
        vec![
            quota_window(
                "session",
                0,
                "Session",
                97,
                "Resets 19:45",
                Some("33% in reserve"),
            ),
            quota_window(
                "weekly",
                1,
                "Weekly",
                19,
                "Resets tomorrow, 04:18",
                Some("12% in reserve"),
            ),
            quota_window(
                "spark-short",
                2,
                "Codex Spark 5-hour",
                100,
                "Resets 21:31",
                None,
            ),
            quota_window(
                "spark-week",
                3,
                "Codex Spark Weekly",
                100,
                "Resets Jul 1 at 16:31",
                None,
            ),
        ],
    );
    let mut resets = quota_window("manual-resets", 4, "Limit Reset Credits", 0, "", None);
    resets.quota_state = UsageQuotaStateV2::NotApplicable;
    resets.remaining_percent = None;
    resets.remaining_raw_percent = None;
    resets.used_percent = None;
    resets.used_raw_percent = None;
    resets.value_label = "2 manual resets available · Next expires Jul 12 at 08:14".into();
    first_account(&mut p).windows.push(resets);
    first_account(&mut p).metric_groups.push(usage_group(
        "credits",
        0,
        "Credits",
        UsageMetricGroupKindV2::Balance,
        UsageMetricValueV2::Balance {
            amount: Money::new(1000, "tokens", 0),
            expires_at_epoch: None,
        },
    ));
    p
}
fn anthropic_usage_view_fixture() -> UsageProjectionV2 {
    let mut p = provider_projection(
        "anthropic",
        "Anthropic",
        "account-anthropic-a",
        "Max",
        vec![
            quota_window(
                "session",
                0,
                "Session",
                89,
                "Resets in 2h 12m (Jun 17, 19:19)",
                Some("34% in reserve"),
            ),
            quota_window(
                "all",
                1,
                "All models",
                55,
                "Resets in 1w 1d (Jun 26, 13:59)",
                Some("28% in reserve"),
            ),
            quota_window(
                "fable",
                2,
                "Fable",
                57,
                "Resets in 1w 1d (Jun 26, 13:59)",
                None,
            ),
            quota_window(
                "sonnet",
                3,
                "Sonnet",
                85,
                "Resets in 1w 1d (Jun 26, 13:59)",
                None,
            ),
        ],
    );
    first_account(&mut p).windows[1].category = UsageWindowCategoryV2::LongRange;
    for w in &mut first_account(&mut p).windows[2..] {
        w.category = UsageWindowCategoryV2::Model;
    }
    p
}
fn amp_usage_view_fixture() -> UsageProjectionV2 {
    let mut p = provider_projection(
        "amp",
        "Amp",
        "account-amp-a",
        "Individual",
        vec![quota_window(
            "free",
            0,
            "Amp Free",
            4,
            "Resets in 22h 40m",
            None,
        )],
    );
    first_account(&mut p).display_label = "account@personal.test".into();
    first_account(&mut p).metric_groups.push(usage_group(
        "credits",
        0,
        "Individual credits",
        UsageMetricGroupKindV2::Balance,
        UsageMetricValueV2::Balance {
            amount: Money::new(476, "USD", 2),
            expires_at_epoch: None,
        },
    ));
    p
}
fn xai_usage_view_fixture() -> UsageProjectionV2 {
    provider_projection(
        "xai",
        "xAI",
        "account-xai-a",
        "SuperGrok",
        vec![quota_window(
            "weekly",
            0,
            "Weekly",
            18,
            "Resets Jul 1 at 07:00",
            None,
        )],
    )
}
fn zai_usage_view_fixture() -> UsageProjectionV2 {
    provider_projection(
        "zai",
        "Z.AI",
        "account-zai-a",
        "GLM Coding",
        vec![
            quota_window("tokens", 0, "Tokens", 99, "Resets Jun 27 at 15:27", None),
            quota_window(
                "mcp",
                1,
                "MCP",
                100,
                "Resets Jul 13 at 15:27",
                Some("0 / 100 (100 remaining)"),
            ),
            quota_window("rolling", 2, "5-hour", 100, "Resets 5 hours window", None),
        ],
    )
}
fn kimi_usage_view_fixture() -> UsageProjectionV2 {
    provider_projection(
        "kimi",
        "Kimi",
        "account-kimi-a",
        "Moonshot",
        vec![
            quota_window("weekly", 0, "Weekly", 100, "Resets Jul 1 at 15:17", None),
            quota_window(
                "rate",
                1,
                "Rate Limit",
                100,
                "Resets 17:17",
                Some("86% in reserve"),
            ),
        ],
    )
}
fn minimax_usage_view_fixture() -> UsageProjectionV2 {
    provider_projection(
        "minimax",
        "MiniMax",
        "account-minimax-a",
        "M1 Coding",
        vec![
            quota_window(
                "short",
                0,
                "General · 5h",
                100,
                "Resets 28m",
                Some("Usage: 0 / 100"),
            ),
            quota_window(
                "weekly",
                1,
                "General · Weekly",
                99,
                "Resets 4d",
                Some("Usage: 1 / 100"),
            ),
            quota_window(
                "video",
                2,
                "Video",
                100,
                "Resets 14h",
                Some("Usage: 0 / 100"),
            ),
        ],
    )
}

fn render_usage_dialog(d: &Dialog, width: u16, height: u16) -> String {
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(height, width);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot)
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
fn render_usage_dialog_snapshot(width: u16, height: u16, tab: UsageDialogTab) -> String {
    render_usage_dialog_snapshot_for_view(width, height, tab, usage_view_fixture())
}
fn render_usage_dialog_snapshot_for_view(
    width: u16,
    height: u16,
    tab: UsageDialogTab,
    view: UsageProjectionV2,
) -> String {
    render_usage_dialog(&Dialog::new_usage_with_tab(Some(view), tab), width, height)
}
fn usage_tab_text_position(d: &Dialog, height: u16, width: u16, label: &str) -> (u16, u16) {
    let text = render_usage_dialog(d, width, height);
    for (y, line) in text.lines().enumerate() {
        if let Some(x) = line.find(label) {
            return (y as u16, x as u16);
        }
    }
    panic!("missing {label}: {text}")
}
fn provider_dialog(view: UsageProjectionV2) -> Dialog {
    Dialog::new_usage_with_tab(Some(view), UsageDialogTab::Provider)
}
fn usage_content_text(d: &Dialog) -> String {
    let state = d.usage_state().expect("usage state");
    crate::tui::components::dialog_widgets::usage_info_lines_for_width(&state, 120)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn assert_usage_contains(view: UsageProjectionV2, expected: &[&str]) {
    let text = usage_content_text(&provider_dialog(view));
    for needle in expected {
        assert!(text.contains(needle), "missing {needle}:\n{text}");
    }
}

#[test]
fn usage_projection_fixtures_have_valid_canonical_ranks_and_semantics() {
    for p in [
        usage_view_fixture(),
        openai_usage_view_fixture(),
        anthropic_usage_view_fixture(),
        amp_usage_view_fixture(),
        xai_usage_view_fixture(),
        zai_usage_view_fixture(),
        kimi_usage_view_fixture(),
        minimax_usage_view_fixture(),
    ] {
        p.validate().unwrap();
    }
}
#[test]
fn usage_projection_empty_inventory_has_no_retry_copy() {
    let d = Dialog::new_usage(Some(projection_with(vec![])));
    let text = render_usage_dialog(&d, 100, 32);
    assert!(!text.contains("retry"), "{text}");
    assert!(
        !d.footer_hint_spans(None, termrock::scroll::ScrollAxes::none())
            .iter()
            .any(|h| matches!(h, termrock::widgets::HintSpan::Key("r")))
    );
}
#[test]
fn usage_overview_renders_one_row_per_account_tab() {
    let p = projection_with(vec![usage_provider(
        "anthropic",
        0,
        "Anthropic",
        vec![
            usage_account(
                "account-a",
                0,
                "same-label",
                Some("Max"),
                vec![quota_window("a", 0, "Weekly", 40, "", None)],
            ),
            usage_account(
                "account-b",
                1,
                "same-label",
                Some("Max 20x"),
                vec![quota_window("b", 0, "Weekly", 60, "", None)],
            ),
            usage_account("account-zero", 2, "zero quota", None, vec![]),
        ],
    )]);
    let d = Dialog::new_usage(Some(p));
    let text = usage_content_text(&d);
    assert!(text.contains("40% left"), "{text}");
    assert!(text.contains("60% left"), "{text}");
    assert!(text.contains("zero quota"), "{text}");
}
#[test]
fn usage_overview_matches_provider_head_of_composite_tab_labels() {
    // Provider identity is explicit; misleading display labels cannot remap it.
    let p = projection_with(vec![usage_provider(
        "anthropic",
        0,
        "OpenAI · misleading",
        vec![usage_account("opaque-anthropic", 0, "same", None, vec![])],
    )]);
    let mut d = Dialog::new_usage(Some(p));
    assert_eq!(
        d.handle_key(b"\x1b[C", None),
        DialogAction::SwitchUsageProvider {
            provider_id: "anthropic".into(),
            canonical_account_id: "opaque-anthropic".into()
        }
    );
}
#[test]
fn usage_dialog_rows_render_provider_quota_snapshot() {
    assert_usage_contains(
        usage_view_fixture(),
        &[
            "OpenAI",
            "alexey@example.com",
            "Pro 20x",
            "Session",
            "37% left",
            "10% in reserve",
            "Resets 15:07",
            "ACP billing unavailable",
            "unsupported",
        ],
    );
}
#[test]
fn usage_dialog_renders_auth_source_and_omits_blank_email() {
    let mut p = zai_usage_view_fixture();
    let account = first_account(&mut p);
    account.display_label = String::new();
    account.username = Some("donbeave".into());
    account.auth_origin = Some("API token · env ZAI_API_KEY".into());
    account.plan_label = Some("GLM Coding".into());
    let text = render_usage_dialog_snapshot_for_view(120, 40, UsageDialogTab::Provider, p);
    assert!(
        text.contains("Auth: API token · env ZAI_API_KEY"),
        "auth source missing: {text}"
    );
    assert!(
        text.contains("Username: donbeave"),
        "distinct username missing: {text}"
    );
    assert!(text.contains("Plan: GLM Coding"), "plan missing: {text}");
    assert!(
        !text.contains("account unavailable"),
        "blank account identity must not fabricate unavailable email: {text}"
    );
}

#[test]
fn usage_dialog_renders_usage_status_rows_for_error_and_stale_states() {
    for lifecycle in [
        UsageLifecycleV2::NeedsLogin,
        UsageLifecycleV2::Unsupported,
        UsageLifecycleV2::Error,
        UsageLifecycleV2::Unavailable,
        UsageLifecycleV2::AgentUninitialized,
    ] {
        let mut p = usage_view_fixture();
        let a = first_account(&mut p);
        a.lifecycle = lifecycle;
        a.status_label = Some(format!("canonical {lifecycle:?}"));
        assert_usage_contains(p, &[&format!("canonical {lifecycle:?}")]);
    }
    let mut p = usage_view_fixture();
    first_account(&mut p).freshness.phase = UsageFreshnessPhaseV2::Stale;
    first_account(&mut p).freshness.is_stale = true;
    assert_usage_contains(p, &["stale"]);
}
#[test]
fn usage_dialog_renders_bucket_status_rows_for_error_states() {
    let states = [
        UsageQuotaStateV2::NotStarted,
        UsageQuotaStateV2::Unsupported,
        UsageQuotaStateV2::Unavailable,
        UsageQuotaStateV2::NoPermission,
        UsageQuotaStateV2::Unknown,
        UsageQuotaStateV2::NotApplicable,
        UsageQuotaStateV2::Error,
    ];
    for state in states {
        let mut p = usage_view_fixture();
        let w = &mut first_account(&mut p).windows[0];
        w.quota_state = state;
        w.remaining_percent = None;
        w.remaining_raw_percent = None;
        w.used_percent = None;
        w.used_raw_percent = None;
        w.value_label = format!("canonical {state:?}");
        assert_usage_contains(p, &[&format!("canonical {state:?}")]);
    }
}
#[test]
fn usage_provider_tab_renders_meterless_family_bucket_as_plain_row() {
    let mut p = usage_view_fixture();
    let w = &mut first_account(&mut p).windows[0];
    w.label = "Gemini family".into();
    w.value_label = "no published limit".into();
    w.remaining_percent = None;
    w.remaining_raw_percent = None;
    w.used_percent = None;
    w.used_raw_percent = None;
    w.quota_state = UsageQuotaStateV2::Unknown;
    let text = render_usage_dialog_snapshot_for_view(100, 32, UsageDialogTab::Provider, p);
    assert!(text.contains("Gemini family"), "{text}");
    assert!(text.contains("no published limit"), "{text}");
    assert!(
        !text.contains("████"),
        "unknown quota must not create meter: {text}"
    );
}
#[test]
fn usage_dialog_renders_deficit_and_runout_quota_labels() {
    let mut p = usage_view_fixture();
    first_account(&mut p).windows.push(quota_window(
        "weekly",
        2,
        "Weekly",
        60,
        "Resets Jun 17 at 23:15",
        Some("31% in deficit · Runs out in 21h 45m"),
    ));
    assert_usage_contains(
        p,
        &[
            "60% left",
            "31% in deficit",
            "Runs out in 21h 45m",
            "Resets Jun 17 at 23:15",
        ],
    );
}
#[test]
fn usage_dialog_renders_dynamic_provider_quota_bucket_meters() {
    let mut p = usage_view_fixture();
    first_account(&mut p).windows = vec![
        quota_window(
            "tokens",
            0,
            "Tokens",
            60,
            "Resets Jun 17 at 23:15",
            Some("31% in deficit"),
        ),
        quota_window("mcp", 1, "MCP", 60, "Resets 18:00", Some("5 hours window")),
        quota_window(
            "free",
            2,
            "Amp Free",
            52,
            "",
            Some("replenishes +$1.00/hour"),
        ),
        quota_window(
            "coding",
            3,
            "MiniMax M1 Coding plan",
            88,
            "Resets tomorrow",
            None,
        ),
    ];
    assert_usage_contains(
        p,
        &[
            "Tokens",
            "60% left",
            "MCP",
            "5 hours window",
            "Amp Free",
            "replenishes +$1.00/hour",
            "MiniMax M1 Coding plan",
            "88% left",
            "████",
        ],
    );
}
#[test]
fn usage_dialog_renders_extra_usage_monthly_cap() {
    let mut p = usage_view_fixture();
    first_account(&mut p).metric_groups.push(usage_group(
        "spend",
        0,
        "Extra usage",
        UsageMetricGroupKindV2::SpendCap,
        UsageMetricValueV2::SpendCap {
            cap: Some(Money::new(26000, "SGD", 2)),
            spent: Some(Money::new(7849, "SGD", 2)),
            remaining: Some(Money::new(18151, "SGD", 2)),
        },
    ));
    assert_usage_contains(p, &["Extra usage", "78.49", "260.00", "SGD"]);
}
#[test]
fn usage_dialog_renders_dollar_budget_window() {
    let mut p = usage_view_fixture();
    first_account(&mut p).metric_groups.push(usage_group(
        "budget",
        0,
        "Amber Ladder",
        UsageMetricGroupKindV2::SpendCap,
        UsageMetricValueV2::SpendCap {
            cap: Some(Money::new(2500000, "USD", 2)),
            spent: Some(Money::new(0, "USD", 2)),
            remaining: Some(Money::new(2500000, "USD", 2)),
        },
    ));
    assert_usage_contains(p, &["Amber Ladder", "25000.00", "0.00"]);
}
#[test]
fn usage_dialog_renders_amp_individual_credits_as_credits_section() {
    assert_usage_contains(
        amp_usage_view_fixture(),
        &[
            "Amp Free",
            "4% left",
            "Individual credits",
            "4.76",
            "account@personal.test",
        ],
    );
}
#[test]
fn usage_dialog_overview_tab_renders_cross_provider_summary() {
    let text = usage_content_text(&Dialog::new_usage(Some(usage_view_fixture())));
    for needle in ["OpenAI", "Anthropic", "xAI", "Z.AI", "37% left", "16% left"] {
        assert!(text.contains(needle), "{text}");
    }
}
#[test]
fn usage_dialog_renders_shared_provider_tab_strip_labels() {
    let text = usage_content_text(&Dialog::new_usage(Some(usage_view_fixture())));
    for label in ["Overview", "OpenAI", "Anthropic", "Amp"] {
        assert!(text.contains(label), "{text}");
    }
}
#[test]
fn usage_dialog_provider_tabs_are_clickable() {
    let mut d = Dialog::new_usage(Some(usage_view_fixture()));
    let (y, x) = usage_tab_text_position(&d, 40, 120, "Anthropic");
    assert!(d.clickable_at(y, x, 40, 120, None));
    assert_eq!(
        d.handle_click(y, x, 40, 120, None),
        DialogAction::SwitchUsageProvider {
            provider_id: "anthropic".into(),
            canonical_account_id: "account-anthropic-a".into()
        }
    );
}
#[test]
fn usage_dialog_provider_tab_hover_uses_shared_tab_hover_color() {
    let mut d = Dialog::new_usage(Some(usage_view_fixture()));
    let (y, x) = usage_tab_text_position(&d, 40, 120, "Anthropic");
    assert!(d.set_usage_tab_hover(y, x, 40, 120));
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let s = d.to_ratatui_snapshot(None);
    let r = d.box_rect(40, 120);
    t.draw(|f| crate::tui::components::dialog_widgets::render_dialog_ratatui(f, r, &s))
        .unwrap();
    assert!(
        t.backend().buffer()[(x, y)]
            .modifier
            .contains(ratatui::style::Modifier::UNDERLINED)
    );
}
#[test]
fn usage_dialog_overview_tab_click_selects_overview() {
    let mut d = provider_dialog(usage_view_fixture());
    let (y, x) = usage_tab_text_position(&d, 40, 120, "Overview");
    assert_eq!(d.handle_click(y, x, 40, 120, None), DialogAction::Redraw);
    assert_eq!(d.usage_selected_tab(), Some(UsageDialogTab::Overview));
}
#[test]
fn usage_dialog_right_arrow_switches_to_next_provider() {
    let mut d = provider_dialog(usage_view_fixture());
    assert_eq!(
        d.handle_key(b"\x1b[C", None),
        DialogAction::SwitchUsageProvider {
            provider_id: "anthropic".into(),
            canonical_account_id: "account-anthropic-a".into()
        }
    );
}
#[test]
fn usage_dialog_tab_key_moves_focus_to_content() {
    let mut d = provider_dialog(usage_view_fixture());
    assert_eq!(d.handle_key(b"\t", None), DialogAction::Redraw);
    assert!(!s8_usage_tab_bar_focused(&d));
}
#[test]
fn usage_dialog_left_arrow_from_first_provider_switches_to_overview() {
    let mut d = provider_dialog(usage_view_fixture());
    assert_eq!(d.handle_key(b"\x1b[D", None), DialogAction::Redraw);
    assert_eq!(d.usage_selected_tab(), Some(UsageDialogTab::Overview));
}
#[test]
fn usage_dialog_renders_inside_narrow_terminal() {
    let text = render_usage_dialog_snapshot(60, 18, UsageDialogTab::Provider);
    for n in [
        "Usage",
        "OpenAI",
        "alexey@example.com",
        "Pro 20x",
        "37% left",
    ] {
        assert!(text.contains(n), "{text}");
    }
}
#[test]
fn usage_dialog_stays_above_bottom_chrome_on_default_terminal() {
    let d = provider_dialog(zai_usage_view_fixture());
    let (row, _, height, _) = d.box_rect(24, 80);
    let bottom = crate::tui::components::status_bar::STATUS_BAR_ROWS
        + crate::tui::layout::available_content_rows(24);
    assert!(row + height <= bottom);
}
#[test]
fn usage_dialog_geometry_counts_rendered_section_lines() {
    let mut p = usage_view_fixture();
    for i in 0..15 {
        first_account(&mut p).windows.push(quota_window(
            &format!("extra-{i}"),
            i + 2,
            &format!("Tokens {i}"),
            90,
            "Resets tomorrow",
            Some("20% in reserve"),
        ));
    }
    let d = provider_dialog(p);
    assert!(d.body_scroll_axes(18, 120, None).vertical);
    assert!(!d.body_scroll_axes(100, 120, None).vertical);
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

// ---- S8 interaction evidence: keyboard, focus, scroll, refresh, resize ----

fn s8_usage_scroll(d: &Dialog) -> (u16, u16) {
    let Dialog::Usage { scroll, .. } = d else {
        panic!("usage dialog");
    };
    (scroll.scroll_x, scroll.scroll_y)
}

fn s8_usage_tab_bar_focused(d: &Dialog) -> bool {
    let Dialog::Usage {
        tab_bar_focused, ..
    } = d
    else {
        panic!("usage dialog");
    };
    *tab_bar_focused
}

#[test]
fn s8_usage_r_and_shift_r_request_refresh() {
    for key in [b"r".as_slice(), b"R".as_slice()] {
        let mut d = provider_dialog(usage_view_fixture());
        assert_eq!(
            d.handle_key(key, None),
            DialogAction::RefreshUsage,
            "key {key:?} must request a joined refresh"
        );
    }
}

#[test]
fn s8_usage_shift_tab_restores_tab_focus() {
    let mut d = provider_dialog(usage_view_fixture());
    assert!(s8_usage_tab_bar_focused(&d));
    assert_eq!(d.handle_key(b"\t", None), DialogAction::Redraw);
    assert!(!s8_usage_tab_bar_focused(&d));
    assert_eq!(d.handle_key(b"\x1b[Z", None), DialogAction::Redraw);
    assert!(s8_usage_tab_bar_focused(&d));
}

#[test]
fn s8_usage_esc_reverses_focus_then_dismisses() {
    let mut d = provider_dialog(usage_view_fixture());
    assert_eq!(d.handle_key(b"\t", None), DialogAction::Redraw);
    assert!(!s8_usage_tab_bar_focused(&d));

    // First Esc walks focus back to the tab bar (focus reversal).
    assert_eq!(d.handle_key(b"\x1b", None), DialogAction::Redraw);
    assert!(s8_usage_tab_bar_focused(&d));

    // Second Esc dismisses the dialog.
    assert_eq!(d.handle_key(b"\x1b", None), DialogAction::Dismiss);
}

#[test]
fn s8_usage_content_arrows_scroll_two_axes() {
    let mut d = provider_dialog(usage_view_fixture());
    // Tab-bar focus owns Left/Right for tab switches: no scroll movement.
    assert_eq!(
        d.handle_key(b"\x1b[C", None),
        DialogAction::SwitchUsageProvider {
            provider_id: "anthropic".to_owned(),
            canonical_account_id: "account-anthropic-a".to_owned(),
        }
    );
    assert_eq!(s8_usage_scroll(&d), (0, 0));

    // Content focus owns every arrow plus hjkl for two-axis scrolling.
    assert_eq!(d.handle_key(b"\t", None), DialogAction::Redraw);
    assert_eq!(d.handle_key(b"\x1b[B", None), DialogAction::Redraw);
    assert_eq!(s8_usage_scroll(&d), (0, 1));
    assert_eq!(d.handle_key(b"j", None), DialogAction::Redraw);
    assert_eq!(s8_usage_scroll(&d), (0, 2));
    assert_eq!(d.handle_key(b"\x1b[A", None), DialogAction::Redraw);
    assert_eq!(d.handle_key(b"k", None), DialogAction::Redraw);
    assert_eq!(s8_usage_scroll(&d), (0, 0));
    assert_eq!(d.handle_key(b"\x1b[C", None), DialogAction::Redraw);
    assert_eq!(s8_usage_scroll(&d), (1, 0));
    assert_eq!(d.handle_key(b"l", None), DialogAction::Redraw);
    assert_eq!(s8_usage_scroll(&d), (2, 0));
    assert_eq!(d.handle_key(b"\x1b[D", None), DialogAction::Redraw);
    assert_eq!(d.handle_key(b"h", None), DialogAction::Redraw);
    assert_eq!(s8_usage_scroll(&d), (0, 0));
}

#[test]
fn s8_usage_right_from_last_tab_wraps_to_overview() {
    let mut d = Dialog::new_usage_with_destination(
        Some(usage_view_fixture()),
        Some(UsageDialogDestination {
            provider_id: "minimax".into(),
            canonical_account_id: "account-minimax-a".into(),
        }),
    );
    assert_eq!(d.handle_key(b"\x1b[C", None), DialogAction::Redraw);
    assert_eq!(d.usage_selected_tab(), Some(UsageDialogTab::Overview));
}

#[test]
fn s8_usage_left_from_overview_goes_to_last_tab() {
    let mut d = Dialog::new_usage(Some(usage_view_fixture()));
    assert_eq!(
        d.handle_key(b"\x1b[D", None),
        DialogAction::SwitchUsageProvider {
            provider_id: "minimax".to_owned(),
            canonical_account_id: "account-minimax-a".to_owned(),
        }
    );
}

#[test]
fn s8_usage_removed_account_renders_honest_unavailable() {
    let mut d = provider_dialog(usage_view_fixture());
    let mut p = usage_view_fixture();
    p.providers.remove(0);
    for (i, provider) in p.providers.iter_mut().enumerate() {
        provider.rank = i as u32;
    }
    d.apply_usage_projection(p);
    assert_eq!(d.usage_selected_tab(), Some(UsageDialogTab::Overview));
    let Dialog::Usage {
        destination,
        notice,
        ..
    } = &d
    else {
        panic!("usage")
    };
    assert!(destination.is_none());
    assert!(notice.is_some(), "removed account needs honest notice");
    let text = usage_content_text(&d);
    assert!(
        !text.contains("37% left"),
        "must not show departed account: {text}"
    );
}

#[test]
fn s8_usage_shrunk_tabs_overview_renders_remaining_rows() {
    let mut p = usage_view_fixture();
    p.providers.truncate(2);
    let d = Dialog::new_usage(Some(p));
    let text = usage_content_text(&d);
    assert!(text.contains("OpenAI"));
    assert!(text.contains("Anthropic"));
    assert!(!text.contains("MiniMax"));
}

#[test]
fn s8_usage_refreshing_placeholder_renders_loading() {
    let d = Dialog::new_usage(None);
    let text = render_usage_dialog(&d, 100, 32);
    assert!(
        text.to_lowercase().contains("publication unavailable"),
        "{text}"
    );
    let Dialog::Usage { projection, .. } = d else {
        panic!("usage")
    };
    assert!(
        projection.is_none(),
        "loading cannot fabricate a broker publication"
    );
}

#[test]
fn s8_usage_long_unicode_labels_render() {
    let mut p = usage_view_fixture();
    first_account(&mut p).display_label =
        format!("work-巴黎-🚀-memo{}", "·很长的账户备注".repeat(6));
    let text = render_usage_dialog_snapshot_for_view(100, 40, UsageDialogTab::Provider, p);
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    for n in ["Usage", "🚀", "巴黎", "很长的账户备注"] {
        assert!(compact.contains(n), "{text}");
    }
}

#[test]
fn s8_usage_resize_pair_keeps_identity() {
    for (width, height) in [(80, 24), (60, 18), (120, 40)] {
        let text = render_usage_dialog_snapshot(width, height, UsageDialogTab::Provider);
        assert!(
            text.contains("alexey@example.com"),
            "account lost at {width}x{height}:\n{text}"
        );
        assert!(
            text.contains("Pro 20x"),
            "plan lost at {width}x{height}:\n{text}"
        );
        assert!(
            text.contains("current"),
            "freshness lost at {width}x{height}:\n{text}"
        );
    }
}

#[test]
fn s8_usage_extreme_scroll_still_renders_chrome() {
    let mut d = provider_dialog(usage_view_fixture());
    assert_eq!(d.handle_key(b"\t", None), DialogAction::Redraw);
    for _ in 0..500 {
        assert_eq!(d.handle_key(b"j", None), DialogAction::Redraw);
    }
    assert!(s8_usage_scroll(&d).1 >= 500);
    // Render clamps the runaway offset: chrome survives, no panic.
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
        scroll: {
            let mut __scroll = termrock::scroll::DialogScroll::default();
            __scroll.scroll_x = u16::MAX;
            __scroll.scroll_y = u16::MAX;
            __scroll
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
        invocation,
        selected,
    } = action
    else {
        panic!("expected ExecConfirm, got {action:?}");
    };
    assert_eq!(invocation.command(), "ssh");
    assert_eq!(invocation.args(), &["sentry".to_owned()]);
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

#[test]
fn trparity_capsule_exit_dirty_esc_keeps_and_exits() {
    // "Esc is ignored" (dialog.rs:269) is implemented as a redirect: the
    // dialog never returns Dismiss, so the operator cannot lose work.
    let mut d = Dialog::new_exit_dirty(vec!["jackin   1 changed".to_owned()], Arc::from([]));
    assert_eq!(
        d.handle_key(b"\x1b", None),
        DialogAction::ExitDirty(ExitDirtyRow::Keep)
    );
}

#[test]
fn trparity_capsule_exit_dirty_ctrl_c_keeps_and_exits() {
    let mut d = Dialog::new_exit_dirty(vec!["jackin   1 changed".to_owned()], Arc::from([]));
    assert_eq!(
        d.handle_key(b"\x03", None),
        DialogAction::ExitDirty(ExitDirtyRow::Keep)
    );
}

#[test]
fn trparity_capsule_exit_dirty_enter_on_inspect_row_requests_inspect() {
    // Forward walk: cursor from StartNewAgent to Inspect, Enter requests the
    // Inspect row — the action that makes the daemon push ExitInspect
    // (input_dispatch.rs:103-113).
    let mut d = Dialog::new_exit_dirty(vec!["jackin   1 changed".to_owned()], Arc::from([]));
    assert_eq!(d.handle_key(b"\x1b[B", None), DialogAction::Redraw);
    assert_eq!(
        d.handle_key(b"\r", None),
        DialogAction::ExitDirty(ExitDirtyRow::Inspect)
    );
}

#[test]
fn trparity_capsule_exit_inspect_esc_walks_back_with_dismiss() {
    // Dismiss pops one level of the daemon's dialog stack, restoring the
    // ExitDirty modal underneath.
    let mut d = Dialog::new_exit_inspect(Arc::from([
        InspectRow::Repo("jackin".to_owned()),
        InspectRow::File("M a.rs".to_owned()),
    ]));
    assert_eq!(d.handle_key(b"\x1b", None), DialogAction::Dismiss);
    assert_eq!(d.handle_key(b"\x03", None), DialogAction::Dismiss);
}

#[test]
fn trparity_capsule_exit_inspect_arrows_scroll_without_dismissing() {
    let mut d = Dialog::new_exit_inspect(Arc::from([
        InspectRow::Repo("jackin".to_owned()),
        InspectRow::File("M a.rs".to_owned()),
    ]));
    assert_eq!(d.handle_key(b"\x1b[B", None), DialogAction::Redraw);
    // Second Down clamps at the last row.
    assert_eq!(d.handle_key(b"\x1b[B", None), DialogAction::Redraw);
    assert_eq!(d.handle_key(b"\x1b[A", None), DialogAction::Redraw);
}

#[test]
fn usage_canonical_inventory_without_active_agent_or_sessions_keeps_zero_quota_accounts() {
    let p = projection_with(vec![usage_provider(
        "openai",
        0,
        "OpenAI",
        vec![
            usage_account("account-zero-a", 0, "same account label", None, vec![]),
            usage_account("account-zero-b", 1, "same account label", None, vec![]),
        ],
    )]);
    let d = Dialog::new_usage(Some(p.clone()));
    let Dialog::Usage { projection, .. } = &d else {
        panic!("usage")
    };
    assert_eq!(projection.as_deref(), Some(&p));
    let mut d = d;
    assert_eq!(
        d.handle_key(b"\x1b[C", None),
        DialogAction::SwitchUsageProvider {
            provider_id: "openai".into(),
            canonical_account_id: "account-zero-a".into()
        }
    );
    let mut d = Dialog::new_usage_with_destination(
        Some(p),
        Some(UsageDialogDestination {
            provider_id: "openai".into(),
            canonical_account_id: "account-zero-a".into(),
        }),
    );
    assert_eq!(
        d.handle_key(b"\x1b[C", None),
        DialogAction::SwitchUsageProvider {
            provider_id: "openai".into(),
            canonical_account_id: "account-zero-b".into()
        }
    );
}

#[test]
fn usage_destination_survives_renames_reordering_and_focus_changes() {
    let mut p = usage_view_fixture();
    let destination = UsageDialogDestination {
        provider_id: "anthropic".into(),
        canonical_account_id: "account-anthropic-a".into(),
    };
    let mut d = Dialog::new_usage_with_destination(Some(p.clone()), Some(destination.clone()));
    p.providers.swap(0, 1);
    for (rank, provider) in p.providers.iter_mut().enumerate() {
        provider.rank = rank as u32;
    }
    p.providers[0].display_name = "renamed provider".into();
    p.providers[0].accounts[0].display_label = "renamed account".into();
    p.projection_id = "publication-next".into();
    p.broker_generation += 1;
    d.apply_usage_projection(p.clone());
    let Dialog::Usage {
        projection,
        destination: actual,
        ..
    } = &d
    else {
        panic!("usage")
    };
    assert_eq!(actual.as_ref(), Some(&UsageDialogTarget::Account(destination)));
    assert_eq!(projection.as_deref(), Some(&p));
}

#[test]
fn usage_refresh_error_preserves_last_publication_and_destination() {
    let p = usage_view_fixture();
    let mut d = provider_dialog(p.clone());
    d.apply_usage_error("broker connection unavailable; retry".into());
    let Dialog::Usage {
        projection,
        transport_error,
        ..
    } = &d
    else {
        panic!("usage")
    };
    assert_eq!(projection.as_deref(), Some(&p));
    assert_eq!(
        d.usage_destination().unwrap().canonical_account_id,
        "account-openai-a"
    );
    assert_eq!(
        transport_error.as_deref(),
        Some("broker connection unavailable; retry")
    );
    assert_eq!(d.handle_key(b"r", None), DialogAction::RefreshUsage);
}

#[test]
fn usage_dialog_typed_groups_preserve_scopes_tokens_rate_balance_and_freshness() {
    let mut p = openai_usage_view_fixture();
    let a = first_account(&mut p);
    let mut balance = usage_group(
        "balance",
        0,
        "Prepaid balance",
        UsageMetricGroupKindV2::Balance,
        UsageMetricValueV2::Balance {
            amount: Money::new(476, "USD", 2),
            expires_at_epoch: Some(USAGE_FIXTURE_TIME + 86400),
        },
    );
    balance.scope.pool = Some("team-pool".into());
    balance.phase = UsageFreshnessPhaseV2::Stale;
    balance.is_stale = true;
    balance.issues.push(UsageIssueV2 {
        code: "balance_delayed".into(),
        scope: UsageIssueScopeV2::Group,
        recoverability: UsageIssueRecoverabilityV2::Retryable,
        message: "balance last-good retained".into(),
        retry_at_epoch: Some(USAGE_FIXTURE_TIME + 60),
    });
    let mut rate = usage_group(
        "rate",
        1,
        "Request rate",
        UsageMetricGroupKindV2::RateLimit,
        UsageMetricValueV2::RateLimit {
            limit: Some(1000),
            remaining: Some(950),
            window_label: Some("per minute".into()),
        },
    );
    rate.scope.model = Some("gpt-fixture".into());
    rate.quota_state = UsageQuotaStateV2::Available;
    let mut tokens = usage_group(
        "tokens",
        2,
        "Token totals",
        UsageMetricGroupKindV2::TokenTotals,
        UsageMetricValueV2::TokenTotals {
            input: Some(1234),
            output: Some(567),
            cached: Some(89),
            reasoning: Some(10),
            interval_label: Some("today".into()),
        },
    );
    tokens.scope.service = Some("api-fixture".into());
    a.metric_groups = vec![balance, rate, tokens];
    p.validate().unwrap();
    assert_usage_contains(
        p,
        &[
            "Prepaid balance",
            "4.76",
            "team-pool",
            "stale",
            "balance last-good retained",
            "Request rate",
            "gpt-fixture",
            "950",
            "1000",
            "per minute",
            "Token totals",
            "1234",
            "567",
            "89",
            "10",
            "api-fixture",
        ],
    );
}

#[test]
fn usage_dialog_exact_count_survives_rounded_zero_meter() {
    use jackin_protocol::control::{CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit};
    let mut p = usage_view_fixture();
    let w = &mut first_account(&mut p).windows[0];
    w.count_quota = Some(CountQuota {
        used: Some(999),
        limit: Some(1000),
        remaining: Some(1),
        unit: CountQuotaUnit::Requests,
        period: CountQuotaPeriod::UtcDaily,
        provenance: CountQuotaProvenance::ProviderReported,
    });
    w.remaining_percent = Some(UsagePercent::new(0).unwrap());
    w.remaining_raw_percent = Some(0);
    w.used_percent = None;
    w.used_raw_percent = None;
    w.quota_state = UsageQuotaStateV2::Available;
    w.value_label = "1 request left of 1000".into();
    p.validate().unwrap();
    assert_usage_contains(p, &["1 request left of 1000"]);
}

#[test]
fn usage_dialog_used_only_windows_render_remaining_geometry() {
    // Independent raw DTO, no remaining-window fixture or geometry mapper.
    for (used, state, expected_filled) in [
        (100, UsageQuotaStateV2::Exhausted, 0),
        (20, UsageQuotaStateV2::Available, 80),
        (0, UsageQuotaStateV2::Available, 100),
    ] {
        let window = UsageLimitWindowV2 {
            window_id: "used-only-window".into(),
            rank: 0,
            category: UsageWindowCategoryV2::Session,
            label: "Used-only allowance".into(),
            value_label: format!("{used}% used"),
            reset_label: String::new(),
            remaining_percent: None,
            remaining_raw_percent: None,
            used_percent: Some(UsagePercent::new(used).unwrap()),
            used_raw_percent: Some(i32::from(used)),
            reset_at_epoch: None,
            quota_state: state,
            count_quota: None,
            pace_label: None,
            runs_out_label: None,
        };
        let p = projection_with(vec![usage_provider(
            "openai",
            0,
            "OpenAI",
            vec![usage_account(
                "used-only-account",
                0,
                "Used account",
                None,
                vec![window],
            )],
        )]);
        p.validate().unwrap();
        let d = provider_dialog(p);
        let state = d.usage_state().unwrap();
        let lines = crate::tui::components::dialog_widgets::usage_info_lines_for_width(&state, 104);
        let meters: Vec<String> = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .filter(|line| line.contains('░') || line.contains('█'))
            .collect();
        assert_eq!(
            meters.len(),
            1,
            "exactly one principal meter for {used}% used"
        );
        assert_eq!(
            meters[0].chars().filter(|c| *c == '█').count(),
            expected_filled,
            "used-only geometry must show remaining allowance"
        );
        assert_eq!(
            meters[0]
                .chars()
                .filter(|c| matches!(*c, '█' | '░'))
                .count(),
            100,
            "meter geometry bounded by content width"
        );
        assert!(
            usage_content_text(&d).contains(&format!("{used}% used")),
            "authoritative used label retained"
        );
    }
}

#[test]
fn usage_dialog_principal_exact_count_retains_generic_label_and_nonempty_meter() {
    use jackin_protocol::control::{CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit};
    let window = UsageLimitWindowV2 {
        window_id: "exact-count-window".into(),
        rank: 0,
        category: UsageWindowCategoryV2::Session,
        label: "Requests".into(),
        value_label: "Provider quota".into(),
        reset_label: String::new(),
        remaining_percent: Some(UsagePercent::new(0).unwrap()),
        remaining_raw_percent: Some(0),
        used_percent: None,
        used_raw_percent: None,
        reset_at_epoch: None,
        quota_state: UsageQuotaStateV2::Available,
        count_quota: Some(CountQuota {
            used: Some(9999),
            limit: Some(10000),
            remaining: Some(1),
            unit: CountQuotaUnit::Requests,
            period: CountQuotaPeriod::UtcDaily,
            provenance: CountQuotaProvenance::ProviderReported,
        }),
        pace_label: None,
        runs_out_label: None,
    };
    let p = projection_with(vec![usage_provider(
        "openai",
        0,
        "OpenAI",
        vec![usage_account(
            "exact-count-account",
            0,
            "Count account",
            None,
            vec![window],
        )],
    )]);
    p.validate().unwrap();
    assert!(
        p.providers[0].accounts[0].metric_groups.is_empty(),
        "principal count needs no duplicate group"
    );
    let d = provider_dialog(p);
    let state = d.usage_state().unwrap();
    let lines = crate::tui::components::dialog_widgets::usage_info_lines_for_width(&state, 104);
    let text = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Provider quota"),
        "supplied value label lost: {text}"
    );
    assert!(
        text.contains("9999")
            && text.contains("10000")
            && text.contains("1 requests left")
            && text.contains("requests"),
        "exact typed summary lost: {text}"
    );
    let meters: Vec<&str> = text
        .lines()
        .filter(|line| line.contains('░') || line.contains('█'))
        .collect();
    assert_eq!(meters.len(), 1, "principal-only count renders one meter");
    assert_eq!(
        meters[0].chars().filter(|c| *c == '█').count(),
        0,
        "geometry preserves the exact-count floor contract"
    );
    assert_eq!(
        meters[0]
            .chars()
            .filter(|c| matches!(*c, '█' | '░'))
            .count(),
        100,
        "meter stays bounded"
    );
    assert!(
        !text.contains("exhausted"),
        "one request is not exhausted: {text}"
    );
}


fn unresolved_grant_projection() -> UsageProjectionV2 {
    use jackin_protocol::usage_broker::UsageUnresolvedGrantV2;
    let mut projection = projection_with(vec![]);
    projection.unresolved_grants = vec![
        UsageUnresolvedGrantV2 {
            configured_account_id: "missing-one".into(),
            surface_id: "claude".into(),
            issues: vec![UsageIssueV2 {
                code: "configured_account_missing".into(),
                scope: UsageIssueScopeV2::Account,
                recoverability: UsageIssueRecoverabilityV2::ActionRequired,
                message: "Configure credential for missing-one".into(),
                retry_at_epoch: None,
            }],
        },
        UsageUnresolvedGrantV2 {
            configured_account_id: "missing-two".into(),
            surface_id: "claude".into(),
            issues: vec![UsageIssueV2 {
                code: "configured_account_missing".into(),
                scope: UsageIssueScopeV2::Account,
                recoverability: UsageIssueRecoverabilityV2::ActionRequired,
                message: "Configure credential for missing-two".into(),
                retry_at_epoch: None,
            }],
        },
    ];
    projection
}

#[test]
fn usage_unresolved_grants_real_render_and_keyboard_navigation_keep_distinct_accounts() {
    let projection = unresolved_grant_projection();
    projection.validate().unwrap();
    let mut dialog = Dialog::new_usage(Some(projection));
    let overview = render_usage_dialog(&dialog, 160, 48);
    for expected in ["Configured account: missing-one", "Configured account: missing-two",
        "Configure credential for missing-one", "Configure credential for missing-two"] {
        assert!(overview.contains(expected), "missing {expected}: {overview}");
    }
    assert!(!overview.contains("No authorized usage accounts"));
    assert_eq!(dialog.handle_key(b"\x1b[C", None), DialogAction::Redraw);
    let first = render_usage_dialog(&dialog, 160, 48);
    assert!(first.contains("Configure credential for missing-one"));
    assert!(!first.contains("Configure credential for missing-two"));
    assert!(dialog.usage_destination().is_none(), "grant must have no canonical refresh target");
    assert!(dialog.usage_state().unwrap().refresh_unavailable);
    assert_eq!(dialog.handle_key(b"\x1b[C", None), DialogAction::Redraw);
    let second = render_usage_dialog(&dialog, 160, 48);
    assert!(second.contains("Configure credential for missing-two"));
    assert!(!second.contains("Configure credential for missing-one"));
    assert_eq!(dialog.handle_key(b"\x1b[C", None), DialogAction::Redraw);
    assert_eq!(dialog.usage_selected_tab(), Some(UsageDialogTab::Overview));
}

#[test]
fn usage_unresolved_grant_mouse_opens_exact_painted_tab_details() {
    let mut dialog = Dialog::new_usage(Some(unresolved_grant_projection()));
    let area = dialog.box_rect(48, 160);
    let snapshot = dialog.to_ratatui_snapshot(None);
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    terminal.draw(|frame| {
        crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, area, &snapshot);
    }).unwrap();
    let buffer = terminal.backend().buffer();
    let (row, _, height, _) = area;
    let (x, y) = (row..row.saturating_add(height)).find_map(|y| {
        let row: String = (0..160).map(|x| buffer[(x, y)].symbol()).collect();
        row.find("missing-two").map(|x| (row[..x].chars().count() as u16, y))
    }).expect("second grant tab painted");
    assert_eq!(dialog.handle_click(y, x, 48, 160, None), DialogAction::Redraw);
    let text = render_usage_dialog(&dialog, 160, 48);
    assert!(text.contains("Configure credential for missing-two"));
    assert!(!text.contains("Configure credential for missing-one"));
    assert!(dialog.usage_destination().is_none());
}

#[test]
fn usage_unresolved_grant_selection_uses_exact_tuple_and_reconciles_revocation() {
    let mut projection = unresolved_grant_projection();
    projection.unresolved_grants[1].configured_account_id = "missing-one".into();
    projection.unresolved_grants[1].surface_id = "codex".into();
    let target = UsageDialogTarget::UnresolvedGrant {
        configured_account_id: "missing-one".into(),
        surface_id: "codex".into(),
    };
    let mut dialog = Dialog::new_usage(Some(projection.clone()));
    assert!(dialog.select_usage_target(target.clone()));
    projection.unresolved_grants.swap(0, 1);
    projection.broker_generation += 1;
    projection.projection_id = "grant-next".into();
    assert!(dialog.apply_usage_projection(projection.clone()));
    assert_eq!(dialog.usage_state().unwrap().destination, Some(target));
    assert!(dialog.usage_state().unwrap().refresh_unavailable);
    assert!(dialog.usage_destination().is_none());
    projection.unresolved_grants.remove(0);
    projection.broker_generation += 1;
    projection.projection_id = "grant-revoked".into();
    assert!(dialog.apply_usage_projection(projection));
    assert_eq!(dialog.usage_selected_tab(), Some(UsageDialogTab::Overview));
    assert!(dialog.usage_state().unwrap().destination.is_none());
}


#[test]
fn usage_mixed_inventory_navigation_preserves_canonical_action_and_grant_inertness() {
    let mut projection = unresolved_grant_projection();
    projection.providers = vec![usage_provider("openai", 0, "OpenAI", vec![
        usage_account("canonical-existing", 0, "Existing account", None, vec![]),
    ])];
    let mut dialog = Dialog::new_usage(Some(projection));
    let text = render_usage_dialog(&dialog, 180, 60);
    assert!(text.contains("Existing account"));
    assert!(text.contains("Configure credential for missing-one"));
    assert!(text.contains("Configure credential for missing-two"));
    assert_eq!(dialog.handle_key(b"\x1b[C", None), DialogAction::SwitchUsageProvider {
        provider_id: "openai".into(),
        canonical_account_id: "canonical-existing".into(),
    });
    assert!(dialog.select_usage_destination(UsageDialogDestination {
        provider_id: "openai".into(),
        canonical_account_id: "canonical-existing".into(),
    }));
    assert_eq!(dialog.handle_key(b"\x1b[C", None), DialogAction::Redraw);
    assert!(dialog.usage_destination().is_none());
    assert_eq!(dialog.handle_key(b"\x1b[D", None), DialogAction::SwitchUsageProvider {
        provider_id: "openai".into(),
        canonical_account_id: "canonical-existing".into(),
    });
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    let mut d = picker(vec!["claude"]);
    d.handle_key(b"\x1b[B", None);
    match d.handle_key(b"\r", None) {
        DialogAction::SpawnAgent { agent, .. } => assert!(agent.is_none()),
        other => panic!("expected SpawnAgent, got {other:?}"),
    }
}

#[test]
fn picker_arrow_down_skips_section_label() {
    let mut d = picker(vec!["claude", "codex"]);

    d.handle_key(b"\x1b[B", None);
    d.handle_key(b"\x1b[B", None);
    let Dialog::AgentPicker { selected, .. } = &d else {
        unreachable!()
    };
    assert_eq!(*selected, 4, "Down must skip the shells section label");
}

#[test]
fn picker_enter_on_section_label_is_noop() {
    let mut d = Dialog::AgentPicker {
        agents: vec!["claude".to_owned()],
        selected: 0,
        intent: PickerIntent::NewTab,
        filter: String::new(),
    };
    assert_eq!(d.handle_key(b"\r", None), DialogAction::Redraw);
}

#[test]
fn click_outside_dialog_dismisses() {
    let mut d = palette();

    assert_eq!(d.handle_click(0, 0, 40, 100, None), DialogAction::Dismiss);
}

#[test]
fn clickable_at_reports_container_info_copy_target() {
    let d = container_info_fixture();
    let (row, col, _, _) = d.box_rect(40, 100);

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
    let mut d = palette();
    assert_eq!(d.handle_key(b"q", None), DialogAction::Redraw);
    let Dialog::CommandPalette { filter, .. } = &d else {
        unreachable!()
    };
    assert_eq!(filter, "q");
}

#[test]
fn picker_typing_sh_narrows_to_shells_section_plus_shell_row() {
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

    match d.handle_key(b"\r", None) {
        DialogAction::SpawnAgent { agent, .. } => {
            assert_eq!(agent.as_deref(), Some("claude"));
        }
        other => panic!("expected SpawnAgent(claude), got {other:?}"),
    }
}

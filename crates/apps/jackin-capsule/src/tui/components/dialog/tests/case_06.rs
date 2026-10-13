// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn s8_usage_r_and_shift_r_request_refresh() {
    for key in [b"r".as_slice(), b"R".as_slice()] {
        let mut d = Dialog::new_usage(usage_view_fixture());
        assert_eq!(
            d.handle_key(key, None),
            DialogAction::RefreshUsage,
            "key {key:?} must request a joined refresh"
        );
    }
}

#[test]
fn s8_usage_shift_tab_restores_tab_focus() {
    let mut d = Dialog::new_usage(usage_view_fixture());
    assert!(s8_usage_tab_bar_focused(&d));
    assert_eq!(d.handle_key(b"\t", None), DialogAction::Redraw);
    assert!(!s8_usage_tab_bar_focused(&d));
    assert_eq!(d.handle_key(b"\x1b[Z", None), DialogAction::Redraw);
    assert!(s8_usage_tab_bar_focused(&d));
}

#[test]
fn s8_usage_esc_reverses_focus_then_dismisses() {
    let mut d = Dialog::new_usage(usage_view_fixture());
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
    let mut d = Dialog::new_usage(usage_view_fixture());
    // Tab-bar focus owns Left/Right for tab switches: no scroll movement.
    assert_eq!(
        d.handle_key(b"\x1b[C", None),
        DialogAction::SwitchUsageProvider {
            provider_label: "Claude".to_owned(),
            account_id: "test-tab-claude".to_owned(),
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
    let mut view = usage_view_fixture();
    for tab in &mut view.tabs {
        tab.active = tab.id == "test-tab-minimax";
    }
    let mut d = Dialog::new_usage(view);
    assert_eq!(d.handle_key(b"\x1b[C", None), DialogAction::Redraw);
    assert_eq!(d.usage_selected_tab(), Some(UsageDialogTab::Overview));
    let state = d.usage_state().expect("usage state");
    assert_eq!(state.rows()[0].label(), "OpenAI");
}

#[test]
fn s8_usage_left_from_overview_goes_to_last_tab() {
    let mut d = Dialog::new_usage_with_tab(usage_view_fixture(), UsageDialogTab::Overview);
    assert_eq!(
        d.handle_key(b"\x1b[D", None),
        DialogAction::SwitchUsageProvider {
            provider_label: "MiniMax".to_owned(),
            account_id: "test-tab-minimax".to_owned(),
        }
    );
}

#[test]
fn s8_usage_removed_account_renders_honest_unavailable() {
    // Daemon fallback for a tab whose account left the cache (removal while
    // the dialog is open): an honest unavailable view, never a sibling.
    let view = jackin_protocol::control::FocusedUsageView::unavailable(
        "usage unavailable: account not cached",
        1_781_185_560,
    );
    let d = Dialog::new_usage(view);
    let state = d.usage_state().expect("usage state");
    assert!(
        state
            .rows()
            .iter()
            .any(|row| row.value() == "usage unavailable: account not cached"),
        "removed account must render its reason: {state:?}"
    );
    let text = render_usage_dialog_snapshot_for_view(
        100,
        32,
        UsageDialogTab::Provider,
        jackin_protocol::control::FocusedUsageView::unavailable(
            "usage unavailable: account not cached",
            1_781_185_560,
        ),
    );
    assert!(
        text.contains("usage unavailable: account not cached"),
        "{text}"
    );
}

#[test]
fn s8_usage_shrunk_tabs_overview_renders_remaining_rows() {
    let mut view = usage_view_fixture();
    view.tabs.truncate(2);
    let d = Dialog::new_usage_with_tab(view, UsageDialogTab::Overview);
    let state = d.usage_state().expect("usage state");
    assert_eq!(state.rows().len(), 2);
    assert_eq!(state.rows()[0].label(), "OpenAI");
    assert_eq!(state.rows()[1].label(), "Anthropic");
    let text = render_usage_dialog_snapshot(100, 32, UsageDialogTab::Overview);
    assert!(text.contains("Overview"), "{text}");
}

#[test]
fn s8_usage_refreshing_placeholder_renders_loading() {
    let view =
        jackin_protocol::control::FocusedUsageView::refreshing(Some("OpenAI"), 1_781_185_560);
    assert!(view.is_refreshing_placeholder());
    let d = Dialog::new_usage(view);
    let state = d.usage_state().expect("usage state");
    assert!(
        state
            .rows()
            .iter()
            .any(|row| row.value().contains("Refreshing") || row.value().contains("refreshing")),
        "refreshing placeholder must render loading copy: {state:?}"
    );
}

#[test]
fn s8_usage_long_unicode_labels_render() {
    let mut view = usage_view_fixture();
    view.account.account_label = format!("work-巴黎-🚀-memo{}", "·很长的账户备注".repeat(6));
    let text = render_usage_dialog_snapshot_for_view(100, 32, UsageDialogTab::Provider, view);
    assert!(text.contains("Usage"), "{text}");
    assert!(text.contains("🚀"), "{text}");
    let squeezed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(squeezed.contains("巴黎"), "{text}");
    assert!(squeezed.contains("很长的账户备注"), "{text}");
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
            text.contains("Updated now"),
            "activity lost at {width}x{height}:\n{text}"
        );
    }
}

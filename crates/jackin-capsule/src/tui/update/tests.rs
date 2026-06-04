//! Tests for `update`.
use super::{
    ActionFramePlan, DialogActionFramePlan, HoverFramePlan, action_frame_plan,
    dialog_action_frame_plan, dialog_change_redraw_reason, drag_resize_ratio,
    drag_resize_redraw_reason, explicit_redraw_reason, first_attach_redraw_reason,
    focus_change_redraw_reason, hover_frame_plan, palette_route_redraw_reason,
    pane_data_redraw_reason, prefix_full_redraw_reason, resize_redraw_reason,
    selection_change_redraw_reason, selection_start_redraw_reason, session_exit_redraw_reason,
    status_change_redraw_reason, wheel_scrollback_redraw_reason,
};
use crate::tui::components::dialog::{DialogAction, PickerIntent};
use crate::tui::input::{ArrowDir, PrefixCommand};
use crate::tui::layout::{Rect, SplitOrient};
use crate::tui::message::{Action, PaletteCommandRoute};
use crate::tui::update::FullRedrawReason;

#[test]
fn prefix_commands_map_to_visible_redraw_reasons() {
    assert_eq!(
        prefix_full_redraw_reason(&PrefixCommand::NewTab),
        FullRedrawReason::PaletteOverlay
    );
    assert_eq!(
        prefix_full_redraw_reason(&PrefixCommand::MoveFocus(ArrowDir::Right)),
        FullRedrawReason::FocusChange
    );
    assert_eq!(
        prefix_full_redraw_reason(&PrefixCommand::Detach),
        FullRedrawReason::ExplicitRedraw
    );
}

#[test]
fn hover_frame_plan_uses_overlay_when_dialog_owns_screen() {
    assert_eq!(
        hover_frame_plan(true),
        HoverFramePlan::DialogOverlay(FullRedrawReason::DialogChange)
    );
    assert_eq!(hover_frame_plan(false), HoverFramePlan::ChromeHover);
}

#[test]
fn dialog_action_frame_plan_keeps_copy_feedback_overlay_scoped() {
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::CopyToClipboard("id".into())),
        DialogActionFramePlan::Overlay(FullRedrawReason::DialogChange)
    );
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::SpawnAgent {
            agent: None,
            intent: PickerIntent::NewTab,
        }),
        DialogActionFramePlan::Full(FullRedrawReason::DialogChange)
    );
}

#[test]
fn direct_actions_map_to_visible_frame_plans() {
    assert_eq!(
        action_frame_plan(&Action::OpenContainerInfo),
        Some(ActionFramePlan::Overlay(FullRedrawReason::DialogChange))
    );
    assert_eq!(
        action_frame_plan(&Action::OpenPalette),
        Some(ActionFramePlan::Full(FullRedrawReason::PaletteOverlay))
    );
    assert_eq!(
        action_frame_plan(&Action::NextTab),
        Some(ActionFramePlan::Full(FullRedrawReason::TabSwitch))
    );
    assert_eq!(
        action_frame_plan(&Action::ClearFocusedPane),
        Some(ActionFramePlan::Full(FullRedrawReason::PaneClear))
    );
    assert_eq!(
        action_frame_plan(&Action::Palette(
            crate::tui::components::dialog::PaletteCommand::ClearPane
        )),
        None
    );
}

#[test]
fn drag_resize_ratio_clamps_to_visible_resize_bounds() {
    let rect = Rect::new(2, 4, 20, 100);
    assert_eq!(drag_resize_ratio(SplitOrient::Horizontal, rect, 2, 0), 0.05);
    assert_eq!(
        drag_resize_ratio(SplitOrient::Horizontal, rect, 2, 200),
        0.95
    );
    assert_eq!(drag_resize_ratio(SplitOrient::Horizontal, rect, 2, 54), 0.5);

    let rect = Rect::new(2, 4, 20, 100);
    assert_eq!(drag_resize_ratio(SplitOrient::Vertical, rect, 0, 4), 0.05);
    assert_eq!(drag_resize_ratio(SplitOrient::Vertical, rect, 40, 4), 0.95);
    assert_eq!(drag_resize_ratio(SplitOrient::Vertical, rect, 12, 4), 0.5);
}

#[test]
fn pane_data_redraw_reason_prioritizes_scrollback_snap() {
    assert_eq!(
        pane_data_redraw_reason(true, true),
        Some(FullRedrawReason::ScrollbackMovement)
    );
    assert_eq!(
        pane_data_redraw_reason(false, true),
        Some(FullRedrawReason::ExplicitRedraw)
    );
    assert_eq!(pane_data_redraw_reason(false, false), None);
}

#[test]
fn selection_start_redraw_reason_only_repaints_when_selection_begins() {
    assert_eq!(
        selection_start_redraw_reason(true),
        Some(FullRedrawReason::SelectionRepaint)
    );
    assert_eq!(selection_start_redraw_reason(false), None);
}

#[test]
fn focus_change_redraw_reason_only_repaints_when_focus_changes() {
    assert_eq!(
        focus_change_redraw_reason(true),
        Some(FullRedrawReason::FocusChange)
    );
    assert_eq!(focus_change_redraw_reason(false), None);
}

#[test]
fn drag_and_selection_redraw_reasons_use_visible_update_vocabulary() {
    assert_eq!(drag_resize_redraw_reason(), FullRedrawReason::LayoutChange);
    assert_eq!(
        selection_change_redraw_reason(),
        FullRedrawReason::SelectionRepaint
    );
}

#[test]
fn lifecycle_redraw_reasons_use_visible_update_vocabulary() {
    assert_eq!(first_attach_redraw_reason(), FullRedrawReason::FirstAttach);
    assert_eq!(resize_redraw_reason(), FullRedrawReason::Resize);
    assert_eq!(session_exit_redraw_reason(), FullRedrawReason::SessionExit);
    assert_eq!(
        status_change_redraw_reason(),
        FullRedrawReason::StatusChange
    );
    assert_eq!(
        dialog_change_redraw_reason(),
        FullRedrawReason::DialogChange
    );
    assert_eq!(explicit_redraw_reason(), FullRedrawReason::ExplicitRedraw);
}

#[test]
fn wheel_redraw_reason_uses_visible_update_vocabulary() {
    assert_eq!(
        wheel_scrollback_redraw_reason(),
        FullRedrawReason::ScrollbackMovement
    );
}

#[test]
fn palette_route_redraw_reason_only_repaints_terminal_actions() {
    assert_eq!(
        palette_route_redraw_reason(PaletteCommandRoute::ClearPane),
        Some(FullRedrawReason::PaneClear)
    );
    assert_eq!(
        palette_route_redraw_reason(PaletteCommandRoute::NextTab),
        None
    );
}

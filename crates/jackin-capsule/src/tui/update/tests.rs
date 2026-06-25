//! Tests for `update`.
use super::{
    ActionFramePlan, DialogActionFramePlan, HoverFramePlan, action_frame_plan,
    dialog_action_frame_plan, dialog_change_redraw_reason, drag_resize_ratio,
    drag_resize_redraw_reason, explicit_redraw_reason, first_attach_redraw_reason,
    focus_change_redraw_reason, hover_frame_plan, palette_route_frame_plan,
    pane_data_redraw_reason, prefix_full_redraw_reason, selection_change_redraw_reason,
    selection_start_redraw_reason, session_exit_redraw_reason, status_change_redraw_reason,
    wheel_scrollback_redraw_reason,
};
use crate::tui::components::dialog::{ConfirmKind, DialogAction, PickerIntent, SplitDirection};
use crate::tui::components::palette::PaletteCommand;
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
        dialog_action_frame_plan(&DialogAction::Dismiss),
        DialogActionFramePlan::Overlay(FullRedrawReason::DialogChange)
    );
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::Redraw),
        DialogActionFramePlan::Overlay(FullRedrawReason::DialogChange)
    );
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::Consume),
        DialogActionFramePlan::Overlay(FullRedrawReason::DialogChange)
    );
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::SplitDirection(SplitDirection::Right)),
        DialogActionFramePlan::Overlay(FullRedrawReason::DialogChange)
    );
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::PickedCloseTarget(ConfirmKind::ClosePane)),
        DialogActionFramePlan::Overlay(FullRedrawReason::DialogChange)
    );
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::RenameTab {
            tab_idx: 0,
            label: "work".into(),
        }),
        DialogActionFramePlan::Overlay(FullRedrawReason::DialogChange)
    );
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::Command(PaletteCommand::NewTab)),
        DialogActionFramePlan::Overlay(FullRedrawReason::DialogChange)
    );
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::SpawnAgent {
            agent: None,
            intent: PickerIntent::NewTab,
        }),
        DialogActionFramePlan::Full(FullRedrawReason::TabSwitch)
    );
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::SpawnAgent {
            agent: Some("claude".into()),
            intent: PickerIntent::Split(SplitDirection::Right),
        }),
        DialogActionFramePlan::Full(FullRedrawReason::LayoutChange)
    );
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::SpawnAgentWithProvider {
            agent: Some("claude".into()),
            provider_label: "Z.AI".into(),
            intent: PickerIntent::NewTab,
        }),
        DialogActionFramePlan::Full(FullRedrawReason::TabSwitch)
    );
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::ConfirmedAction(ConfirmKind::ClosePane)),
        DialogActionFramePlan::Full(FullRedrawReason::SplitClose)
    );
    assert_eq!(
        dialog_action_frame_plan(&DialogAction::ConfirmedAction(ConfirmKind::Exit)),
        DialogActionFramePlan::Full(FullRedrawReason::SessionExit)
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
        Some(ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay))
    );
    assert_eq!(
        action_frame_plan(&Action::NextTab),
        Some(ActionFramePlan::Full(FullRedrawReason::TabSwitch))
    );
    assert_eq!(
        action_frame_plan(&Action::MoveFocus(ArrowDir::Right)),
        Some(ActionFramePlan::Diff(FullRedrawReason::FocusChange))
    );
    assert_eq!(
        action_frame_plan(&Action::ClearFocusedPane),
        Some(ActionFramePlan::Diff(FullRedrawReason::PaneClear))
    );
    assert_eq!(
        action_frame_plan(&Action::Palette(PaletteCommand::ClearPane)),
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
fn palette_routes_map_to_visible_frame_plans() {
    assert_eq!(
        palette_route_frame_plan(PaletteCommandRoute::OpenSplitDirectionPicker),
        ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay)
    );
    assert_eq!(
        palette_route_frame_plan(PaletteCommandRoute::OpenAgentPicker(PickerIntent::NewTab)),
        ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay)
    );
    assert_eq!(
        palette_route_frame_plan(PaletteCommandRoute::ConfirmAction(ConfirmKind::CloseTab)),
        ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay)
    );
    assert_eq!(
        palette_route_frame_plan(PaletteCommandRoute::OpenCloseTargetPicker),
        ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay)
    );
    assert_eq!(
        palette_route_frame_plan(PaletteCommandRoute::NextTab),
        ActionFramePlan::Full(FullRedrawReason::TabSwitch)
    );
    assert_eq!(
        palette_route_frame_plan(PaletteCommandRoute::PreviousTab),
        ActionFramePlan::Full(FullRedrawReason::TabSwitch)
    );
    assert_eq!(
        palette_route_frame_plan(PaletteCommandRoute::ToggleZoom),
        ActionFramePlan::Full(FullRedrawReason::ZoomChange)
    );
    assert_eq!(
        palette_route_frame_plan(PaletteCommandRoute::StageImageFromClipboardPath),
        ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay)
    );
    assert_eq!(
        palette_route_frame_plan(PaletteCommandRoute::StageImageFromClipboard),
        ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay)
    );
    assert_eq!(
        palette_route_frame_plan(PaletteCommandRoute::ExportFileUnderCursor {
            reveal_after_export: true,
            open_after_export: false,
        }),
        ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay)
    );
    assert_eq!(
        palette_route_frame_plan(PaletteCommandRoute::ExportSelectedFile {
            reveal_after_export: false,
            open_after_export: true,
        }),
        ActionFramePlan::Overlay(FullRedrawReason::PaletteOverlay)
    );
    assert_eq!(
        palette_route_frame_plan(PaletteCommandRoute::ClearPane),
        ActionFramePlan::Diff(FullRedrawReason::PaneClear)
    );
}

#[test]
fn frame_plans_keep_diff_tier_reasons_out_of_full_redraws() {
    let direct_action_plans = [
        action_frame_plan(&Action::OpenPalette).expect("open palette should redraw"),
        action_frame_plan(&Action::OpenContainerInfo).expect("container info should redraw"),
        action_frame_plan(&Action::OpenGithubContext).expect("github context should redraw"),
        action_frame_plan(&Action::OpenRenameTab(0)).expect("rename tab should redraw"),
        action_frame_plan(&Action::OpenAgentPicker(PickerIntent::NewTab))
            .expect("agent picker should redraw"),
        action_frame_plan(&Action::SwitchTab(0)).expect("switch tab should redraw"),
        action_frame_plan(&Action::NextTab).expect("next tab should redraw"),
        action_frame_plan(&Action::PreviousTab).expect("previous tab should redraw"),
        action_frame_plan(&Action::JumpTab(0)).expect("jump tab should redraw"),
        action_frame_plan(&Action::SplitFocused(SplitDirection::Right))
            .expect("split should redraw"),
        action_frame_plan(&Action::ResizePane(ArrowDir::Right)).expect("resize should redraw"),
        action_frame_plan(&Action::MoveFocus(ArrowDir::Right)).expect("focus should redraw"),
        action_frame_plan(&Action::ToggleZoom).expect("zoom should redraw"),
        action_frame_plan(&Action::CloseFocusedPane).expect("close pane should redraw"),
        action_frame_plan(&Action::CloseFocusedTab).expect("close tab should redraw"),
        action_frame_plan(&Action::ClearFocusedPane).expect("clear pane should redraw"),
        action_frame_plan(&Action::Detach).expect("detach should redraw"),
    ];
    for plan in direct_action_plans {
        assert_action_frame_plan_avoids_full_diff_tier(plan);
    }

    let palette_plans = [
        palette_route_frame_plan(PaletteCommandRoute::OpenSplitDirectionPicker),
        palette_route_frame_plan(PaletteCommandRoute::OpenAgentPicker(PickerIntent::NewTab)),
        palette_route_frame_plan(PaletteCommandRoute::ConfirmAction(ConfirmKind::CloseTab)),
        palette_route_frame_plan(PaletteCommandRoute::OpenCloseTargetPicker),
        palette_route_frame_plan(PaletteCommandRoute::OpenExportFileDialog {
            reveal_after_export: false,
            open_after_export: false,
        }),
        palette_route_frame_plan(PaletteCommandRoute::OpenExportFileDialog {
            reveal_after_export: false,
            open_after_export: true,
        }),
        palette_route_frame_plan(PaletteCommandRoute::NextTab),
        palette_route_frame_plan(PaletteCommandRoute::PreviousTab),
        palette_route_frame_plan(PaletteCommandRoute::ToggleZoom),
        palette_route_frame_plan(PaletteCommandRoute::StageImageFromClipboardPath),
        palette_route_frame_plan(PaletteCommandRoute::PasteImageFromClipboard),
        palette_route_frame_plan(PaletteCommandRoute::StageImageFromClipboard),
        palette_route_frame_plan(PaletteCommandRoute::ExportFileUnderCursor {
            reveal_after_export: true,
            open_after_export: false,
        }),
        palette_route_frame_plan(PaletteCommandRoute::ExportSelectedFile {
            reveal_after_export: false,
            open_after_export: true,
        }),
        palette_route_frame_plan(PaletteCommandRoute::OpenLinkUnderCursor),
        palette_route_frame_plan(PaletteCommandRoute::ClearPane),
    ];
    for plan in palette_plans {
        assert_action_frame_plan_avoids_full_diff_tier(plan);
    }

    let dialog_actions = [
        DialogAction::Command(PaletteCommand::NewTab),
        DialogAction::SplitDirection(SplitDirection::Right),
        DialogAction::PickedCloseTarget(ConfirmKind::ClosePane),
        DialogAction::ConfirmedAction(ConfirmKind::ClosePane),
        DialogAction::ConfirmedAction(ConfirmKind::CloseTab),
        DialogAction::ConfirmedAction(ConfirmKind::Exit),
        DialogAction::SpawnAgent {
            agent: None,
            intent: PickerIntent::NewTab,
        },
        DialogAction::SpawnAgent {
            agent: Some("claude".into()),
            intent: PickerIntent::Split(SplitDirection::Right),
        },
        DialogAction::SpawnAgentWithProvider {
            agent: Some("claude".into()),
            provider_label: "Z.AI".into(),
            intent: PickerIntent::NewTab,
        },
        DialogAction::RenameTab {
            tab_idx: 0,
            label: "work".into(),
        },
        DialogAction::ExportFile {
            path: "target/report.txt".into(),
            reveal_after_export: false,
            open_after_export: false,
        },
        DialogAction::CopyToClipboard("container".into()),
        DialogAction::OpenHostUrl("https://github.com/jackin-project/jackin/pull/565".into()),
        DialogAction::RevealHostPath(
            "/Users/operator/.jackin/data/diagnostics/runs/jk-run-abc123.jsonl".into(),
        ),
        DialogAction::SwitchUsageProvider {
            provider_label: "Claude".into(),
        },
        DialogAction::Dismiss,
        DialogAction::Redraw,
        DialogAction::Consume,
    ];
    for action in dialog_actions {
        assert_dialog_frame_plan_avoids_full_diff_tier(dialog_action_frame_plan(&action));
    }
}

fn assert_action_frame_plan_avoids_full_diff_tier(plan: ActionFramePlan) {
    if let ActionFramePlan::Full(reason) = plan {
        assert_clear_tier_reason(reason);
    }
}

fn assert_dialog_frame_plan_avoids_full_diff_tier(plan: DialogActionFramePlan) {
    if let DialogActionFramePlan::Full(reason) = plan {
        assert_clear_tier_reason(reason);
    }
}

fn assert_clear_tier_reason(reason: FullRedrawReason) {
    assert!(
        matches!(
            reason,
            FullRedrawReason::FirstAttach
                | FullRedrawReason::Resize
                | FullRedrawReason::TabSwitch
                | FullRedrawReason::LayoutChange
                | FullRedrawReason::SplitClose
                | FullRedrawReason::ZoomChange
                | FullRedrawReason::SessionExit
                | FullRedrawReason::ExplicitRedraw
        ),
        "{reason:?} must not route through a full clear redraw"
    );
}

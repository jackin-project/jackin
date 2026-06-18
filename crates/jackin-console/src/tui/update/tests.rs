//! Tests for `update`.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn term_width_scroll_plan_updates_and_clamps_offset() {
    assert_eq!(term_width_scroll_plan(0, 8, 10, 40), 8);
    assert_eq!(term_width_scroll_plan(8, -99, 10, 40), 0);
}

#[test]
fn selection_move_plan_clamps_to_rows() {
    assert_eq!(selection_move_plan(0, 3, 99), 2);
    assert_eq!(selection_move_plan(2, 3, -99), 0);
}

#[test]
fn selected_index_plan_clamps_to_rows() {
    assert_eq!(selected_index_plan(99, 3), 2);
    assert_eq!(selected_index_plan(0, 0), 0);
}

#[test]
fn unclamped_scroll_plan_updates_without_upper_clamp() {
    assert_eq!(unclamped_scroll_plan(4, 3), 7);
    assert_eq!(unclamped_scroll_plan(4, -99), 0);
}

#[test]
fn status_overlay_plans_construct_open_and_dismiss() {
    let StatusOverlayPlan::Open(state) = open_status_overlay_plan("Title", "Body") else {
        panic!("expected open plan");
    };
    let debug = format!("{state:?}");
    assert!(debug.contains("Title"));
    assert!(debug.contains("Body"));
    assert!(matches!(
        dismiss_status_overlay_plan(),
        StatusOverlayPlan::Dismiss
    ));
}

#[test]
fn inline_picker_dismissal_plan_returns_requested_kind() {
    assert_eq!(
        inline_picker_dismissal_plan(InlinePickerDismissal::Agent),
        InlinePickerDismissal::Agent
    );
}

#[test]
fn shell_state_plans_return_normalized_values() {
    assert_eq!(
        list_scroll_focus_plan(Some(crate::tui::focus::MountScrollFocus::Workspace)),
        Some(crate::tui::focus::MountScrollFocus::Workspace)
    );
    assert!(list_names_focus_plan(true));
    let drag = crate::tui::split::DragState {
        anchor_pct: 30,
        anchor_x: 12,
    };
    assert_eq!(drag_state_plan(Some(drag)), Some(drag));
    assert_eq!(list_split_pct_plan(1), crate::tui::split::MIN_SPLIT_PCT);
    assert_eq!(list_split_pct_plan(99), crate::tui::split::MAX_SPLIT_PCT);
}

#[test]
fn list_pre_render_focus_plan_handles_sidebar_liveness() {
    let missing_sidebar = list_pre_render_focus_plan(
        Some(crate::tui::focus::MountScrollFocus::Workspace),
        false,
        false,
        false,
        false,
    );
    assert_eq!(missing_sidebar.list_scroll_focus, None);
    assert!(missing_sidebar.list_names_focused);

    let preview_missing_sidebar = list_pre_render_focus_plan(
        Some(crate::tui::focus::MountScrollFocus::Workspace),
        false,
        true,
        false,
        false,
    );
    assert_eq!(preview_missing_sidebar.list_scroll_focus, None);
    assert!(!preview_missing_sidebar.list_names_focused);

    let stale_focus = list_pre_render_focus_plan(
        Some(crate::tui::focus::MountScrollFocus::Workspace),
        false,
        true,
        true,
        false,
    );
    assert_eq!(stale_focus.list_scroll_focus, None);
    assert!(stale_focus.list_names_focused);

    let live_focus = list_pre_render_focus_plan(
        Some(crate::tui::focus::MountScrollFocus::Workspace),
        false,
        false,
        true,
        true,
    );
    assert_eq!(
        live_focus.list_scroll_focus,
        Some(crate::tui::focus::MountScrollFocus::Workspace)
    );
    assert!(!live_focus.list_names_focused);
}

#[test]
fn list_pre_render_scroll_reset_plan_resets_missing_scroll_slots() {
    assert_eq!(
        list_pre_render_scroll_reset_plan(false, false, false),
        ListPreRenderScrollResetPlan {
            reset_workspace: true,
            reset_global: true,
            reset_role_global: true,
            reset_roles: true,
        }
    );
    assert_eq!(
        list_pre_render_scroll_reset_plan(true, false, true),
        ListPreRenderScrollResetPlan {
            reset_workspace: false,
            reset_global: false,
            reset_role_global: true,
            reset_roles: false,
        }
    );
    assert_eq!(
        list_pre_render_scroll_reset_plan(true, true, false),
        ListPreRenderScrollResetPlan {
            reset_workspace: false,
            reset_global: false,
            reset_role_global: false,
            reset_roles: true,
        }
    );
}

#[test]
fn inline_provider_followup_plan_opens_picker_only_when_supported() {
    assert_eq!(
        inline_provider_followup_plan("container", "claude", vec!["anthropic", "zai"]),
        InlineProviderFollowupPlan::OpenProviderPicker(ProviderPickerState::new(
            "container",
            "claude",
            vec!["anthropic", "zai"]
        ))
    );
    // Codex with two providers opens the picker.
    assert_eq!(
        inline_provider_followup_plan("container", "codex", vec!["openai", "minimax"]),
        InlineProviderFollowupPlan::OpenProviderPicker(ProviderPickerState::new(
            "container",
            "codex",
            vec!["openai", "minimax"]
        ))
    );
    // Single-provider choice collapses to a direct start.
    assert_eq!(
        inline_provider_followup_plan("container", "codex", vec!["openai"]),
        InlineProviderFollowupPlan::StartSession {
            context: "container",
            agent: "codex",
        }
    );
    assert_eq!(
        inline_provider_followup_plan::<_, _, &str>("container", "claude", Vec::new()),
        InlineProviderFollowupPlan::StartSession {
            context: "container",
            agent: "claude",
        }
    );
}

#[test]
fn inline_picker_shell_plan_routes_scroll_exit_and_delegate() {
    assert_eq!(
        inline_picker_shell_plan(key(KeyCode::Left), true),
        InlinePickerShellPlan::ScrollHorizontal(-8)
    );
    assert_eq!(
        inline_picker_shell_plan(key(KeyCode::Char('l')), true),
        InlinePickerShellPlan::ScrollHorizontal(8)
    );
    assert_eq!(
        inline_picker_shell_plan(key(KeyCode::Char('q')), true),
        InlinePickerShellPlan::Exit
    );
    assert_eq!(
        inline_picker_shell_plan(key(KeyCode::Char('q')), false),
        InlinePickerShellPlan::Delegate
    );
    assert_eq!(
        inline_picker_shell_plan(key(KeyCode::Enter), true),
        InlinePickerShellPlan::Delegate
    );
}

#[test]
fn inline_picker_plan_routes_modal_outcomes() {
    assert_eq!(
        inline_picker_plan(jackin_tui::ModalOutcome::Commit("agent-smith")),
        InlinePickerPlan::Commit("agent-smith")
    );
    assert_eq!(
        inline_picker_plan::<&str>(jackin_tui::ModalOutcome::Cancel),
        InlinePickerPlan::Dismiss
    );
    assert_eq!(
        inline_picker_plan::<&str>(jackin_tui::ModalOutcome::Continue),
        InlinePickerPlan::Continue
    );
}

#[test]
fn file_browser_modal_plan_routes_browser_outcomes() {
    use crate::tui::components::file_browser::FileBrowserOutcome;
    use std::path::PathBuf;

    assert_eq!(
        file_browser_modal_plan::<PathBuf>(FileBrowserOutcome::Cancel),
        FileBrowserModalPlan::Dismiss
    );
    assert_eq!(
        file_browser_modal_plan::<PathBuf>(FileBrowserOutcome::ResolveGitUrl(PathBuf::from(
            "/tmp/repo"
        ))),
        FileBrowserModalPlan::ResolveGitUrl(PathBuf::from("/tmp/repo"))
    );
    assert_eq!(
        file_browser_modal_plan::<PathBuf>(FileBrowserOutcome::OpenGitUrl(
            "file:///tmp/repo".to_owned()
        )),
        FileBrowserModalPlan::OpenUrl("file:///tmp/repo".to_owned())
    );
    assert_eq!(
        file_browser_modal_plan::<PathBuf>(FileBrowserOutcome::Continue),
        FileBrowserModalPlan::Continue
    );
    assert_eq!(
        file_browser_modal_plan(FileBrowserOutcome::<PathBuf>::NavigateTo(PathBuf::from(
            "/tmp/repo"
        ))),
        FileBrowserModalPlan::ApplyFileBrowserOutcome(FileBrowserOutcome::NavigateTo(
            PathBuf::from("/tmp/repo")
        ))
    );
}

#[test]
fn mount_dst_choice_plan_routes_choice_outcomes() {
    use crate::tui::components::mount_dst_choice::MountDstChoice;

    assert_eq!(
        mount_dst_choice_plan(jackin_tui::ModalOutcome::Commit(MountDstChoice::SamePath)),
        MountDstChoicePlan::CommitSamePath
    );
    assert_eq!(
        mount_dst_choice_plan(jackin_tui::ModalOutcome::Commit(MountDstChoice::Edit)),
        MountDstChoicePlan::OpenEditInput
    );
    assert_eq!(
        mount_dst_choice_plan(jackin_tui::ModalOutcome::Cancel),
        MountDstChoicePlan::Dismiss
    );
    assert_eq!(
        mount_dst_choice_plan(jackin_tui::ModalOutcome::Continue),
        MountDstChoicePlan::Continue
    );
}

#[test]
fn save_discard_modal_plan_routes_save_discard_outcomes() {
    use jackin_tui::components::SaveDiscardChoice;

    assert_eq!(
        save_discard_modal_plan(jackin_tui::ModalOutcome::Commit(SaveDiscardChoice::Save)),
        SaveDiscardModalPlan::Save
    );
    assert_eq!(
        save_discard_modal_plan(jackin_tui::ModalOutcome::Commit(SaveDiscardChoice::Discard)),
        SaveDiscardModalPlan::Discard
    );
    assert_eq!(
        save_discard_modal_plan(jackin_tui::ModalOutcome::Cancel),
        SaveDiscardModalPlan::Dismiss
    );
    assert_eq!(
        save_discard_modal_plan(jackin_tui::ModalOutcome::Continue),
        SaveDiscardModalPlan::Continue
    );
}

#[test]
fn confirm_save_modal_plan_routes_confirm_outcomes() {
    use crate::tui::components::confirm_save::SaveChoice;

    assert_eq!(
        confirm_save_modal_plan(jackin_tui::ModalOutcome::Commit(SaveChoice::Save)),
        ConfirmSaveModalPlan::Commit
    );
    assert_eq!(
        confirm_save_modal_plan(jackin_tui::ModalOutcome::Cancel),
        ConfirmSaveModalPlan::Dismiss
    );
    assert_eq!(
        confirm_save_modal_plan(jackin_tui::ModalOutcome::Continue),
        ConfirmSaveModalPlan::Continue
    );
}

#[test]
fn list_github_picker_plan_routes_picker_outcomes() {
    assert_eq!(
        list_github_picker_plan(jackin_tui::ModalOutcome::Commit(
            "https://github.com/jackin-project/jackin".to_owned()
        )),
        ListGithubPickerPlan::OpenUrl("https://github.com/jackin-project/jackin".to_owned())
    );
    assert_eq!(
        list_github_picker_plan(jackin_tui::ModalOutcome::Cancel),
        ListGithubPickerPlan::Dismiss
    );
    assert_eq!(
        list_github_picker_plan(jackin_tui::ModalOutcome::Continue),
        ListGithubPickerPlan::Continue
    );
}

#[test]
fn list_role_picker_plan_routes_picker_outcomes() {
    assert_eq!(
        list_role_picker_plan(jackin_tui::ModalOutcome::Commit("agent-smith")),
        ListRolePickerPlan::Launch("agent-smith")
    );
    assert_eq!(
        list_role_picker_plan::<&str>(jackin_tui::ModalOutcome::Cancel),
        ListRolePickerPlan::Dismiss
    );
    assert_eq!(
        list_role_picker_plan::<&str>(jackin_tui::ModalOutcome::Continue),
        ListRolePickerPlan::Continue
    );
}

#[test]
fn dismissible_modal_plan_dismisses_commit_and_cancel() {
    assert_eq!(
        dismissible_modal_plan(jackin_tui::ModalOutcome::Commit(())),
        DismissibleModalPlan::Dismiss
    );
    assert_eq!(
        dismissible_modal_plan::<()>(jackin_tui::ModalOutcome::Cancel),
        DismissibleModalPlan::Dismiss
    );
    assert_eq!(
        dismissible_modal_plan::<()>(jackin_tui::ModalOutcome::Continue),
        DismissibleModalPlan::Continue
    );
}

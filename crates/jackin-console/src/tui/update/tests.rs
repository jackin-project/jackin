// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `update`.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

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
fn role_resolution_status_overlay_plan_names_role() {
    let StatusOverlayPlan::Open(state) = role_resolution_status_overlay_plan("agent-smith") else {
        panic!("expected open plan");
    };
    let debug = format!("{state:?}");
    assert!(debug.contains("Resolving agent role"));
    assert!(debug.contains("agent-smith"));
}

#[derive(Default)]
struct TestStatusOverlay {
    overlay: Option<crate::tui::components::StatusPopupState>,
}

impl StatusOverlayState for TestStatusOverlay {
    fn set_status_overlay(&mut self, overlay: Option<crate::tui::components::StatusPopupState>) {
        self.overlay = overlay;
    }
}

#[test]
fn apply_status_overlay_plan_opens_and_dismisses() {
    let mut state = TestStatusOverlay::default();

    apply_status_overlay_plan(&mut state, open_status_overlay_plan("Title", "Body"));
    assert!(state.overlay.is_some());

    apply_status_overlay_plan(&mut state, dismiss_status_overlay_plan());
    assert!(state.overlay.is_none());
}

#[derive(Default)]
struct TestListModal {
    opened: Option<&'static str>,
}

impl ListModalState for TestListModal {
    fn open_container_info_modal(
        &mut self,
        _state: crate::tui::components::container_info_surface::ContainerInfoState,
    ) {
        self.opened = Some("container-info");
    }

    fn open_error_popup_modal(&mut self, _state: crate::tui::components::ErrorPopupState) {
        self.opened = Some("error-popup");
    }

    fn open_github_picker_modal(
        &mut self,
        _state: crate::tui::components::github_picker::GithubPickerState,
    ) {
        self.opened = Some("github-picker");
    }

    fn dismiss_list_modal(&mut self) {
        self.opened = None;
    }
}

#[test]
fn apply_list_modal_plan_routes_modal_storage() {
    let mut state = TestListModal::default();

    apply_list_modal_plan(
        &mut state,
        open_container_info_modal_plan(
            crate::tui::components::container_info_surface::ContainerInfoState::new(
                "title",
                vec![
                    crate::tui::components::container_info_surface::ContainerInfoRow::new(
                        "label", "value",
                    ),
                ],
            ),
        ),
    );
    assert_eq!(state.opened, Some("container-info"));

    apply_list_modal_plan(
        &mut state,
        open_error_popup_modal_plan("Error title", "Error body"),
    );
    assert_eq!(state.opened, Some("error-popup"));

    apply_list_modal_plan(&mut state, dismiss_list_modal_plan());
    assert_eq!(state.opened, None);
}

#[test]
fn inline_picker_dismissal_plan_returns_requested_kind() {
    assert_eq!(
        inline_picker_dismissal_plan(InlinePickerDismissal::Agent),
        InlinePickerDismissal::Agent
    );
}

#[derive(Default)]
struct TestInlinePickers {
    cleared: Vec<&'static str>,
}

impl InlinePickerDismissalState for TestInlinePickers {
    fn clear_inline_new_session_picker(&mut self) {
        self.cleared.push("new-session");
    }

    fn clear_inline_role_picker(&mut self) {
        self.cleared.push("role");
    }

    fn clear_inline_agent_picker(&mut self) {
        self.cleared.push("agent");
    }

    fn clear_inline_provider_picker(&mut self) {
        self.cleared.push("provider");
    }

    fn clear_launch_provider_picker(&mut self) {
        self.cleared.push("launch-provider");
    }
}

#[test]
fn apply_inline_picker_dismissal_plan_clears_requested_picker() {
    let mut state = TestInlinePickers::default();

    for dismissal in [
        InlinePickerDismissal::NewSession,
        InlinePickerDismissal::Role,
        InlinePickerDismissal::Agent,
        InlinePickerDismissal::Provider,
        InlinePickerDismissal::LaunchProvider,
    ] {
        apply_inline_picker_dismissal_plan(&mut state, dismissal);
    }

    assert_eq!(
        state.cleared,
        [
            "new-session",
            "role",
            "agent",
            "provider",
            "launch-provider",
        ]
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

#[derive(Default)]
struct TestListShell {
    drag: Option<crate::tui::split::DragState>,
    split_pct: u16,
}

impl ListShellState for TestListShell {
    fn set_drag_state(&mut self, drag: Option<crate::tui::split::DragState>) {
        self.drag = drag;
    }

    fn set_list_split_pct(&mut self, pct: u16) {
        self.split_pct = pct;
    }
}

#[test]
fn shell_state_plan_application_updates_storage() {
    let mut state = TestListShell::default();
    let drag = crate::tui::split::DragState {
        anchor_pct: 30,
        anchor_x: 12,
    };

    apply_drag_state_plan(&mut state, drag_state_plan(Some(drag)));
    assert_eq!(state.drag, Some(drag));

    apply_drag_state_plan(&mut state, drag_state_plan(None));
    assert_eq!(state.drag, None);

    apply_list_split_pct_plan(&mut state, list_split_pct_plan(99));
    assert_eq!(state.split_pct, crate::tui::split::MAX_SPLIT_PCT);
}

#[test]
fn modal_scroll_targets_route_by_modal_facts() {
    assert_eq!(
        list_modal_key_target(true, true, true, true),
        ListModalKeyTarget::GithubPicker
    );
    assert_eq!(
        list_modal_key_target(false, true, true, true),
        ListModalKeyTarget::RolePicker
    );
    assert_eq!(
        list_modal_key_target(false, false, true, true),
        ListModalKeyTarget::ErrorPopup
    );
    assert_eq!(
        list_modal_key_target(false, false, false, true),
        ListModalKeyTarget::ContainerInfo
    );
    assert_eq!(
        list_modal_key_target(false, false, false, false),
        ListModalKeyTarget::Dismiss
    );

    assert_eq!(
        list_modal_scroll_target(true, true, true),
        ListModalScrollTarget::GithubPicker
    );
    assert_eq!(
        list_modal_scroll_target(false, true, true),
        ListModalScrollTarget::RolePicker
    );
    assert_eq!(
        list_modal_scroll_target(false, false, true),
        ListModalScrollTarget::OpPicker
    );
    assert_eq!(
        list_modal_scroll_target(false, false, false),
        ListModalScrollTarget::None
    );

    assert_eq!(
        shared_modal_scroll_target(true, true, true, true, true),
        SharedModalScrollTarget::WorkdirPick
    );
    assert_eq!(
        shared_modal_scroll_target(false, false, true, false, true),
        SharedModalScrollTarget::RolePicker
    );
    assert_eq!(
        shared_modal_scroll_target(false, false, false, false, true),
        SharedModalScrollTarget::OpPicker
    );
    assert_eq!(
        shared_modal_scroll_target(false, false, false, false, false),
        SharedModalScrollTarget::None
    );

    assert_eq!(
        settings_env_modal_scroll_target(true, true),
        SettingsModalScrollTarget::EnvOpPicker
    );
    assert_eq!(
        settings_env_modal_scroll_target(false, true),
        SettingsModalScrollTarget::EnvRolePicker
    );
    assert_eq!(
        settings_auth_modal_scroll_target(true),
        SettingsModalScrollTarget::AuthOpPicker
    );
    assert_eq!(
        global_mount_modal_scroll_target(true),
        SettingsModalScrollTarget::MountRolePicker
    );
}

#[test]
fn console_mouse_wheel_plan_routes_native_axes_and_shift_fallback() {
    assert_eq!(
        console_mouse_wheel_plan(MouseEventKind::ScrollDown, KeyModifiers::NONE),
        ConsoleMouseWheelPlan::Vertical(1)
    );
    assert_eq!(
        console_mouse_wheel_plan(MouseEventKind::ScrollUp, KeyModifiers::NONE),
        ConsoleMouseWheelPlan::Vertical(-1)
    );
    assert_eq!(
        console_mouse_wheel_plan(MouseEventKind::ScrollRight, KeyModifiers::NONE),
        ConsoleMouseWheelPlan::Horizontal {
            delta: 1,
            vertical_fallback: None,
        }
    );
    assert_eq!(
        console_mouse_wheel_plan(MouseEventKind::ScrollDown, KeyModifiers::SHIFT),
        ConsoleMouseWheelPlan::Horizontal {
            delta: 1,
            vertical_fallback: Some(1),
        }
    );
    assert_eq!(
        console_mouse_wheel_plan(MouseEventKind::Moved, KeyModifiers::NONE),
        ConsoleMouseWheelPlan::None
    );
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
fn list_pre_render_plan_combines_scroll_reset_and_focus() {
    let plan = list_pre_render_plan(ListPreRenderFacts {
        list_scroll_focus: Some(crate::tui::focus::MountScrollFocus::Roles),
        list_names_focused: false,
        preview_focused: true,
        sidebar_available: true,
        focused_block_scrollable: false,
        role_global_available: false,
        roles_available: true,
    });

    assert_eq!(
        plan.scroll_reset,
        ListPreRenderScrollResetPlan {
            reset_workspace: false,
            reset_global: false,
            reset_role_global: true,
            reset_roles: false,
        }
    );
    assert_eq!(
        plan.focus,
        ListPreRenderFocusPlan {
            list_scroll_focus: None,
            list_names_focused: true,
        }
    );
}

#[test]
fn list_pre_render_facts_derive_sidebar_availability_from_scroll_areas() {
    use crate::tui::sidebar_layout::{SidebarScrollArea, SidebarScrollAreas};
    use ratatui::layout::Rect;

    let scrollable = SidebarScrollArea {
        area: Rect::new(0, 0, 10, 4),
        content_width: 20,
        content_height: 8,
    };
    let not_scrollable = SidebarScrollArea {
        area: Rect::new(0, 0, 10, 4),
        content_width: 8,
        content_height: 2,
    };
    let areas = SidebarScrollAreas {
        workspace: not_scrollable,
        global: scrollable,
        role_global: None,
        roles: Some(scrollable),
    };

    assert_eq!(
        list_pre_render_facts_from_scroll_areas(
            Some(crate::tui::focus::MountScrollFocus::Workspace),
            false,
            true,
            Some(&areas),
        ),
        ListPreRenderFacts {
            list_scroll_focus: Some(crate::tui::focus::MountScrollFocus::Workspace),
            list_names_focused: false,
            preview_focused: true,
            sidebar_available: true,
            focused_block_scrollable: false,
            role_global_available: false,
            roles_available: true,
        }
    );

    assert_eq!(
        list_pre_render_facts_from_scroll_areas(None, true, false, None),
        ListPreRenderFacts {
            list_scroll_focus: None,
            list_names_focused: true,
            preview_focused: false,
            sidebar_available: false,
            focused_block_scrollable: true,
            role_global_available: false,
            roles_available: false,
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

struct TestInlineNewSessionPicker<C, A: AgentChoice, P> {
    picker: Option<(C, AgentChoiceState<A>, Vec<P>)>,
}

impl<C, A: AgentChoice, P> Default for TestInlineNewSessionPicker<C, A, P> {
    fn default() -> Self {
        Self { picker: None }
    }
}

impl<C, A: AgentChoice, P> InlineNewSessionPickerState<C, A, P>
    for TestInlineNewSessionPicker<C, A, P>
{
    fn set_inline_new_session_picker(
        &mut self,
        context: C,
        picker: AgentChoiceState<A>,
        providers: Vec<P>,
    ) {
        self.picker = Some((context, picker, providers));
    }
}

#[test]
fn inline_new_session_picker_plan_application_opens_picker() {
    let mut state = TestInlineNewSessionPicker::default();
    let picker = AgentChoiceState::with_choices(jackin_core::Agent::ALL.to_vec());

    apply_inline_new_session_picker_plan(&mut state, "container", picker, Vec::<()>::new());
    assert!(state.picker.is_some());
}

#[derive(Default)]
struct TestInlineProviderPicker<C, A, P> {
    picker: Option<ProviderPickerState<C, A, P>>,
}

impl<C, A, P> InlineProviderPickerState<C, A, P> for TestInlineProviderPicker<C, A, P> {
    fn set_inline_provider_picker(&mut self, picker: ProviderPickerState<C, A, P>) {
        self.picker = Some(picker);
    }
}

#[test]
fn inline_provider_picker_plan_application_opens_picker() {
    let mut state = TestInlineProviderPicker::default();
    let picker = ProviderPickerState::new("container", "claude", vec!["anthropic", "zai"]);

    apply_inline_provider_picker_plan(&mut state, picker.clone());
    assert_eq!(state.picker, Some(picker));
}

#[test]
fn inline_picker_shell_plan_routes_scroll_and_delegate() {
    assert_eq!(
        inline_picker_shell_plan(key(KeyCode::Left), false),
        InlinePickerShellPlan::ScrollHorizontal(-8)
    );
    assert_eq!(
        inline_picker_shell_plan(key(KeyCode::Right), false),
        InlinePickerShellPlan::ScrollHorizontal(8)
    );
    assert_eq!(
        inline_picker_shell_plan(key(KeyCode::Char('h')), false),
        InlinePickerShellPlan::ScrollHorizontal(-8)
    );
    assert_eq!(
        inline_picker_shell_plan(key(KeyCode::Char('l')), false),
        InlinePickerShellPlan::ScrollHorizontal(8)
    );
    // q/Q always delegates — exit_on_q is always false in production callers.
    assert_eq!(
        inline_picker_shell_plan(key(KeyCode::Char('q')), false),
        InlinePickerShellPlan::Delegate
    );
    assert_eq!(
        inline_picker_shell_plan(key(KeyCode::Enter), false),
        InlinePickerShellPlan::Delegate
    );
}

#[test]
fn inline_picker_plan_routes_modal_outcomes() {
    assert_eq!(
        inline_picker_plan(jackin_core::ModalOutcome::Commit("agent-smith")),
        InlinePickerPlan::Commit("agent-smith")
    );
    assert_eq!(
        inline_picker_plan::<&str>(jackin_core::ModalOutcome::Cancel),
        InlinePickerPlan::Dismiss
    );
    assert_eq!(
        inline_picker_plan::<&str>(jackin_core::ModalOutcome::Continue),
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
fn auth_source_folder_picker_plan_routes_browser_outcomes() {
    use crate::tui::components::file_browser::FileBrowserOutcome;
    use std::path::PathBuf;

    let path = PathBuf::from("/tmp/auth-source");
    assert_eq!(
        auth_source_folder_picker_plan(FileBrowserOutcome::Commit(path.clone())),
        AuthSourceFolderPickerPlan::Commit(path)
    );
    assert_eq!(
        auth_source_folder_picker_plan::<PathBuf>(FileBrowserOutcome::Cancel),
        AuthSourceFolderPickerPlan::Close
    );
    assert_eq!(
        auth_source_folder_picker_plan::<PathBuf>(FileBrowserOutcome::Continue),
        AuthSourceFolderPickerPlan::KeepModal
    );
    assert_eq!(
        auth_source_folder_picker_plan(FileBrowserOutcome::<PathBuf>::NavigateTo(PathBuf::from(
            "/tmp"
        ))),
        AuthSourceFolderPickerPlan::KeepModal
    );
    assert_eq!(
        auth_source_folder_picker_plan::<PathBuf>(FileBrowserOutcome::NavigateUp),
        AuthSourceFolderPickerPlan::KeepModal
    );
}

#[test]
fn mount_dst_choice_plan_routes_choice_outcomes() {
    use crate::tui::components::mount_dst_choice::MountDstChoice;

    assert_eq!(
        mount_dst_choice_plan(jackin_core::ModalOutcome::Commit(MountDstChoice::SamePath)),
        MountDstChoicePlan::CommitSamePath
    );
    assert_eq!(
        mount_dst_choice_plan(jackin_core::ModalOutcome::Commit(MountDstChoice::Edit)),
        MountDstChoicePlan::OpenEditInput
    );
    assert_eq!(
        mount_dst_choice_plan(jackin_core::ModalOutcome::Cancel),
        MountDstChoicePlan::Dismiss
    );
    assert_eq!(
        mount_dst_choice_plan(jackin_core::ModalOutcome::Continue),
        MountDstChoicePlan::Continue
    );
}

#[test]
fn save_discard_modal_plan_routes_save_discard_outcomes() {
    use crate::tui::components::SaveDiscardChoice;

    assert_eq!(
        save_discard_modal_plan(jackin_core::ModalOutcome::Commit(SaveDiscardChoice::Save)),
        SaveDiscardModalPlan::Save
    );
    assert_eq!(
        save_discard_modal_plan(jackin_core::ModalOutcome::Commit(
            SaveDiscardChoice::Discard
        )),
        SaveDiscardModalPlan::Discard
    );
    assert_eq!(
        save_discard_modal_plan(jackin_core::ModalOutcome::Cancel),
        SaveDiscardModalPlan::Dismiss
    );
    assert_eq!(
        save_discard_modal_plan(jackin_core::ModalOutcome::Continue),
        SaveDiscardModalPlan::Continue
    );
}

#[test]
fn confirm_save_modal_plan_routes_confirm_outcomes() {
    use crate::tui::components::confirm_save::SaveChoice;

    assert_eq!(
        confirm_save_modal_plan(jackin_core::ModalOutcome::Commit(SaveChoice::Save)),
        ConfirmSaveModalPlan::Commit
    );
    assert_eq!(
        confirm_save_modal_plan(jackin_core::ModalOutcome::Cancel),
        ConfirmSaveModalPlan::Dismiss
    );
    assert_eq!(
        confirm_save_modal_plan(jackin_core::ModalOutcome::Continue),
        ConfirmSaveModalPlan::Continue
    );
}

#[test]
fn bool_confirm_modal_plan_routes_confirm_outcomes() {
    assert_eq!(
        bool_confirm_modal_plan(jackin_core::ModalOutcome::Commit(true)),
        BoolConfirmModalPlan::Confirm
    );
    assert_eq!(
        bool_confirm_modal_plan(jackin_core::ModalOutcome::Commit(false)),
        BoolConfirmModalPlan::Dismiss
    );
    assert_eq!(
        bool_confirm_modal_plan(jackin_core::ModalOutcome::Cancel),
        BoolConfirmModalPlan::Dismiss
    );
    assert_eq!(
        bool_confirm_modal_plan(jackin_core::ModalOutcome::Continue),
        BoolConfirmModalPlan::Continue
    );
}

#[test]
fn create_op_picker_plan_routes_create_mode_outcomes() {
    use crate::tui::components::op_picker::OpPickerSelection;

    let new_item = OpPickerSelection::<&str, &str, &str, &str, &str>::NewItem {
        account: Some("acct"),
        vault: "vault",
        item_name: "item".to_owned(),
        section: Some("section".to_owned()),
        field_label: "field".to_owned(),
    };
    assert_eq!(
        create_op_picker_plan(jackin_oppicker::ModalOutcome::Commit(new_item.clone())),
        CreateOpPickerPlan::Commit(new_item)
    );

    let edit_existing = OpPickerSelection::<&str, &str, &str, &str, &str>::EditItemField {
        account: None,
        vault: "vault",
        item: "item",
        section: None,
        field: "field",
    };
    assert_eq!(
        create_op_picker_plan(jackin_oppicker::ModalOutcome::Commit(edit_existing.clone())),
        CreateOpPickerPlan::Commit(edit_existing)
    );

    assert_eq!(
        create_op_picker_plan(jackin_oppicker::ModalOutcome::Commit(OpPickerSelection::<
            &str,
            &str,
            &str,
            &str,
            &str,
        >::Existing(
            "ref"
        ))),
        CreateOpPickerPlan::Dismiss
    );
    assert_eq!(
        create_op_picker_plan::<&str, &str, &str, &str, &str>(
            jackin_oppicker::ModalOutcome::Cancel
        ),
        CreateOpPickerPlan::Dismiss
    );
    assert_eq!(
        create_op_picker_plan::<&str, &str, &str, &str, &str>(
            jackin_oppicker::ModalOutcome::Continue
        ),
        CreateOpPickerPlan::Continue
    );
}

#[test]
fn scope_picker_plan_routes_scope_outcomes() {
    use crate::tui::components::scope_picker::ScopeChoice;

    assert_eq!(
        scope_picker_plan(jackin_core::ModalOutcome::Commit(ScopeChoice::AllAgents)),
        ScopePickerPlan::AllAgents
    );
    assert_eq!(
        scope_picker_plan(jackin_core::ModalOutcome::Commit(
            ScopeChoice::SpecificAgent
        )),
        ScopePickerPlan::SpecificAgent
    );
    assert_eq!(
        scope_picker_plan(jackin_core::ModalOutcome::Cancel),
        ScopePickerPlan::Dismiss
    );
    assert_eq!(
        scope_picker_plan(jackin_core::ModalOutcome::Continue),
        ScopePickerPlan::Continue
    );
}

#[test]
fn source_picker_plan_routes_source_outcomes() {
    use crate::tui::components::source_picker::SourceChoice;

    assert_eq!(
        source_picker_plan(jackin_core::ModalOutcome::Commit(SourceChoice::Plain)),
        SourcePickerPlan::Plain
    );
    assert_eq!(
        source_picker_plan(jackin_core::ModalOutcome::Commit(SourceChoice::Op)),
        SourcePickerPlan::Op
    );
    assert_eq!(
        source_picker_plan(jackin_core::ModalOutcome::Cancel),
        SourcePickerPlan::Dismiss
    );
    assert_eq!(
        source_picker_plan(jackin_core::ModalOutcome::Continue),
        SourcePickerPlan::Continue
    );
}

#[test]
fn list_github_picker_plan_routes_picker_outcomes() {
    assert_eq!(
        list_github_picker_plan(jackin_core::ModalOutcome::Commit(
            "https://github.com/jackin-project/jackin".to_owned()
        )),
        ListGithubPickerPlan::OpenUrl("https://github.com/jackin-project/jackin".to_owned())
    );
    assert_eq!(
        list_github_picker_plan(jackin_core::ModalOutcome::Cancel),
        ListGithubPickerPlan::Dismiss
    );
    assert_eq!(
        list_github_picker_plan(jackin_core::ModalOutcome::Continue),
        ListGithubPickerPlan::Continue
    );
}

#[test]
fn list_role_picker_plan_routes_picker_outcomes() {
    assert_eq!(
        list_role_picker_plan(jackin_core::ModalOutcome::Commit("agent-smith")),
        ListRolePickerPlan::Launch("agent-smith")
    );
    assert_eq!(
        list_role_picker_plan::<&str>(jackin_core::ModalOutcome::Cancel),
        ListRolePickerPlan::Dismiss
    );
    assert_eq!(
        list_role_picker_plan::<&str>(jackin_core::ModalOutcome::Continue),
        ListRolePickerPlan::Continue
    );
}

#[test]
fn dismissible_modal_plan_dismisses_commit_and_cancel() {
    assert_eq!(
        dismissible_modal_plan(jackin_core::ModalOutcome::Commit(())),
        DismissibleModalPlan::Dismiss
    );
    assert_eq!(
        dismissible_modal_plan::<()>(jackin_core::ModalOutcome::Cancel),
        DismissibleModalPlan::Dismiss
    );
    assert_eq!(
        dismissible_modal_plan::<()>(jackin_core::ModalOutcome::Continue),
        DismissibleModalPlan::Continue
    );
}

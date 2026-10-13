// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn inline_account_followup_plan_opens_picker_only_when_supported() {
    assert_eq!(
        inline_account_followup_plan("container", "claude", vec!["anthropic", "zai"]),
        InlineAccountFollowupPlan::OpenAccountPicker(AccountPickerState::new(
            "container",
            "claude",
            vec!["anthropic", "zai"]
        ))
    );
    // Codex with two providers opens the picker.
    assert_eq!(
        inline_account_followup_plan("container", "codex", vec!["openai", "minimax"]),
        InlineAccountFollowupPlan::OpenAccountPicker(AccountPickerState::new(
            "container",
            "codex",
            vec!["openai", "minimax"]
        ))
    );
    // Single-provider choice collapses to a direct start.
    assert_eq!(
        inline_account_followup_plan("container", "codex", vec!["openai"]),
        InlineAccountFollowupPlan::StartSession {
            context: "container",
            agent: "codex",
            account: Some("openai"),
        }
    );
    assert_eq!(
        inline_account_followup_plan::<_, _, &str>("container", "claude", Vec::new()),
        InlineAccountFollowupPlan::StartSession {
            context: "container",
            agent: "claude",
            account: None,
        }
    );
}

#[test]
fn inline_new_session_picker_plan_application_opens_picker() {
    let mut state = TestInlineNewSessionPicker::default();
    let picker = AgentChoiceState::with_choices(jackin_core::Agent::ALL.to_vec());

    apply_inline_new_session_picker_plan(&mut state, "container", picker, Vec::<()>::new());
    assert!(state.picker.is_some());
}

#[test]
fn inline_account_picker_plan_application_opens_picker() {
    let mut state = TestInlineAccountPicker::default();
    let picker = AccountPickerState::new("container", "claude", vec!["anthropic", "zai"]);

    apply_inline_account_picker_plan(&mut state, picker.clone());
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
        inline_picker_plan(jackin_oppicker::ModalOutcome::Commit("agent-smith")),
        InlinePickerPlan::Commit("agent-smith")
    );
    assert_eq!(
        inline_picker_plan::<&str>(jackin_oppicker::ModalOutcome::Cancel),
        InlinePickerPlan::Dismiss
    );
    assert_eq!(
        inline_picker_plan::<&str>(jackin_oppicker::ModalOutcome::Continue),
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
        mount_dst_choice_plan(jackin_oppicker::ModalOutcome::Commit(
            MountDstChoice::SamePath
        )),
        MountDstChoicePlan::CommitSamePath
    );
    assert_eq!(
        mount_dst_choice_plan(jackin_oppicker::ModalOutcome::Commit(MountDstChoice::Edit)),
        MountDstChoicePlan::OpenEditInput
    );
    assert_eq!(
        mount_dst_choice_plan(jackin_oppicker::ModalOutcome::Cancel),
        MountDstChoicePlan::Dismiss
    );
    assert_eq!(
        mount_dst_choice_plan(jackin_oppicker::ModalOutcome::Continue),
        MountDstChoicePlan::Continue
    );
}

#[test]
fn save_discard_modal_plan_routes_save_discard_outcomes() {
    use crate::tui::components::SaveDiscardChoice;

    assert_eq!(
        save_discard_modal_plan(jackin_oppicker::ModalOutcome::Commit(
            SaveDiscardChoice::Save
        )),
        SaveDiscardModalPlan::Save
    );
    assert_eq!(
        save_discard_modal_plan(jackin_oppicker::ModalOutcome::Commit(
            SaveDiscardChoice::Discard
        )),
        SaveDiscardModalPlan::Discard
    );
    assert_eq!(
        save_discard_modal_plan(jackin_oppicker::ModalOutcome::Cancel),
        SaveDiscardModalPlan::Dismiss
    );
    assert_eq!(
        save_discard_modal_plan(jackin_oppicker::ModalOutcome::Continue),
        SaveDiscardModalPlan::Continue
    );
}

#[test]
fn confirm_save_modal_plan_routes_confirm_outcomes() {
    use crate::tui::components::confirm_save::SaveChoice;

    assert_eq!(
        confirm_save_modal_plan(jackin_oppicker::ModalOutcome::Commit(SaveChoice::Save)),
        ConfirmSaveModalPlan::Commit
    );
    assert_eq!(
        confirm_save_modal_plan(jackin_oppicker::ModalOutcome::Cancel),
        ConfirmSaveModalPlan::Dismiss
    );
    assert_eq!(
        confirm_save_modal_plan(jackin_oppicker::ModalOutcome::Continue),
        ConfirmSaveModalPlan::Continue
    );
}

#[test]
fn bool_confirm_modal_plan_routes_confirm_outcomes() {
    assert_eq!(
        bool_confirm_modal_plan(jackin_oppicker::ModalOutcome::Commit(true)),
        BoolConfirmModalPlan::Confirm
    );
    assert_eq!(
        bool_confirm_modal_plan(jackin_oppicker::ModalOutcome::Commit(false)),
        BoolConfirmModalPlan::Dismiss
    );
    assert_eq!(
        bool_confirm_modal_plan(jackin_oppicker::ModalOutcome::Cancel),
        BoolConfirmModalPlan::Dismiss
    );
    assert_eq!(
        bool_confirm_modal_plan(jackin_oppicker::ModalOutcome::Continue),
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
        section: Some(jackin_core::OpSectionTarget::NewLabel("section".to_owned())),
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
        scope_picker_plan(jackin_oppicker::ModalOutcome::Commit(
            ScopeChoice::AllAgents
        )),
        ScopePickerPlan::AllAgents
    );
    assert_eq!(
        scope_picker_plan(jackin_oppicker::ModalOutcome::Commit(
            ScopeChoice::SpecificAgent
        )),
        ScopePickerPlan::SpecificAgent
    );
    assert_eq!(
        scope_picker_plan(jackin_oppicker::ModalOutcome::Cancel),
        ScopePickerPlan::Dismiss
    );
    assert_eq!(
        scope_picker_plan(jackin_oppicker::ModalOutcome::Continue),
        ScopePickerPlan::Continue
    );
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Manager action recording and telemetry.

use super::ManagerMessage;

use super::super::{EditorTab, ManagerEffect, ManagerStage, ManagerState, SettingsTab};

pub(crate) fn apply_settings_message(
    state: &mut ManagerState<'_>,
    message: crate::tui::screens::settings::message::SettingsMessage,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let effect = crate::tui::screens::settings::update::reduce_account_scan_message(
        &mut settings.auth,
        &message,
    );
    if let Some(crate::tui::screens::settings::effect::SettingsEffect::StartAccountScan {
        generation,
    }) = effect
    {
        state.request_effect(ManagerEffect::StartAccountScan { generation });
    }
}

pub(crate) fn request_poll_effect(state: &mut ManagerState<'_>, message: ManagerMessage) {
    let effect = match message {
        ManagerMessage::PollPickerLoads => ManagerEffect::PollPickerLoads,
        ManagerMessage::PollFileBrowserGitUrls => ManagerEffect::PollFileBrowserGitUrls,
        _ => return,
    };
    state.request_effect(effect);
}

pub(crate) fn start_manager_action(
    state: &ManagerState<'_>,
    action: jackin_telemetry::schema::enums::UiActionName,
) -> Option<jackin_telemetry::operation::OperationGuard> {
    jackin_telemetry::ui::start_action(action, telemetry_screen(state), telemetry_widget(state))
}

pub(crate) fn telemetry_screen(
    state: &ManagerState<'_>,
) -> jackin_telemetry::schema::enums::ScreenId {
    use jackin_telemetry::schema::enums::ScreenId;

    match state.stage {
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => ScreenId::WorkspaceList,
        ManagerStage::Editor(_) => ScreenId::WorkspaceEditor,
        ManagerStage::Settings(_) => ScreenId::Settings,
        ManagerStage::CreatePrelude(_) => ScreenId::WorkspaceCreate,
    }
}

pub(crate) fn telemetry_widget(state: &ManagerState<'_>) -> Option<&'static str> {
    match &state.stage {
        ManagerStage::Editor(editor) => Some(match editor.active_tab {
            EditorTab::General => "general",
            EditorTab::Mounts => "mounts",
            EditorTab::Roles => "roles",
            EditorTab::Secrets => "secrets_environments",
            EditorTab::Auth => "auth",
        }),
        ManagerStage::Settings(settings) => Some(match settings.active_tab {
            SettingsTab::General => "general",
            SettingsTab::Mounts => "mounts",
            SettingsTab::Environments => "environments",
            SettingsTab::Auth => "auth",
            SettingsTab::Trust => "trust",
        }),
        _ => None,
    }
}

pub(crate) fn record_manager_action(
    state: &ManagerState<'_>,
    action: jackin_telemetry::schema::enums::UiActionName,
) {
    jackin_telemetry::ui::record_action(action, telemetry_screen(state), telemetry_widget(state));
}

pub(crate) const fn action_of(
    message: &ManagerMessage,
) -> Option<jackin_telemetry::schema::enums::UiActionName> {
    use jackin_telemetry::schema::enums::UiActionName;

    match message {
        ManagerMessage::MoveEditorTab { .. }
        | ManagerMessage::SelectEditorTab(_)
        | ManagerMessage::MoveSettingsTab { .. }
        | ManagerMessage::SelectSettingsTab(_) => Some(UiActionName::TabSwitch),
        ManagerMessage::EnterCreatePrelude(_) => Some(UiActionName::WorkspaceCreate),
        ManagerMessage::EnterEditor(_) => Some(UiActionName::WorkspaceOpen),
        ManagerMessage::EnterSettings(_) => Some(UiActionName::SettingsOpen),
        ManagerMessage::ReturnToList => Some(UiActionName::ScreenBack),
        ManagerMessage::DismissSettingsErrorPopup
        | ManagerMessage::DismissStatusPopup
        | ManagerMessage::DismissListModal
        | ManagerMessage::DismissInlineSessionPicker
        | ManagerMessage::DismissInlineRolePicker
        | ManagerMessage::DismissInlineAgentPicker
        | ManagerMessage::DismissInlineAccountPicker
        | ManagerMessage::DismissLaunchAccountPicker => Some(UiActionName::DialogCancel),
        ManagerMessage::CollapseSelectedTree
        | ManagerMessage::EnterPreview
        | ManagerMessage::EnterConfirmDelete { .. }
        | ManagerMessage::EnterConfirmInstancePurge { .. }
        | ManagerMessage::EnterCreateEditor { .. }
        | ManagerMessage::FileBrowserCommitValidated(_)
        | ManagerMessage::FileBrowserListingLoaded(_)
        | ManagerMessage::InstancesRefreshed(_)
        | ManagerMessage::MountInfoRefreshed(_)
        | ManagerMessage::OpCommitResolved { .. }
        | ManagerMessage::PollFileBrowserGitUrls
        | ManagerMessage::PollPickerLoads
        | ManagerMessage::FocusEditorContent
        | ManagerMessage::FocusEditorTabBar
        | ManagerMessage::FocusSettingsContent
        | ManagerMessage::FocusSettingsTabBar
        | ManagerMessage::ExitPreview
        | ManagerMessage::ExpandSelectedTree
        | ManagerMessage::ClearSettingsAuthKind
        | ManagerMessage::OpenSettingsErrorPopup { .. }
        | ManagerMessage::EnterSettingsAuthKind
        | ManagerMessage::ScrollEditorTabHorizontal { .. }
        | ManagerMessage::SelectEditorMountRow(_)
        | ManagerMessage::SelectListRow(_)
        | ManagerMessage::SelectSettingsTrustRow(_)
        | ManagerMessage::ScrollEditorWorkspaceMountsHorizontal { .. }
        | ManagerMessage::ScrollSettingsGlobalMountsHorizontal { .. }
        | ManagerMessage::ScrollSettingsTrustHorizontal { .. }
        | ManagerMessage::MoveSettingsGlobalMountsSelection { .. }
        | ManagerMessage::MoveSettingsEnvSelection { .. }
        | ManagerMessage::MoveSettingsTrustSelection { .. }
        | ManagerMessage::MoveEditorFieldSelection { .. }
        | ManagerMessage::MoveSettingsGeneralSelection { .. }
        | ManagerMessage::MoveSettingsAuthSelection { .. }
        | ManagerMessage::SetSettingsEnvRoleExpanded { .. }
        | ManagerMessage::SetEditorSecretsRoleExpanded { .. }
        | ManagerMessage::ToggleSettingsGlobalMountReadonly
        | ManagerMessage::ToggleEditorGeneralSelected
        | ManagerMessage::ToggleEditorMountReadonlySelected
        | ManagerMessage::ToggleEditorSecretMask { .. }
        | ManagerMessage::ToggleSettingsGeneralSelected
        | ManagerMessage::ToggleSettingsTrustSelected
        | ManagerMessage::MoveListSelection(_)
        | ManagerMessage::MovePreviewPane { .. }
        | ManagerMessage::ReloadFromConfig { .. }
        | ManagerMessage::ScrollListHorizontal(_)
        | ManagerMessage::ScrollFocusedListBlockVertical(_)
        | ManagerMessage::SetListScrollFocus(_)
        | ManagerMessage::SetListNamesFocused(_)
        | ManagerMessage::SetDragState(_)
        | ManagerMessage::SetListSplitPct(_)
        | ManagerMessage::OpenListErrorPopup { .. }
        | ManagerMessage::OpenStatusPopup { .. }
        | ManagerMessage::OpenListContainerInfo { .. }
        | ManagerMessage::OpenListGithubPicker { .. }
        | ManagerMessage::Settings(_) => None,
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

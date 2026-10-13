// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EditorState` workspace identity and modal chain.

use super::super::super::{
    EditorErrorPopupModal, EditorMode, EditorRoleOverridePickerModal, EditorSaveDiscardModal,
    EditorState, EditorStatusPopupModal,
};
use jackin_config::WorkspaceConfig;

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>
    EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    #[must_use]
    pub fn workspace_name_for_panel(&self) -> String {
        crate::tui::screens::editor::view::editor_name_value(
            &self.mode,
            self.pending_name.as_deref(),
            "(new workspace)",
        )
    }

    pub fn new_create() -> Self
    where
        WorkspaceConfig: Clone + Default,
        MountInfoCache: Default,
        SaveFlow: Default,
    {
        let empty = WorkspaceConfig::default();
        Self::new_edit(String::new(), empty).into_create_mode()
    }

    pub fn new_create_with_workspace(name: String, workspace: WorkspaceConfig) -> Self
    where
        WorkspaceConfig: Clone,
        MountInfoCache: Default,
        SaveFlow: Default,
    {
        let mut editor = Self::new_edit(String::new(), workspace).into_create_mode();
        editor.pending_name = Some(name);
        editor
    }

    pub fn commit_workspace_name_input(&mut self, name: impl Into<String>) {
        self.pending_name = Some(name.into());
        self.clear_modal_chain();
    }

    #[must_use]
    pub(crate) fn into_create_mode(mut self) -> Self {
        self.mode = EditorMode::Create;
        self
    }

    pub fn open_sub_modal(&mut self, child: Modal) {
        if let Some(parent) = self.modal.take() {
            self.modal_parents.push(parent);
        }
        self.modal = Some(child);
    }

    pub fn open_save_discard_cancel<SaveDiscardState>(&mut self, state: SaveDiscardState)
    where
        Modal: EditorSaveDiscardModal<SaveDiscardState>,
    {
        self.modal = Some(Modal::save_discard_cancel_modal(state));
    }

    pub fn open_error_popup<ErrorPopupState>(&mut self, state: ErrorPopupState)
    where
        Modal: EditorErrorPopupModal<ErrorPopupState>,
    {
        self.modal = Some(Modal::error_popup_modal(state));
    }

    pub fn pop_modal_chain(&mut self) {
        self.modal = self.modal_parents.pop();
    }

    pub fn clear_modal_chain(&mut self) {
        self.modal = None;
        self.modal_parents.clear();
    }

    pub fn dismiss_active_modal(&mut self) {
        self.modal = None;
    }

    #[must_use]
    pub fn has_modal_parent(&self) -> bool {
        !self.modal_parents.is_empty()
    }

    pub fn dismiss_status_popup(&mut self)
    where
        Modal: EditorStatusPopupModal,
    {
        if self
            .modal
            .as_ref()
            .is_some_and(EditorStatusPopupModal::is_status_popup)
        {
            self.modal = None;
        }
    }

    #[must_use]
    pub fn has_active_role_override_picker(&self) -> bool
    where
        Modal: EditorRoleOverridePickerModal,
    {
        self.modal
            .as_ref()
            .is_some_and(EditorRoleOverridePickerModal::is_role_override_picker)
    }

    #[must_use]
    pub fn active_auth_form_focus(
        &self,
    ) -> Option<crate::tui::screens::settings::model::AuthFormFocus>
    where
        Modal: crate::tui::auth_config::ModalAuthFormFocusInspect<
                crate::tui::screens::settings::model::AuthFormFocus,
            >,
    {
        self.modal
            .as_ref()
            .and_then(crate::tui::auth_config::ModalAuthFormFocusInspect::active_auth_form_focus)
    }

    #[must_use]
    pub fn has_auth_form_parent(&self) -> bool
    where
        Modal: crate::tui::auth_config::ModalAuthFormParentInspect,
    {
        self.modal_parents
            .last()
            .is_some_and(crate::tui::auth_config::ModalAuthFormParentInspect::is_auth_form_parent)
    }
}

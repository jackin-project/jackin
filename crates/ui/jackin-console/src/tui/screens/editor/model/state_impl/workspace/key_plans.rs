// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EditorState` per-tab action key plans.

use super::super::super::{
    EditorAuthActionKeyPlan, EditorEscapeKeyPlan, EditorFieldSelectionKeyPlan,
    EditorImmediateActionKeyPlan, EditorMountActionKeyPlan, EditorRoleActionKeyPlan,
    EditorRoleHeaderExpansionKeyPlan, EditorSaveKeyPlan, EditorSecretsActionKeyPlan, EditorState,
    EditorTab, EditorTabActionKeyPlan, FieldFocus,
};

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
    pub fn escape_key_plan(&self) -> EditorEscapeKeyPlan {
        if !self.tab_bar_focused() {
            return EditorEscapeKeyPlan::FocusTabBar;
        }
        use crate::tui::screens::edit_save::{EditSaveDisposition, plan_leave_when_dirty};
        match plan_leave_when_dirty(self.is_dirty()) {
            EditSaveDisposition::ConfirmDiscard => EditorEscapeKeyPlan::OpenSaveDiscard,
            EditSaveDisposition::Noop | EditSaveDisposition::SaveNow => {
                EditorEscapeKeyPlan::ReloadFromConfig
            }
        }
    }

    #[must_use]
    pub fn save_key_plan(&self) -> EditorSaveKeyPlan {
        use crate::tui::screens::edit_save::{EditSaveDisposition, plan_explicit_save};
        match plan_explicit_save(self.change_count() > 0) {
            EditSaveDisposition::Noop => EditorSaveKeyPlan::Noop,
            EditSaveDisposition::SaveNow | EditSaveDisposition::ConfirmDiscard => {
                EditorSaveKeyPlan::BeginSave
            }
        }
    }

    #[must_use]
    pub fn focused_role_header_expansion_key_plan(
        &self,
        _config: &jackin_config::AppConfig,
        expanded: bool,
    ) -> EditorRoleHeaderExpansionKeyPlan {
        match self.active_tab {
            EditorTab::Secrets => EditorRoleHeaderExpansionKeyPlan::Secrets(
                self.focused_secrets_role_expansion_plan(expanded),
            ),
            EditorTab::Auth | EditorTab::General | EditorTab::Mounts | EditorTab::Roles => {
                EditorRoleHeaderExpansionKeyPlan::NotRoleHeaderTab
            }
        }
    }

    #[must_use]
    pub fn focused_mount_add_row_selected(&self) -> bool {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::editor_mount_add_row_selected(
            n,
            self.pending.mounts.len(),
        )
    }

    #[must_use]
    pub fn focused_role_add_row_selected(&self, config: &jackin_config::AppConfig) -> bool {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::editor_role_add_row_selected(n, config.roles.len())
    }

    #[must_use]
    pub fn selection_bounds(&self, config: &jackin_config::AppConfig) -> (usize, Vec<usize>) {
        let secrets_rows = self.secrets_flat_rows();
        let auth_rows = self.auth_flat_rows(config);
        crate::tui::screens::editor::update::editor_selection_bounds(
            self.active_tab,
            self.pending.mounts.len(),
            config.roles.len(),
            &secrets_rows,
            &auth_rows,
        )
    }

    #[must_use]
    pub fn field_selection_key_plan(
        &self,
        config: &jackin_config::AppConfig,
        delta: isize,
        term: ratatui::layout::Rect,
    ) -> EditorFieldSelectionKeyPlan {
        let (max_row, skipped_rows) = self.selection_bounds(config);
        EditorFieldSelectionKeyPlan {
            delta,
            max_row,
            skipped_rows,
            term,
            footer_h: self.cached_footer_h,
        }
    }

    #[must_use]
    pub fn immediate_action_key_plan(
        &self,
        _config: &jackin_config::AppConfig,
        key_code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) -> EditorImmediateActionKeyPlan {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key_code {
            KeyCode::Char(' ') if self.active_tab == EditorTab::General => {
                EditorImmediateActionKeyPlan::ToggleGeneralSelected
            }
            KeyCode::Char('r' | 'R') if self.active_tab == EditorTab::Mounts => {
                EditorImmediateActionKeyPlan::ToggleMountReadonlySelected
            }
            KeyCode::Char('m' | 'M')
                if self.active_tab == EditorTab::Secrets
                    && (modifiers - KeyModifiers::SHIFT).is_empty() =>
            {
                self.focused_unmask_key().map_or(
                    EditorImmediateActionKeyPlan::NotImmediateAction,
                    |(scope, key)| EditorImmediateActionKeyPlan::ToggleSecretMask { scope, key },
                )
            }
            _ => EditorImmediateActionKeyPlan::NotImmediateAction,
        }
    }

    #[must_use]
    pub fn role_action_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
    ) -> EditorRoleActionKeyPlan {
        use crossterm::event::KeyCode;

        if self.active_tab != EditorTab::Roles {
            return EditorRoleActionKeyPlan::NotRoleAction;
        }

        match key_code {
            KeyCode::Char('a' | 'A') => EditorRoleActionKeyPlan::OpenRoleInput,
            KeyCode::Char(' ') => EditorRoleActionKeyPlan::ToggleAllowed,
            KeyCode::Char('*') => EditorRoleActionKeyPlan::ToggleDefault,
            _ => EditorRoleActionKeyPlan::NotRoleAction,
        }
    }

    #[must_use]
    pub fn mount_action_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
    ) -> EditorMountActionKeyPlan {
        use crossterm::event::KeyCode;

        if self.active_tab != EditorTab::Mounts {
            return EditorMountActionKeyPlan::NotMountAction;
        }

        match key_code {
            KeyCode::Char('a' | 'A') => EditorMountActionKeyPlan::AddMount,
            KeyCode::Char('d' | 'D') => EditorMountActionKeyPlan::RemoveSelectedMount,
            KeyCode::Char('i' | 'I') => EditorMountActionKeyPlan::CycleIsolation,
            KeyCode::Char('o' | 'O') => EditorMountActionKeyPlan::OpenGithub,
            _ => EditorMountActionKeyPlan::NotMountAction,
        }
    }

    #[must_use]
    pub fn secrets_action_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
        op_available: bool,
    ) -> EditorSecretsActionKeyPlan {
        use crossterm::event::{KeyCode, KeyModifiers};

        if self.active_tab != EditorTab::Secrets || !(modifiers - KeyModifiers::SHIFT).is_empty() {
            return EditorSecretsActionKeyPlan::NotSecretsAction;
        }

        match key_code {
            KeyCode::Char('p' | 'P') if op_available => EditorSecretsActionKeyPlan::OpenPicker,
            KeyCode::Char('d' | 'D') => EditorSecretsActionKeyPlan::OpenDeleteConfirm,
            KeyCode::Char('a' | 'A') => EditorSecretsActionKeyPlan::OpenAddModal,
            _ => EditorSecretsActionKeyPlan::NotSecretsAction,
        }
    }

    #[must_use]
    pub fn auth_action_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
    ) -> EditorAuthActionKeyPlan {
        use crossterm::event::KeyCode;

        if self.active_tab != EditorTab::Auth {
            return EditorAuthActionKeyPlan::NotAuthAction;
        }

        match key_code {
            KeyCode::Char('d' | 'D') => EditorAuthActionKeyPlan::ClearFocusedRow,
            _ => EditorAuthActionKeyPlan::NotAuthAction,
        }
    }

    #[must_use]
    pub fn tab_action_key_plan(
        &self,
        config: &jackin_config::AppConfig,
        key_code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
        op_available: bool,
    ) -> EditorTabActionKeyPlan {
        use crossterm::event::KeyCode;

        let role_action = self.role_action_key_plan(key_code);
        if !matches!(role_action, EditorRoleActionKeyPlan::NotRoleAction) {
            return EditorTabActionKeyPlan::Role(role_action);
        }

        let mount_action = self.mount_action_key_plan(key_code);
        if !matches!(mount_action, EditorMountActionKeyPlan::NotMountAction) {
            return EditorTabActionKeyPlan::Mount(mount_action);
        }

        let secrets_action = self.secrets_action_key_plan(key_code, modifiers, op_available);
        if !matches!(secrets_action, EditorSecretsActionKeyPlan::NotSecretsAction) {
            return EditorTabActionKeyPlan::Secrets(secrets_action);
        }

        let auth_action = self.auth_action_key_plan(key_code);
        if !matches!(auth_action, EditorAuthActionKeyPlan::NotAuthAction) {
            return EditorTabActionKeyPlan::Auth(auth_action);
        }

        if key_code == KeyCode::Enter {
            return EditorTabActionKeyPlan::Enter(self.enter_key_plan(config, op_available));
        }

        EditorTabActionKeyPlan::Noop
    }
}

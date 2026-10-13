// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EditorState` secret value and enter-plan queries.

use super::super::super::{
    EditorState, FieldFocus, RoleHeaderExpansionPlan, SecretsEnterPlan, SecretsRow, SecretsScopeTag,
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
    pub fn secret_value(
        &self,
        scope: &SecretsScopeTag,
        key: &str,
    ) -> Option<&jackin_core::EnvValue> {
        match scope {
            SecretsScopeTag::Workspace => self.pending.env.get(key),
            SecretsScopeTag::Role(role) => self
                .pending
                .roles
                .get(role)
                .and_then(|role_override| role_override.env.get(key)),
        }
    }

    #[must_use]
    pub fn secret_is_text_editable(&self, scope: &SecretsScopeTag, key: &str) -> bool {
        !self
            .secret_value(scope, key)
            .is_some_and(|value| matches!(value, jackin_core::EnvValue::OpRef(_)))
    }

    #[must_use]
    pub fn focused_secret_is_op_ref(&self) -> bool {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        match rows.get(n) {
            Some(SecretsRow::WorkspaceKeyRow(key)) => self
                .pending
                .env
                .get(key)
                .is_some_and(|value| matches!(value, jackin_core::EnvValue::OpRef(_))),
            Some(SecretsRow::RoleKeyRow { role, key }) => self
                .pending
                .roles
                .get(role)
                .and_then(|role_override| role_override.env.get(key))
                .is_some_and(|value| matches!(value, jackin_core::EnvValue::OpRef(_))),
            _ => false,
        }
    }

    /// No-op on header/sentinel/op:// rows.
    #[must_use]
    pub fn focused_unmask_key(&self) -> Option<(SecretsScopeTag, String)> {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        crate::tui::screens::editor::update::secret_unmask_target_for_row(
            rows.get(n),
            |scope, key| self.secret_is_text_editable(scope, key),
        )
    }

    #[must_use]
    pub fn focused_secret_enter_plan(&self) -> SecretsEnterPlan {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        crate::tui::screens::editor::update::secret_enter_plan_for_row(rows.get(n), |scope, key| {
            self.secret_is_text_editable(scope, key)
        })
    }

    #[must_use]
    pub fn focused_secret_delete_target(&self) -> Option<(SecretsScopeTag, String)> {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        crate::tui::screens::editor::update::secret_delete_target_for_row(rows.get(n))
    }

    #[must_use]
    pub fn focused_secret_add_target(&self) -> Option<SecretsScopeTag> {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        crate::tui::screens::editor::update::secret_add_target_for_row(rows.get(n))
    }

    #[must_use]
    pub fn focused_secrets_role_expansion_plan(&self, expanded: bool) -> RoleHeaderExpansionPlan {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        let Some(SecretsRow::RoleHeader {
            role,
            expanded: current,
        }) = rows.get(n).cloned()
        else {
            return RoleHeaderExpansionPlan::NotHeader;
        };
        if current == expanded {
            RoleHeaderExpansionPlan::HeaderNoop
        } else {
            RoleHeaderExpansionPlan::Set { role, expanded }
        }
    }

    #[must_use]
    pub fn synthesize_app_config_for_auth(
        &self,
        config: &jackin_config::AppConfig,
    ) -> jackin_config::AppConfig {
        let mut result = config.clone();
        result
            .workspaces
            .insert(self.workspace_name_for_panel(), self.pending.clone());
        result
    }
}

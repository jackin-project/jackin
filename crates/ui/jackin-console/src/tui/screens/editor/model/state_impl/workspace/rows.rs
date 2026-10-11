// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EditorState` flat rows and enter-key plans.

use super::super::super::{
    AuthEnterPlan, AuthRow, EditorEnterKeyPlan, EditorState, EditorTab, FieldFocus, SecretsRow,
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
    pub fn secrets_flat_rows(&self) -> Vec<SecretsRow> {
        crate::tui::screens::editor::update::secrets_flat_rows(
            &self.pending.env,
            &self.pending.roles,
            &self.secrets_expanded,
            |role| &role.env,
        )
    }

    #[must_use]
    pub fn auth_flat_rows(
        &self,
        config: &jackin_config::AppConfig,
    ) -> Vec<AuthRow<crate::tui::auth::AuthKind>> {
        let mut rows: Vec<_> = config
            .accounts
            .keys()
            .map(|id| AuthRow::Account { id: id.clone() })
            .collect();
        for &agent in jackin_core::Agent::ALL {
            rows.push(AuthRow::Binding { agent, role: None });
        }
        rows.push(AuthRow::WorkspaceMode {
            kind: crate::tui::auth::AuthKind::Github,
        });
        for role in self.eligible_role_override_selectors(config.roles.keys()) {
            rows.push(AuthRow::RoleMode {
                role: role.key(),
                kind: crate::tui::auth::AuthKind::Github,
            });
            for &agent in jackin_core::Agent::ALL {
                rows.push(AuthRow::Binding {
                    agent,
                    role: Some(role.key()),
                });
            }
        }
        rows
    }

    #[must_use]
    pub fn focused_auth_enter_plan(&self, config: &jackin_config::AppConfig) -> AuthEnterPlan {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.auth_flat_rows(config);
        match rows.get(n) {
            Some(AuthRow::WorkspaceMode { .. } | AuthRow::RoleMode { .. }) => {
                AuthEnterPlan::OpenForm
            }
            _ => AuthEnterPlan::Noop,
        }
    }

    #[must_use]
    pub fn enter_key_plan(
        &self,
        config: &jackin_config::AppConfig,
        op_available: bool,
    ) -> EditorEnterKeyPlan {
        match self.active_tab {
            EditorTab::General => EditorEnterKeyPlan::OpenGeneralField,
            EditorTab::Mounts if self.focused_mount_add_row_selected() => {
                EditorEnterKeyPlan::OpenMountFileBrowser
            }
            EditorTab::Mounts => EditorEnterKeyPlan::Noop,
            EditorTab::Secrets if self.focused_secret_is_op_ref() && op_available => {
                EditorEnterKeyPlan::OpenSecretsPicker
            }
            EditorTab::Secrets => EditorEnterKeyPlan::OpenSecretsEnterModal,
            EditorTab::Roles if self.focused_role_add_row_selected(config) => {
                EditorEnterKeyPlan::OpenRoleInput
            }
            EditorTab::Roles => EditorEnterKeyPlan::Noop,
            EditorTab::Auth => EditorEnterKeyPlan::Auth(self.focused_auth_enter_plan(config)),
        }
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EditorState` mount, role, and secret mutations.

use super::super::super::{EditorState, FieldFocus, SecretsScopeTag};

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
    pub fn cycle_isolation_for_selected_mount(&mut self) {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::cycle_mount_isolation_at(&mut self.pending.mounts, n);
    }

    pub fn remove_selected_mount(&mut self) {
        let FieldFocus::Row(n) = self.active_field;
        if n < self.pending.mounts.len() {
            self.pending.mounts.remove(n);
        }
    }

    pub fn add_shared_mount(&mut self, src: &str, dst: &str) {
        self.pending
            .mounts
            .push(crate::services::workspace::shared_mount_config(
                src, dst, false,
            ));
    }

    pub fn toggle_general_selected(&mut self) {
        let FieldFocus::Row(row) = self.active_field;
        match row {
            2 => {
                self.pending.keep_awake.enabled = !self.pending.keep_awake.enabled;
            }
            3 => {
                self.pending.git_pull_on_entry = !self.pending.git_pull_on_entry;
            }
            _ => {}
        }
    }

    pub fn toggle_selected_mount_readonly(&mut self) {
        let FieldFocus::Row(row) = self.active_field;
        if let Some(mount) = self.pending.mounts.get_mut(row) {
            mount.readonly = !mount.readonly;
        }
    }

    #[must_use]
    pub fn eligible_role_override_selectors<'a>(
        &self,
        registered_roles: impl Iterator<Item = &'a String> + 'a,
    ) -> Vec<jackin_core::RoleSelector> {
        crate::workspace::eligible_role_keys_for_override(registered_roles, &self.pending)
            .into_iter()
            .filter_map(|name| jackin_core::RoleSelector::parse(&name).ok())
            .collect()
    }

    pub fn toggle_allowed_role_at_cursor(&mut self, role_names: &[String]) {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::toggle_allowed_role_at(
            &mut self.pending.allowed_roles,
            &mut self.pending.default_role,
            role_names,
            n,
        );
    }

    pub fn toggle_default_role_at_cursor(&mut self, role_names: &[String]) {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::toggle_default_role_at(
            &self.pending.allowed_roles,
            &mut self.pending.default_role,
            role_names,
            n,
        );
    }

    pub fn set_secrets_role_expanded(&mut self, role: String, expanded: bool) {
        if expanded {
            self.secrets_expanded.insert(role);
        } else {
            self.secrets_expanded.remove(&role);
        }
    }

    pub fn toggle_secret_mask(&mut self, scope: SecretsScopeTag, key: String) {
        let entry = (scope, key);
        if !self.unmasked_rows.remove(&entry) {
            self.unmasked_rows.insert(entry);
        }
    }

    /// Delete an environment key from the draft workspace or role override.
    /// Remove a role override only when every role-specific field is default.
    pub fn delete_env_var(&mut self, scope: &SecretsScopeTag, key: &str) -> anyhow::Result<()> {
        match scope {
            SecretsScopeTag::Workspace => {
                self.pending.env.remove(key);
            }
            SecretsScopeTag::Role(role) => {
                let mut drop_role = false;
                if let Some(override_config) = self.pending.roles.get_mut(role) {
                    override_config.env.remove(key);
                    drop_role = override_config.is_default();
                }
                if drop_role {
                    self.pending.roles.remove(role);
                }
            }
        }

        Ok(())
    }
}

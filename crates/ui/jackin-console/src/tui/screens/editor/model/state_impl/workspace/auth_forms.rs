// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EditorState` auth-form focus and persistence.

use super::super::super::{AuthRow, EditorState, FieldFocus};

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
    /// Toggle assignment or cycle an explicit binding through compatible assigned accounts.
    pub fn edit_account_row(&mut self, config: &jackin_config::AppConfig, clear: bool) {
        let FieldFocus::Row(index) = self.active_field;
        let rows = self.auth_flat_rows(config);
        match rows.get(index) {
            Some(AuthRow::Account { id }) => {
                if clear || self.pending.accounts.contains(id) {
                    self.pending.accounts.retain(|value| value != id);
                    self.pending.account_bindings.retain(|_, value| value != id);
                    for role in self.pending.roles.values_mut() {
                        role.account_bindings.retain(|_, value| value != id);
                    }
                } else if config
                    .accounts
                    .get(id)
                    .is_some_and(|account| account.enabled)
                {
                    self.pending.accounts.push(id.clone());
                }
            }
            Some(AuthRow::Binding { agent, role }) => {
                let candidates: Vec<_> = self
                    .pending
                    .accounts
                    .iter()
                    .filter(|id| {
                        config
                            .accounts
                            .get(*id)
                            .is_some_and(|account| account.supports_agent(*agent))
                    })
                    .cloned()
                    .collect();
                let bindings = match role {
                    Some(role) => {
                        &mut self
                            .pending
                            .roles
                            .entry(role.clone())
                            .or_default()
                            .account_bindings
                    }
                    None => &mut self.pending.account_bindings,
                };
                let next = if clear {
                    None
                } else {
                    match bindings.get(agent) {
                        Some(current) => candidates
                            .iter()
                            .position(|id| id == current)
                            .and_then(|index| candidates.get(index + 1))
                            .cloned(),
                        None => candidates.first().cloned(),
                    }
                };
                if let Some(id) = next {
                    bindings.insert(*agent, id);
                } else {
                    bindings.remove(agent);
                }
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn focused_account_row(&self, config: &jackin_config::AppConfig) -> bool {
        let FieldFocus::Row(index) = self.active_field;
        matches!(
            self.auth_flat_rows(config).get(index),
            Some(AuthRow::Account { .. } | AuthRow::Binding { .. })
        )
    }

    #[must_use]
    pub fn focused_auth_form(
        &self,
        config: &jackin_config::AppConfig,
    ) -> Option<(
        crate::tui::state::AuthFormTarget,
        crate::tui::state::AuthForm,
    )> {
        let FieldFocus::Row(index) = self.active_field;
        let target = self.resolve_auth_form_target(config, index)?;
        if *target.kind() != crate::tui::auth::AuthKind::Github {
            return None;
        }
        let existing = match &target {
            crate::tui::state::AuthFormTarget::Workspace { .. } => self.pending.github.as_ref(),
            crate::tui::state::AuthFormTarget::WorkspaceRole { role, .. } => self
                .pending
                .roles
                .get(role)
                .and_then(|role| role.github.as_ref()),
        };
        let form = existing.map_or_else(
            || crate::tui::state::AuthForm::new(crate::tui::auth::AuthKind::Github),
            |github| {
                let mode = match github.auth_forward {
                    jackin_config::GithubAuthMode::Sync => crate::tui::auth::AuthMode::Sync,
                    jackin_config::GithubAuthMode::Token => crate::tui::auth::AuthMode::Token,
                    jackin_config::GithubAuthMode::Ignore => crate::tui::auth::AuthMode::Ignore,
                };
                crate::tui::state::AuthForm::from_existing(
                    crate::tui::auth::AuthKind::Github,
                    mode,
                    github.env.get("GH_TOKEN").cloned(),
                )
            },
        );
        Some((target, form))
    }

    pub fn persist_auth_form(
        &mut self,
        target: &crate::tui::state::AuthFormTarget,
        form: &crate::tui::state::AuthForm,
    ) {
        if *target.kind() != crate::tui::auth::AuthKind::Github {
            return;
        }
        let Some(outcome) = form.commit() else {
            return;
        };
        let mode = match outcome.mode {
            crate::tui::auth::AuthMode::Sync => jackin_config::GithubAuthMode::Sync,
            crate::tui::auth::AuthMode::Token => jackin_config::GithubAuthMode::Token,
            crate::tui::auth::AuthMode::Ignore => jackin_config::GithubAuthMode::Ignore,
            _ => return,
        };
        let slot = match target {
            crate::tui::state::AuthFormTarget::Workspace { .. } => &mut self.pending.github,
            crate::tui::state::AuthFormTarget::WorkspaceRole { role, .. } => {
                &mut self.pending.roles.entry(role.clone()).or_default().github
            }
        };
        let github = slot.get_or_insert_with(Default::default);
        github.auth_forward = mode;
        github.env.remove("GH_TOKEN");
        if let Some(value) = outcome.env_value {
            github.env.insert("GH_TOKEN".into(), value);
        }
    }

    pub fn clear_auth_form_layer(&mut self, target: &crate::tui::state::AuthFormTarget) {
        if *target.kind() != crate::tui::auth::AuthKind::Github {
            return;
        }
        match target {
            crate::tui::state::AuthFormTarget::Workspace { .. } => self.pending.github = None,
            crate::tui::state::AuthFormTarget::WorkspaceRole { role, .. } => {
                if let Some(role) = self.pending.roles.get_mut(role) {
                    role.github = None;
                }
            }
        }
    }

    pub fn clear_auth_row_at_cursor(&mut self, config: &jackin_config::AppConfig) {
        if self.focused_account_row(config) {
            self.edit_account_row(config, true);
        } else {
            let FieldFocus::Row(index) = self.active_field;
            if let Some(target) = self.resolve_auth_form_target(config, index) {
                self.clear_auth_form_layer(&target);
            }
        }
    }

    #[must_use]
    pub fn resolve_auth_form_target(
        &self,
        config: &jackin_config::AppConfig,
        row: usize,
    ) -> Option<crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>>
    {
        let rows = self.auth_flat_rows(config);
        crate::tui::screens::editor::update::resolve_auth_form_target(&rows, row)
    }
}

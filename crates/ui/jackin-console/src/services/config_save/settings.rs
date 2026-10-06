// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings save execution.

use super::validate_settings_env;
use jackin_config::{AppConfig, EnvScope};
use std::collections::BTreeMap;

use crate::tui::screens::settings::model::SettingsTrustRow;
use jackin_core::Agent;

/// Input bundle for a settings-screen save operation.
#[derive(Debug)]
pub struct SettingsSaveInput<'a> {
    pub mounts_original: &'a [jackin_config::GlobalMountRow],
    pub mounts_pending: &'a [jackin_config::GlobalMountRow],
    pub env_original: &'a crate::tui::state::SettingsEnvConfig,
    pub env_pending: &'a crate::tui::state::SettingsEnvConfig,
    pub auth_pending: &'a BTreeMap<String, jackin_config::AccountConfig>,
    pub auth_original: &'a BTreeMap<String, jackin_config::AccountConfig>,
    pub github: &'a jackin_config::GithubAuthConfig,
    pub original_github: &'a jackin_config::GithubAuthConfig,
    pub bindings_pending: &'a BTreeMap<Agent, String>,
    pub bindings_original: &'a BTreeMap<Agent, String>,
    pub trust_pending: &'a [SettingsTrustRow],
    pub git_coauthor_trailer: bool,
    pub git_dco: bool,
}

/// Save all settings tabs and return the reloaded config model.
#[expect(
    clippy::needless_pass_by_value,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn save_settings(
    paths: &jackin_core::JackinPaths,
    input: SettingsSaveInput<'_>,
) -> anyhow::Result<AppConfig> {
    AppConfig::validate_global_mount_rows(input.mounts_pending)?;
    validate_settings_env(input.env_pending, input.trust_pending)?;
    let mut editor_doc = jackin_config::ConfigEditor::open(paths)?;

    for row in input.mounts_original {
        editor_doc.remove_mount(&row.name, row.scope.as_deref());
    }
    for row in input.mounts_pending {
        editor_doc.add_mount(&row.name, row.mount.clone(), row.scope.as_deref());
    }

    for key in input.env_original.env.keys() {
        editor_doc.remove_env_var(&EnvScope::Global, key);
    }
    for (role, env) in &input.env_original.roles {
        for key in env.keys() {
            editor_doc.remove_env_var(&EnvScope::Role(role.clone()), key);
        }
    }
    for (key, value) in &input.env_pending.env {
        editor_doc.set_env_var(&EnvScope::Global, key, value.clone())?;
    }
    for (role, env) in &input.env_pending.roles {
        for (key, value) in env {
            editor_doc.set_env_var(&EnvScope::Role(role.clone()), key, value.clone())?;
        }
    }

    for id in input
        .auth_original
        .keys()
        .filter(|id| !input.auth_pending.contains_key(*id))
    {
        editor_doc.remove_account(id)?;
    }
    for (id, account) in input.auth_pending {
        if input.auth_original.get(id) != Some(account) {
            editor_doc.upsert_account(id, account)?;
        }
    }

    for agent in Agent::ALL {
        if input.bindings_original.get(agent) != input.bindings_pending.get(agent) {
            editor_doc.set_account_binding(
                None,
                None,
                *agent,
                input.bindings_pending.get(agent).map(String::as_str),
            )?;
        }
    }

    if input.github != input.original_github {
        editor_doc.set_global_github_auth_forward(input.github.auth_forward);
        for key in input.original_github.env.keys() {
            editor_doc.remove_global_github_env_var(key);
        }
        for (key, value) in &input.github.env {
            editor_doc.set_global_github_env_var(key, value.clone())?;
        }
    }

    for row in input.trust_pending {
        editor_doc.set_agent_trust(&row.role, row.trusted);
    }

    editor_doc.set_git_coauthor_trailer(input.git_coauthor_trailer);
    editor_doc.set_git_dco(input.git_dco);

    Ok(editor_doc.save()?)
}

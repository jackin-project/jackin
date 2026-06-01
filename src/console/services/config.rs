//! Non-TUI config persistence services.

use std::collections::{BTreeMap, BTreeSet};

use crate::config::{AppConfig, EnvScope, GlobalMountRow, RoleSource};
use crate::console::manager::auth_kind::{
    AuthKind, auth_kind_agent, auth_mode_to_auth_forward, auth_mode_to_github,
};
use crate::console::manager::state::{SettingsAuthRow, SettingsEnvConfig, SettingsTrustRow};
use crate::operator_env::EnvValue;
use crate::paths::JackinPaths;

/// Upsert one role source into the operator config and reload the saved model.
pub fn upsert_role_source(
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: &str,
    source: &RoleSource,
) -> anyhow::Result<()> {
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;
    editor_doc.upsert_agent_source(key, source);
    *config = editor_doc.save()?;
    Ok(())
}

/// Remove one saved workspace from operator config and reload the saved model.
pub fn remove_workspace(
    config: &mut AppConfig,
    paths: &JackinPaths,
    name: &str,
) -> anyhow::Result<()> {
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;
    editor_doc.remove_workspace(name)?;
    *config = editor_doc.save()?;
    Ok(())
}

/// Save the global mount table and return the reloaded config model.
pub fn save_global_mounts(
    paths: &JackinPaths,
    original: &[GlobalMountRow],
    pending: &[GlobalMountRow],
) -> anyhow::Result<AppConfig> {
    AppConfig::validate_global_mount_rows(pending)?;
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;
    for row in original {
        editor_doc.remove_mount(&row.name, row.scope.as_deref());
    }
    for row in pending {
        editor_doc.add_mount(&row.name, row.mount.clone(), row.scope.as_deref());
    }
    editor_doc.save()
}

pub struct SettingsSaveInput<'a> {
    pub mounts_original: &'a [GlobalMountRow],
    pub mounts_pending: &'a [GlobalMountRow],
    pub env_original: &'a SettingsEnvConfig,
    pub env_pending: &'a SettingsEnvConfig,
    pub auth_pending: &'a [SettingsAuthRow],
    pub original_github_env: &'a BTreeMap<String, EnvValue>,
    pub github_env: &'a BTreeMap<String, EnvValue>,
    pub trust_pending: &'a [SettingsTrustRow],
    pub git_coauthor_trailer: bool,
    pub git_dco: bool,
}

/// Save all settings tabs and return the reloaded config model.
pub fn save_settings(paths: &JackinPaths, input: SettingsSaveInput<'_>) -> anyhow::Result<AppConfig> {
    AppConfig::validate_global_mount_rows(input.mounts_pending)?;
    validate_settings_env(input.env_pending, input.trust_pending)?;
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;

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

    for row in input.auth_pending {
        match row.kind {
            AuthKind::Claude
            | AuthKind::Codex
            | AuthKind::Amp
            | AuthKind::Kimi
            | AuthKind::Opencode => {
                let Some(agent) = auth_kind_agent(row.kind) else {
                    continue;
                };
                let Some(mode) = auth_mode_to_auth_forward(row.mode) else {
                    anyhow::bail!(
                        "auth mode {} is not supported for {}",
                        row.mode.as_str(),
                        row.kind.label()
                    );
                };
                editor_doc.set_global_auth_forward(agent, mode);
            }
            AuthKind::Github => {
                let Some(mode) = auth_mode_to_github(row.mode) else {
                    anyhow::bail!(
                        "auth mode {} is not supported for {}",
                        row.mode.as_str(),
                        row.kind.label()
                    );
                };
                editor_doc.set_global_github_auth_forward(mode);
            }
            AuthKind::Zai => {}
        }
    }
    for key in input.original_github_env.keys() {
        editor_doc.remove_global_github_env_var(key);
    }
    for (key, value) in input.github_env {
        editor_doc.set_global_github_env_var(key, value.clone())?;
    }

    for row in input.trust_pending {
        editor_doc.set_agent_trust(&row.role, row.trusted);
    }

    editor_doc.set_git_coauthor_trailer(input.git_coauthor_trailer);
    editor_doc.set_git_dco(input.git_dco);

    editor_doc.save()
}

fn validate_settings_env(
    env: &SettingsEnvConfig,
    roles: &[SettingsTrustRow],
) -> anyhow::Result<()> {
    let registered: BTreeSet<&str> = roles.iter().map(|r| r.role.as_str()).collect();
    validate_settings_env_keys("global", env.env.keys())?;
    for (role, role_env) in &env.roles {
        if !registered.contains(role.as_str()) {
            anyhow::bail!("role {role:?} is not registered");
        }
        validate_settings_env_keys(role, role_env.keys())?;
    }
    Ok(())
}

fn validate_settings_env_keys<'a>(
    scope: &str,
    keys: impl Iterator<Item = &'a String>,
) -> anyhow::Result<()> {
    for key in keys {
        if key.trim().is_empty() {
            anyhow::bail!("env var key cannot be empty");
        }
        if crate::env_model::is_reserved(key) {
            anyhow::bail!(
                "env name {key:?} in {scope} is reserved by the jackin runtime and cannot be set"
            );
        }
    }
    Ok(())
}

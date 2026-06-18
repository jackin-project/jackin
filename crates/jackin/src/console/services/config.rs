//! Non-TUI config persistence services.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::agent::Agent;
use crate::config::{AppConfig, AuthForwardMode, EnvScope, GlobalMountRow, RoleSource};
use crate::console::domain::{auth_kind_agent, auth_mode_to_auth_forward, auth_mode_to_github};
use crate::console::tui::state::{SettingsAuthRow, SettingsEnvConfig, SettingsTrustRow};
use crate::operator_env::EnvValue;
use crate::paths::JackinPaths;
use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
use jackin_console::tui::auth::AuthKind;

const WORKSPACE_AUTH_AGENTS: [Agent; 6] = [
    Agent::Claude,
    Agent::Codex,
    Agent::Amp,
    Agent::Kimi,
    Agent::Opencode,
    Agent::Grok,
];

/// Upsert one role source into the operator config and reload the saved model.
pub(crate) fn upsert_role_source(
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
pub(crate) fn remove_workspace(
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
#[cfg(test)]
pub(crate) fn save_global_mounts(
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

pub(crate) struct SettingsSaveInput<'a> {
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
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)]
pub(crate) fn save_settings(
    paths: &JackinPaths,
    input: SettingsSaveInput<'_>,
) -> anyhow::Result<AppConfig> {
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
            | AuthKind::Opencode
            | AuthKind::Grok => {
                let Some(agent) = auth_kind_agent(row.kind) else {
                    continue;
                };
                if !row.kind.supported_modes().contains(&row.mode) {
                    anyhow::bail!(
                        "auth mode {} is not supported for {}",
                        row.mode.as_str(),
                        row.kind.label()
                    );
                }
                let Some(mode) = auth_mode_to_auth_forward(row.mode) else {
                    anyhow::bail!(
                        "auth mode {} is not supported for {}",
                        row.mode.as_str(),
                        row.kind.label()
                    );
                };
                editor_doc.set_global_auth_forward(agent, mode);
                editor_doc.set_global_sync_source_dir(agent, row.sync_source_dir.as_deref());
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
            // Provider-credential kinds are env-only; the credential lives in the
            // env_vars block and is written via the env commit path above.
            AuthKind::Zai | AuthKind::Minimax => {}
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

pub(crate) enum WorkspaceSaveMode {
    Edit {
        original_name: String,
        pending_name: Option<String>,
        effective_removals: Vec<String>,
    },
    Create {
        name: String,
    },
}

pub(crate) struct WorkspaceSaveInput<'a> {
    pub mode: WorkspaceSaveMode,
    pub original: &'a WorkspaceConfig,
    pub pending: &'a WorkspaceConfig,
}

pub(crate) struct WorkspaceSaveResult {
    pub config: AppConfig,
    pub current_name: String,
    pub pending_rename: Option<String>,
}

/// Persist a workspace create/edit and return the reloaded config model.
#[allow(clippy::useless_let_if_seq)]
pub(crate) fn save_workspace(
    paths: &JackinPaths,
    input: WorkspaceSaveInput<'_>,
) -> anyhow::Result<WorkspaceSaveResult> {
    let mut editor_doc = crate::config::ConfigEditor::open(paths)?;
    let (pending_rename, current_name) = match input.mode {
        WorkspaceSaveMode::Edit {
            original_name,
            pending_name,
            effective_removals,
        } => {
            let mut current_name = original_name;
            let mut rename_to = None;
            if let Some(new_name) = pending_name
                && new_name != current_name
            {
                editor_doc.rename_workspace(&current_name, &new_name)?;
                current_name.clone_from(&new_name);
                rename_to = Some(new_name);
            }

            let mut edit =
                crate::console::domain::build_workspace_edit(input.original, input.pending);
            edit.remove_destinations = effective_removals;
            editor_doc.edit_workspace(&current_name, edit)?;
            apply_auth_forward_diff(
                &mut editor_doc,
                &current_name,
                input.original,
                input.pending,
            );
            apply_sync_source_dir_diff(
                &mut editor_doc,
                &current_name,
                input.original,
                input.pending,
            );
            (rename_to, current_name)
        }
        WorkspaceSaveMode::Create { name } => {
            editor_doc.create_workspace(&name, input.pending.clone())?;
            (None, name)
        }
    };

    apply_env_diff(
        &mut editor_doc,
        &current_name,
        input.original,
        input.pending,
    )?;
    let config = editor_doc.save()?;
    Ok(WorkspaceSaveResult {
        config,
        current_name,
        pending_rename,
    })
}

/// Reapply auth-forward deltas after `edit_workspace` rewrites the workspace table.
pub(crate) fn apply_auth_forward_diff(
    editor_doc: &mut crate::config::ConfigEditor,
    workspace_name: &str,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) {
    for agent in WORKSPACE_AUTH_AGENTS {
        let original_mode = original.auth_forward_for(agent);
        let pending_mode = pending.auth_forward_for(agent);
        if original_mode != pending_mode {
            editor_doc.set_workspace_auth_forward(workspace_name, agent, pending_mode);
        }
    }
    let original_github = original.github.as_ref().map(|g| g.auth_forward);
    let pending_github = pending.github.as_ref().map(|g| g.auth_forward);
    if original_github != pending_github {
        editor_doc.set_workspace_github_auth_forward(workspace_name, pending_github);
    }

    let role_keys: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    for role in role_keys {
        let orig_override = original.roles.get(role);
        let pend_override = pending.roles.get(role);
        for agent in WORKSPACE_AUTH_AGENTS {
            let original_mode = role_auth_forward_for(orig_override, agent);
            let pending_mode = role_auth_forward_for(pend_override, agent);
            if original_mode != pending_mode {
                editor_doc.set_workspace_role_auth_forward(
                    workspace_name,
                    role,
                    agent,
                    pending_mode,
                );
            }
        }
        let orig_github = orig_override
            .and_then(|o| o.github.as_ref())
            .map(|g| g.auth_forward);
        let pend_github = pend_override
            .and_then(|p| p.github.as_ref())
            .map(|g| g.auth_forward);
        if orig_github != pend_github {
            editor_doc.set_workspace_role_github_auth_forward(workspace_name, role, pend_github);
        }
    }
}

fn apply_sync_source_dir_diff(
    editor_doc: &mut crate::config::ConfigEditor,
    workspace_name: &str,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) {
    for agent in WORKSPACE_AUTH_AGENTS {
        let original_source = original.sync_source_dir_for(agent);
        let pending_source = pending.sync_source_dir_for(agent);
        if original_source != pending_source {
            editor_doc.set_workspace_sync_source_dir(
                workspace_name,
                agent,
                pending_source.as_deref(),
            );
        }
    }

    let role_keys: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    for role in role_keys {
        let orig_override = original.roles.get(role);
        let pend_override = pending.roles.get(role);
        for agent in WORKSPACE_AUTH_AGENTS {
            let original_source = role_sync_source_dir_for(orig_override, agent);
            let pending_source = role_sync_source_dir_for(pend_override, agent);
            if original_source != pending_source {
                editor_doc.set_workspace_role_sync_source_dir(
                    workspace_name,
                    role,
                    agent,
                    pending_source.as_deref(),
                );
            }
        }
    }
}

fn role_auth_forward_for(
    role: Option<&WorkspaceRoleOverride>,
    agent: Agent,
) -> Option<AuthForwardMode> {
    role.and_then(|r| r.auth_forward_for(agent))
}

fn role_sync_source_dir_for(role: Option<&WorkspaceRoleOverride>, agent: Agent) -> Option<PathBuf> {
    role.and_then(|r| r.sync_source_dir_for(agent))
}

fn apply_env_diff(
    editor_doc: &mut crate::config::ConfigEditor,
    workspace_name: &str,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) -> anyhow::Result<()> {
    let ws_scope = EnvScope::Workspace(workspace_name.to_owned());
    apply_env_map_diff(editor_doc, &ws_scope, &original.env, &pending.env)?;

    let empty = BTreeMap::<String, EnvValue>::new();
    let orig_ws_github_env = original.github.as_ref().map_or(&empty, |g| &g.env);
    let pend_ws_github_env = pending.github.as_ref().map_or(&empty, |g| &g.env);
    let ws_github_scope = EnvScope::WorkspaceGithub(workspace_name.to_owned());
    apply_env_map_diff(
        editor_doc,
        &ws_github_scope,
        orig_ws_github_env,
        pend_ws_github_env,
    )?;

    let role_keys: BTreeSet<&String> = original.roles.keys().chain(pending.roles.keys()).collect();
    for role in role_keys {
        let orig_env = original.roles.get(role).map_or(&empty, |o| &o.env);
        let pend_env = pending.roles.get(role).map_or(&empty, |p| &p.env);
        let scope = EnvScope::WorkspaceRole {
            workspace: workspace_name.to_owned(),
            role: role.clone(),
        };
        apply_env_map_diff(editor_doc, &scope, orig_env, pend_env)?;

        let orig_role_github_env = original
            .roles
            .get(role)
            .and_then(|o| o.github.as_ref())
            .map_or(&empty, |g| &g.env);
        let pend_role_github_env = pending
            .roles
            .get(role)
            .and_then(|p| p.github.as_ref())
            .map_or(&empty, |g| &g.env);
        let role_github_scope = EnvScope::WorkspaceRoleGithub {
            workspace: workspace_name.to_owned(),
            role: role.clone(),
        };
        apply_env_map_diff(
            editor_doc,
            &role_github_scope,
            orig_role_github_env,
            pend_role_github_env,
        )?;
    }
    Ok(())
}

fn apply_env_map_diff(
    editor_doc: &mut crate::config::ConfigEditor,
    scope: &EnvScope,
    original: &BTreeMap<String, EnvValue>,
    pending: &BTreeMap<String, EnvValue>,
) -> anyhow::Result<()> {
    for (key, value) in pending {
        match original.get(key) {
            Some(original_value) if original_value == value => {}
            _ => {
                editor_doc.set_env_var(scope, key, value.clone())?;
            }
        }
    }
    for key in original.keys() {
        if !pending.contains_key(key) {
            let _ = editor_doc.remove_env_var(scope, key);
        }
    }
    Ok(())
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

#[allow(unfulfilled_lint_expectations)]
#[expect(
    single_use_lifetimes,
    reason = "impl Iterator over borrowed String keys cannot use anonymous lifetimes on stable Rust"
)]
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{WorkspaceSaveInput, WorkspaceSaveMode, save_workspace};
    use crate::config::{AgentAuthConfig, AppConfig, CURRENT_WORKSPACE_VERSION};
    use crate::isolation::MountIsolation;
    use crate::paths::JackinPaths;
    use crate::workspace::{MountConfig, WorkspaceConfig, WorkspaceRoleOverride};

    fn workspace_file_contents(paths: &JackinPaths, name: &str) -> String {
        std::fs::read_to_string(paths.workspaces_dir.join(format!("{name}.toml"))).unwrap()
    }

    #[test]
    fn save_workspace_persists_and_clears_workspace_and_role_sync_source_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let mount_src = tmp.path().join("repo");
        std::fs::create_dir(&mount_src).unwrap();
        let original = WorkspaceConfig {
            version: CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: "/workspace/proj".to_owned(),
            mounts: vec![MountConfig {
                src: mount_src.display().to_string(),
                dst: "/workspace/proj".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            ..WorkspaceConfig::default()
        };
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        config
            .workspaces
            .insert("proj".to_owned(), original.clone());
        std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

        let workspace_source = PathBuf::from("/host/claude");
        let role_source = PathBuf::from("/host/codex");
        let mut pending = original.clone();
        pending.claude = Some(AgentAuthConfig {
            sync_source_dir: Some(workspace_source.clone()),
            ..Default::default()
        });
        pending.roles.insert(
            "smith".to_owned(),
            WorkspaceRoleOverride {
                codex: Some(AgentAuthConfig {
                    sync_source_dir: Some(role_source.clone()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );

        let saved = save_workspace(
            &paths,
            WorkspaceSaveInput {
                mode: WorkspaceSaveMode::Edit {
                    original_name: "proj".to_owned(),
                    pending_name: None,
                    effective_removals: Vec::new(),
                },
                original: &original,
                pending: &pending,
            },
        )
        .unwrap();

        let reloaded = saved.config.workspaces.get("proj").unwrap();
        assert_eq!(
            reloaded
                .claude
                .as_ref()
                .and_then(|c| c.sync_source_dir.clone()),
            Some(workspace_source)
        );
        assert_eq!(
            reloaded
                .roles
                .get("smith")
                .and_then(|r| r.codex.as_ref())
                .and_then(|c| c.sync_source_dir.clone()),
            Some(role_source)
        );

        let mut cleared = reloaded.clone();
        cleared.claude = None;
        cleared.roles.clear();
        save_workspace(
            &paths,
            WorkspaceSaveInput {
                mode: WorkspaceSaveMode::Edit {
                    original_name: "proj".to_owned(),
                    pending_name: None,
                    effective_removals: Vec::new(),
                },
                original: reloaded,
                pending: &cleared,
            },
        )
        .unwrap();

        let reloaded = AppConfig::load_or_init(&paths).unwrap();
        let workspace = reloaded.workspaces.get("proj").unwrap();
        assert!(workspace.claude.is_none());
        assert!(workspace.roles.is_empty());

        let out = workspace_file_contents(&paths, "proj");
        assert!(!out.contains("sync_source_dir"), "{out}");
    }
}

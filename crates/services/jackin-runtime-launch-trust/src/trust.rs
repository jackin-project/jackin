// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;

pub const MISE_TRUSTED_CONFIG_PATHS_ENV: &str = "MISE_TRUSTED_CONFIG_PATHS";

/// Container paths Codex must trust without prompting: the workspace
/// workdir plus every mount destination. Shared with the account
/// publication path so the mounted generation carries the same trust
/// the session home seed writes.
pub fn workspace_trusted_project_paths(
    workspace: &jackin_config::ResolvedWorkspace,
) -> std::collections::BTreeSet<String> {
    let mut paths = std::collections::BTreeSet::new();
    if !workspace.workdir.trim().is_empty() {
        paths.insert(workspace.workdir.clone());
    }
    for mount in &workspace.mounts {
        if !mount.dst.trim().is_empty() {
            paths.insert(mount.dst.clone());
        }
    }
    paths
}

pub fn workspace_mise_trusted_config_paths(
    workspace: &jackin_config::ResolvedWorkspace,
) -> Option<String> {
    let paths = workspace_trusted_project_paths(workspace);
    (!paths.is_empty()).then(|| paths.into_iter().collect::<Vec<_>>().join(":"))
}

pub fn inject_workspace_mise_env(
    vars: &mut Vec<(String, String)>,
    workspace: &jackin_config::ResolvedWorkspace,
) {
    if vars
        .iter()
        .any(|(key, _)| key == MISE_TRUSTED_CONFIG_PATHS_ENV)
    {
        return;
    }

    if let Some(value) = workspace_mise_trusted_config_paths(workspace) {
        vars.push((MISE_TRUSTED_CONFIG_PATHS_ENV.to_owned(), value));
    }
}

#[expect(
    clippy::expect_used,
    reason = "toml_edit item is normalized to a table immediately before borrowing it"
)]
fn ensure_table<'a>(table: &'a mut toml_edit::Table, key: &str) -> &'a mut toml_edit::Table {
    let item = table
        .entry(key)
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
    if !item.is_table() {
        *item = toml_edit::Item::Table(toml_edit::Table::new());
    }
    item.as_table_mut()
        .expect("item was just normalized to a table")
}

pub fn seed_codex_project_trust(
    state: &jackin_instance::RoleState,
    workspace: &jackin_config::ResolvedWorkspace,
) -> anyhow::Result<()> {
    let trusted_paths = workspace_trusted_project_paths(workspace);
    if trusted_paths.is_empty() {
        return Ok(());
    }

    for slot in state
        .auth
        .slots
        .values()
        .filter(|slot| slot.agent == jackin_core::Agent::Codex)
    {
        let config_path = state
            .root
            .join("home")
            .join(&slot.container_home_rel)
            .join("config.toml");
        let raw = match std::fs::read_to_string(&config_path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("reading Codex config at {}", config_path.display()));
            }
        };
        let mut doc: toml_edit::DocumentMut = if raw.trim().is_empty() {
            toml_edit::DocumentMut::new()
        } else {
            raw.parse()
                .with_context(|| format!("parsing Codex config at {}", config_path.display()))?
        };

        let projects = ensure_table(doc.as_table_mut(), "projects");
        for path in &trusted_paths {
            ensure_table(projects, path).insert("trust_level", toml_edit::value("trusted"));
        }

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("creating Codex config directory at {}", parent.display())
            })?;
        }
        std::fs::write(&config_path, doc.to_string())
            .with_context(|| format!("writing Codex config at {}", config_path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

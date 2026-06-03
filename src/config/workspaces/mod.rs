//! Workspace drift detection: find isolated mounts whose src changed while containers are running.
//!
//! Classifies drifted records into `running_containers` (edit blocked) and
//! `stopped_records` (requires `--delete-isolated-state`). Not responsible
//! for applying edits or removing isolation records — callers handle that
//! after inspecting the returned `DriftDetection`.

use super::AppConfig;
use crate::isolation::state::{IsolationRecord, list_records_for_workspace};
use crate::workspace::{WorkspaceConfig, WorkspaceEdit, validate_workspace_config};
use anyhow::Context as _;

/// Outcome of a pre-edit drift check for a saved workspace.
///
/// `running_containers` are containers that are still running and have
/// preserved isolated state for a mount whose `src` would be changed by the
/// edit. The CLI rejects the edit unconditionally — the operator must eject
/// before re-editing.
///
/// `stopped_records` are the corresponding records on stopped containers.
/// The CLI requires `--delete-isolated-state` to drop them before applying
/// the edit.
#[derive(Debug, Clone, Default)]
pub struct DriftDetection {
    pub running_containers: Vec<String>,
    pub stopped_records: Vec<IsolationRecord>,
}

/// Classify isolation drift across every container that holds preserved
/// state for `workspace_name`.
///
/// A record drifts when its mount destination is no longer present in the
/// edited mounts, or when the new `src` differs from the `original_src`
/// recorded at materialization time. Drifted records on running containers
/// go into `running_containers`; the rest land in `stopped_records`.
pub async fn detect_workspace_edit_drift(
    paths: &crate::paths::JackinPaths,
    workspace_name: &str,
    edited_mounts: &[crate::workspace::MountConfig],
    docker: &impl crate::docker_client::DockerApi,
) -> anyhow::Result<DriftDetection> {
    let records = list_records_for_workspace(&paths.data_dir, workspace_name)?;
    if records.is_empty() {
        return Ok(DriftDetection::default());
    }
    let running = crate::runtime::list_role_names(docker, false)
        .await
        .context("listing running containers to check for workspace edit drift")?;

    let mut affected_running = Vec::new();
    let mut affected_stopped = Vec::new();
    for rec in records {
        let edited = edited_mounts.iter().find(|m| m.dst == rec.mount_dst);
        let drifted = edited.is_none_or(|m| m.src != rec.original_src);
        if !drifted {
            continue;
        }
        if running.iter().any(|n| n == &rec.container_name) {
            affected_running.push(rec.container_name.clone());
        } else {
            affected_stopped.push(rec);
        }
    }
    Ok(DriftDetection {
        running_containers: affected_running,
        stopped_records: affected_stopped,
    })
}

impl AppConfig {
    /// Return the workspace named `name`, or an `unknown workspace` error.
    ///
    /// Shared by every CLI and runtime site that needs to look up a saved
    /// workspace by name and error on miss. The error message shape is
    /// part of the CLI contract — do not change it casually.
    pub fn require_workspace(&self, name: &str) -> anyhow::Result<&WorkspaceConfig> {
        self.workspaces
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))
    }

    // pub(crate): ConfigEditor::create_workspace delegates here for validation;
    // external callers must go through ConfigEditor to ensure TOML preservation.
    pub(crate) fn create_workspace(
        &mut self,
        name: &str,
        workspace: WorkspaceConfig,
    ) -> anyhow::Result<()> {
        if self.workspaces.contains_key(name) {
            anyhow::bail!("workspace {name:?} already exists; use `workspace edit`");
        }
        validate_workspace_config(name, &workspace)?;

        // Rule-C invariant: the initial mount list must be pairwise
        // non-covering. All mounts are "new" in a create.
        let all_indexes: Vec<usize> = (0..workspace.mounts.len()).collect();
        match crate::workspace::plan_collapse(&workspace.mounts, &all_indexes) {
            Ok(plan) if plan.removed.is_empty() => {}
            Ok(plan) => {
                let details: Vec<String> = plan
                    .removed
                    .iter()
                    .map(|r| format!("{} covered by {}", r.child.src, r.covered_by.src))
                    .collect();
                anyhow::bail!(
                    "workspace {name:?} initial mounts contain redundant entries:\n  - {}",
                    details.join("\n  - ")
                );
            }
            Err(e) => return Err(e.into()),
        }

        self.workspaces.insert(name.to_string(), workspace);
        Ok(())
    }

    // pub(crate): ConfigEditor::edit_workspace delegates here for validation;
    // external callers must go through ConfigEditor to ensure TOML preservation.
    pub(crate) fn edit_workspace(&mut self, name: &str, edit: WorkspaceEdit) -> anyhow::Result<()> {
        let mut seen_upsert_destinations = std::collections::HashSet::new();
        for mount in &edit.upsert_mounts {
            if !seen_upsert_destinations.insert(mount.dst.as_str()) {
                anyhow::bail!("duplicate workspace edit mount destination: {}", mount.dst);
            }
        }

        let mut workspace = self.require_workspace(name)?.clone();

        if let Some(workdir) = edit.workdir {
            workspace.workdir = workdir;
        }

        for dst in edit.remove_destinations {
            let original_len = workspace.mounts.len();
            workspace.mounts.retain(|mount| mount.dst != dst);
            if workspace.mounts.len() == original_len {
                anyhow::bail!("unknown workspace mount destination: {dst}");
            }
        }

        if edit.no_workdir_mount {
            let workdir = &workspace.workdir;
            let original_len = workspace.mounts.len();
            workspace
                .mounts
                .retain(|mount| !(mount.src == *workdir && mount.dst == *workdir));
            if workspace.mounts.len() == original_len {
                anyhow::bail!("no auto-mounted workdir found (mount where src = dst = {workdir})");
            }
        }

        for mount in edit.upsert_mounts {
            if let Some(existing) = workspace
                .mounts
                .iter_mut()
                .find(|existing| existing.dst == mount.dst)
            {
                *existing = mount;
            } else {
                workspace.mounts.push(mount);
            }
        }

        crate::workspace::planner::apply_isolation_overrides(
            &mut workspace.mounts,
            &edit.mount_isolation_overrides,
        )?;

        for selector in edit.allowed_roles_to_add {
            if !workspace
                .allowed_roles
                .iter()
                .any(|existing| existing == &selector)
            {
                workspace.allowed_roles.push(selector);
            }
        }

        for selector in edit.allowed_roles_to_remove {
            workspace
                .allowed_roles
                .retain(|existing| existing != &selector);
        }

        if let Some(default_role) = edit.default_role {
            workspace.default_role = default_role;
        }

        if let Some(default_agent) = edit.default_agent {
            workspace.default_agent = default_agent;
        }

        if let Some(enabled) = edit.keep_awake_enabled {
            workspace.keep_awake.enabled = enabled;
        }

        if let Some(enabled) = edit.git_pull_on_entry_enabled {
            workspace.git_pull_on_entry = enabled;
        }

        // Rule-C invariant: after applying this edit, the mount list must be
        // pairwise non-covering under rule C. The CLI layer pre-collapses
        // redundants; if any remain here, the caller is buggy (non-CLI) or
        // the workspace has a pre-existing violation that wasn't cleaned up.
        //
        // Re-run plan_collapse with empty new_indexes: any removal indicates
        // a violation is present, whether freshly introduced or pre-existing.
        match crate::workspace::plan_collapse(&workspace.mounts, &[]) {
            Ok(plan) if plan.removed.is_empty() => {}
            Ok(plan) => {
                let details: Vec<String> = plan
                    .removed
                    .iter()
                    .map(|r| format!("{} covered by {}", r.child.src, r.covered_by.src))
                    .collect();
                anyhow::bail!(
                    "workspace {name:?} would contain redundant mounts after this edit:\n  - {}\n\
                     use `jackin workspace prune {name}` or pass `--prune` to clean up",
                    details.join("\n  - ")
                );
            }
            Err(e) => return Err(e.into()),
        }

        validate_workspace_config(name, &workspace)?;
        self.workspaces.insert(name.to_string(), workspace);
        Ok(())
    }

    // pub(crate): production callers use ConfigEditor::remove_workspace (which
    // deletes the TOML table directly); this stays for the test in workspaces.rs
    // that validates the error message shape.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn remove_workspace(&mut self, name: &str) -> anyhow::Result<()> {
        self.workspaces
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))
    }

    pub fn list_workspaces(&self) -> Vec<(&str, &WorkspaceConfig)> {
        self.workspaces
            .iter()
            .map(|(name, workspace)| (name.as_str(), workspace))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn insert_workspace_raw(&mut self, name: &str, ws: WorkspaceConfig) {
        self.workspaces.insert(name.into(), ws);
    }

    pub(super) fn validate_workspaces(&self) -> anyhow::Result<()> {
        for (name, workspace) in &self.workspaces {
            validate_workspace_config(name, workspace)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

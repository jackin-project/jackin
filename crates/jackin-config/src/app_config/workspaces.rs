// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `AppConfig` workspace CRUD impl blocks.

use super::AppConfig;
use crate::planner::plan_collapse;
use crate::schema::{WorkspaceConfig, WorkspaceEdit};
use crate::validation::validate_workspace_config;

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
    pub fn create_workspace(
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
        match plan_collapse(&workspace.mounts, &all_indexes) {
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
            Err(e) => return Err(anyhow::anyhow!("{e}")),
        }

        self.workspaces.insert(name.to_owned(), workspace);
        Ok(())
    }

    // pub(crate): ConfigEditor::edit_workspace delegates here for validation;
    // external callers must go through ConfigEditor to ensure TOML preservation.
    pub fn edit_workspace(&mut self, name: &str, edit: WorkspaceEdit) -> anyhow::Result<()> {
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

        crate::planner::apply_isolation_overrides(
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
        match plan_collapse(&workspace.mounts, &[]) {
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
            Err(e) => return Err(anyhow::anyhow!("{e}")),
        }

        validate_workspace_config(name, &workspace)?;
        self.workspaces.insert(name.to_owned(), workspace);
        Ok(())
    }

    // pub(crate): production callers use ConfigEditor::remove_workspace (which
    // deletes the TOML table directly); this stays for the test in workspaces.rs
    // that validates the error message shape.
    pub fn remove_workspace(&mut self, name: &str) -> anyhow::Result<()> {
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

    pub fn insert_workspace_raw(&mut self, name: &str, ws: WorkspaceConfig) {
        self.workspaces.insert(name.into(), ws);
    }

    pub fn validate_workspaces(&self) -> anyhow::Result<()> {
        for (name, workspace) in &self.workspaces {
            validate_workspace_config(name, workspace)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

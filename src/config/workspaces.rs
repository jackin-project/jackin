use super::AppConfig;
use crate::workspace::{WorkspaceConfig, WorkspaceEdit, validate_workspace_config};

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

        for selector in edit.allowed_agents_to_add {
            if !workspace
                .allowed_agents
                .iter()
                .any(|existing| existing == &selector)
            {
                workspace.allowed_agents.push(selector);
            }
        }

        for selector in edit.allowed_agents_to_remove {
            workspace
                .allowed_agents
                .retain(|existing| existing != &selector);
        }

        if let Some(default_agent) = edit.default_agent {
            workspace.default_agent = default_agent;
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
mod tests {
    use super::*;
    use crate::workspace::MountConfig;
    use tempfile::tempdir;

    #[test]
    fn edit_workspace_leaves_original_value_when_validation_fails() {
        let temp = tempdir().unwrap();
        let mut config = AppConfig::default();
        let original = WorkspaceConfig {
            workdir: "/workspace/project".to_string(),
            mounts: vec![MountConfig {
                src: temp.path().display().to_string(),
                dst: "/workspace/project".to_string(),
                readonly: false,
            }],
            allowed_agents: vec!["agent-smith".to_string()],
            default_agent: Some("agent-smith".to_string()),
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        config
            .create_workspace("big-monorepo", original.clone())
            .unwrap();

        let err = config
            .edit_workspace(
                "big-monorepo",
                WorkspaceEdit {
                    workdir: Some("/workspace/elsewhere".to_string()),
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        assert!(err.to_string().contains(
            "must be equal to, inside, or a parent of one of the workspace mount destinations"
        ));
        assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
    }

    #[test]
    fn create_workspace_rejects_duplicate_name_and_preserves_existing_value() {
        let temp = tempdir().unwrap();
        let mut config = AppConfig::default();
        let original = WorkspaceConfig {
            workdir: "/workspace/project".to_string(),
            mounts: vec![MountConfig {
                src: temp.path().display().to_string(),
                dst: "/workspace/project".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        config
            .create_workspace("big-monorepo", original.clone())
            .unwrap();

        let err = config
            .create_workspace(
                "big-monorepo",
                WorkspaceConfig {
                    workdir: "/workspace/other".to_string(),
                    mounts: vec![MountConfig {
                        src: temp.path().display().to_string(),
                        dst: "/workspace/other".to_string(),
                        readonly: true,
                    }],
                    allowed_agents: vec!["agent-smith".to_string()],
                    default_agent: Some("agent-smith".to_string()),
                    last_agent: None,
                    env: std::collections::BTreeMap::new(),
                    agents: std::collections::BTreeMap::new(),
                },
            )
            .unwrap_err();

        assert!(err.to_string().contains("already exists"));
        assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
    }

    #[test]
    fn edit_workspace_rejects_duplicate_upsert_destinations() {
        let temp = tempdir().unwrap();
        let original_src = temp.path().join("project");
        let first_upsert = temp.path().join("cache-a");
        let second_upsert = temp.path().join("cache-b");
        std::fs::create_dir_all(&original_src).unwrap();
        std::fs::create_dir_all(&first_upsert).unwrap();
        std::fs::create_dir_all(&second_upsert).unwrap();

        let mut config = AppConfig::default();
        let original = WorkspaceConfig {
            workdir: "/workspace/project".to_string(),
            mounts: vec![MountConfig {
                src: original_src.display().to_string(),
                dst: "/workspace/project".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        config
            .create_workspace("big-monorepo", original.clone())
            .unwrap();

        let err = config
            .edit_workspace(
                "big-monorepo",
                WorkspaceEdit {
                    upsert_mounts: vec![
                        MountConfig {
                            src: first_upsert.display().to_string(),
                            dst: "/workspace/cache".to_string(),
                            readonly: false,
                        },
                        MountConfig {
                            src: second_upsert.display().to_string(),
                            dst: "/workspace/cache".to_string(),
                            readonly: true,
                        },
                    ],
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("duplicate workspace edit mount destination")
        );
        assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
    }

    #[test]
    fn edit_workspace_rejects_missing_remove_destination() {
        let temp = tempdir().unwrap();
        let original_src = temp.path().join("project");
        std::fs::create_dir_all(&original_src).unwrap();

        let mut config = AppConfig::default();
        let original = WorkspaceConfig {
            workdir: "/workspace/project".to_string(),
            mounts: vec![MountConfig {
                src: original_src.display().to_string(),
                dst: "/workspace/project".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        config
            .create_workspace("big-monorepo", original.clone())
            .unwrap();

        let err = config
            .edit_workspace(
                "big-monorepo",
                WorkspaceEdit {
                    remove_destinations: vec!["/workspace/missing".to_string()],
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("unknown workspace mount destination")
        );
        assert_eq!(config.workspaces.get("big-monorepo").unwrap(), &original);
    }

    #[test]
    fn remove_workspace_errors_when_missing() {
        let mut config = AppConfig::default();

        let err = config.remove_workspace("missing").unwrap_err();

        assert!(err.to_string().contains("unknown workspace missing"));
    }
}

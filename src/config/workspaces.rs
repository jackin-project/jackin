use super::AppConfig;
use crate::isolation::state::{IsolationRecord, list_records_for_workspace};
use crate::workspace::{WorkspaceConfig, WorkspaceEdit, validate_workspace_config};

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
#[derive(Debug, Clone)]
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
pub fn detect_workspace_edit_drift(
    paths: &crate::paths::JackinPaths,
    workspace_name: &str,
    edited_mounts: &[crate::workspace::MountConfig],
    runner: &mut impl crate::docker::CommandRunner,
) -> anyhow::Result<DriftDetection> {
    let records = list_records_for_workspace(&paths.data_dir, workspace_name)?;
    let running = crate::runtime::list_role_names(runner, false).unwrap_or_default();

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
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_roles: vec!["agent-smith".to_string()],
            default_role: Some("agent-smith".to_string()),
            default_agent: None,
            last_role: None,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: crate::workspace::KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            git_pull_on_entry: false,
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
    fn edit_workspace_toggles_keep_awake_when_set() {
        // Round-trip: enable, disable, no-change. The Option<bool> shape
        // distinguishes "user touched the field" from "user said nothing
        // about it", which is the whole point of the field type.
        let temp = tempdir().unwrap();
        let mut config = AppConfig::default();
        config
            .create_workspace(
                "my-app",
                WorkspaceConfig {
                    workdir: "/workspace/proj".to_string(),
                    mounts: vec![MountConfig {
                        src: temp.path().display().to_string(),
                        dst: "/workspace/proj".to_string(),
                        readonly: false,
                        isolation: crate::isolation::MountIsolation::Shared,
                    }],
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(!config.workspaces.get("my-app").unwrap().keep_awake.enabled);

        config
            .edit_workspace(
                "my-app",
                WorkspaceEdit {
                    keep_awake_enabled: Some(true),
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap();
        assert!(config.workspaces.get("my-app").unwrap().keep_awake.enabled);

        // Subsequent edit with no keep_awake change must leave the
        // field alone — this is the contract that lets `workspace edit
        // --workdir` not silently flip power-management state.
        config
            .edit_workspace(
                "my-app",
                WorkspaceEdit {
                    workdir: Some("/workspace/proj".to_string()),
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap();
        assert!(
            config.workspaces.get("my-app").unwrap().keep_awake.enabled,
            "unrelated edits must not flip keep_awake",
        );

        config
            .edit_workspace(
                "my-app",
                WorkspaceEdit {
                    keep_awake_enabled: Some(false),
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap();
        assert!(!config.workspaces.get("my-app").unwrap().keep_awake.enabled);
    }

    #[test]
    fn edit_workspace_sets_and_clears_agent() {
        let temp = tempdir().unwrap();
        let mut config = AppConfig::default();
        config
            .create_workspace(
                "my-app",
                WorkspaceConfig {
                    workdir: "/workspace/proj".to_string(),
                    mounts: vec![MountConfig {
                        src: temp.path().display().to_string(),
                        dst: "/workspace/proj".to_string(),
                        readonly: false,
                        isolation: crate::isolation::MountIsolation::Shared,
                    }],
                    ..Default::default()
                },
            )
            .unwrap();

        config
            .edit_workspace(
                "my-app",
                WorkspaceEdit {
                    default_agent: Some(Some(crate::agent::Agent::Codex)),
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap();
        assert_eq!(
            config.workspaces.get("my-app").unwrap().default_agent,
            Some(crate::agent::Agent::Codex)
        );

        config
            .edit_workspace(
                "my-app",
                WorkspaceEdit {
                    workdir: Some("/workspace/proj".to_string()),
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap();
        assert_eq!(
            config.workspaces.get("my-app").unwrap().default_agent,
            Some(crate::agent::Agent::Codex),
            "unrelated edits must not clear default_agent"
        );

        config
            .edit_workspace(
                "my-app",
                WorkspaceEdit {
                    default_agent: Some(None),
                    ..WorkspaceEdit::default()
                },
            )
            .unwrap();
        assert_eq!(config.workspaces.get("my-app").unwrap().default_agent, None);
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
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..Default::default()
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
                        isolation: crate::isolation::MountIsolation::Shared,
                    }],
                    allowed_roles: vec!["agent-smith".to_string()],
                    default_role: Some("agent-smith".to_string()),
                    ..Default::default()
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
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..Default::default()
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
                            isolation: crate::isolation::MountIsolation::Shared,
                        },
                        MountConfig {
                            src: second_upsert.display().to_string(),
                            dst: "/workspace/cache".to_string(),
                            readonly: true,
                            isolation: crate::isolation::MountIsolation::Shared,
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
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..Default::default()
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

    mod drift_detection {
        use super::super::*;
        use crate::isolation::MountIsolation;
        use crate::isolation::state::{CleanupStatus, IsolationRecord, write_records};
        use crate::paths::JackinPaths;
        use crate::runtime::test_support::FakeRunner;
        use tempfile::TempDir;

        fn record_for(workspace: &str, container: &str, dst: &str, src: &str) -> IsolationRecord {
            IsolationRecord {
                workspace: workspace.into(),
                mount_dst: dst.into(),
                original_src: src.into(),
                isolation: MountIsolation::Worktree,
                worktree_path: format!("/data/{container}/isolated{dst}"),
                scratch_branch: format!("jackin/scratch/{container}"),
                base_commit: "abc".into(),
                selector_key: container.trim_start_matches("jackin-").into(),
                container_name: container.into(),
                cleanup_status: CleanupStatus::Active,
            }
        }

        fn paths_for(data: &std::path::Path) -> JackinPaths {
            JackinPaths {
                home_dir: data.into(),
                config_dir: data.into(),
                config_file: data.join("config.toml"),
                roles_dir: data.into(),
                data_dir: data.into(),
                cache_dir: data.into(),
            }
        }

        fn mount(src: &str, dst: &str, iso: MountIsolation) -> crate::workspace::MountConfig {
            crate::workspace::MountConfig {
                src: src.into(),
                dst: dst.into(),
                readonly: false,
                isolation: iso,
            }
        }

        #[test]
        fn detect_drift_flags_running_containers() {
            let data = TempDir::new().unwrap();
            let cdir = data.path().join("jackin-x");
            std::fs::create_dir_all(&cdir).unwrap();
            write_records(
                &cdir,
                std::slice::from_ref(&record_for(
                    "jackin",
                    "jackin-x",
                    "/workspace/jackin",
                    "/old/src",
                )),
            )
            .unwrap();

            let paths = paths_for(data.path());
            let edited = vec![mount(
                "/new/src",
                "/workspace/jackin",
                MountIsolation::Worktree,
            )];
            let mut runner = FakeRunner::default();
            runner.capture_queue.push_back("jackin-x\n".into());
            runner.capture_queue.push_back(String::new());
            let det = detect_workspace_edit_drift(&paths, "jackin", &edited, &mut runner).unwrap();
            assert_eq!(det.running_containers, vec!["jackin-x".to_string()]);
            assert!(det.stopped_records.is_empty());
        }

        #[test]
        fn detect_drift_flags_stopped_records_when_src_changes() {
            let data = TempDir::new().unwrap();
            let cdir = data.path().join("jackin-x");
            std::fs::create_dir_all(&cdir).unwrap();
            write_records(
                &cdir,
                std::slice::from_ref(&record_for(
                    "jackin",
                    "jackin-x",
                    "/workspace/jackin",
                    "/old/src",
                )),
            )
            .unwrap();

            let paths = paths_for(data.path());
            let edited = vec![mount(
                "/new/src",
                "/workspace/jackin",
                MountIsolation::Worktree,
            )];
            let mut runner = FakeRunner::default();
            runner.capture_queue.push_back(String::new());
            runner.capture_queue.push_back(String::new());
            let det = detect_workspace_edit_drift(&paths, "jackin", &edited, &mut runner).unwrap();
            assert!(det.running_containers.is_empty());
            assert_eq!(det.stopped_records.len(), 1);
            assert_eq!(det.stopped_records[0].container_name, "jackin-x");
        }

        #[test]
        fn detect_drift_quiet_when_src_unchanged() {
            let data = TempDir::new().unwrap();
            let cdir = data.path().join("jackin-x");
            std::fs::create_dir_all(&cdir).unwrap();
            write_records(
                &cdir,
                std::slice::from_ref(&record_for(
                    "jackin",
                    "jackin-x",
                    "/workspace/jackin",
                    "/same/src",
                )),
            )
            .unwrap();

            let paths = paths_for(data.path());
            let edited = vec![mount(
                "/same/src",
                "/workspace/jackin",
                MountIsolation::Worktree,
            )];
            let mut runner = FakeRunner::default();
            runner.capture_queue.push_back(String::new());
            runner.capture_queue.push_back(String::new());
            let det = detect_workspace_edit_drift(&paths, "jackin", &edited, &mut runner).unwrap();
            assert!(det.running_containers.is_empty());
            assert!(det.stopped_records.is_empty());
        }

        /// Documents a known V1 limitation: flipping the isolation mode
        /// from `worktree` to `shared` on the same `dst`+`src` does NOT
        /// fire drift detection today. The existing isolation.json
        /// record + materialized worktree become stranded silently;
        /// they're only reclaimed by `jackin purge`. Pinning this here
        /// so a future change that extends the drift predicate
        /// (proposed in code review of PR #177) updates this test in
        /// the same change instead of accidentally regressing on it.
        #[test]
        fn detect_drift_does_not_currently_flag_isolation_mode_flips() {
            let data = TempDir::new().unwrap();
            let cdir = data.path().join("jackin-x");
            std::fs::create_dir_all(&cdir).unwrap();
            write_records(
                &cdir,
                std::slice::from_ref(&record_for(
                    "jackin",
                    "jackin-x",
                    "/workspace/jackin",
                    "/same/src",
                )),
            )
            .unwrap();

            let paths = paths_for(data.path());
            // Same src+dst as the recorded mount, but isolation flipped.
            let edited = vec![mount(
                "/same/src",
                "/workspace/jackin",
                MountIsolation::Shared,
            )];
            let mut runner = FakeRunner::default();
            runner.capture_queue.push_back(String::new());
            runner.capture_queue.push_back(String::new());
            let det = detect_workspace_edit_drift(&paths, "jackin", &edited, &mut runner).unwrap();
            // Current behavior — known gap. If this test starts failing
            // because drift now correctly flags the flip, update it to
            // assert `det.stopped_records.len() == 1` and remove this
            // explanatory note.
            assert!(
                det.stopped_records.is_empty(),
                "current V1 behavior: isolation-mode flips don't fire drift; \
                 update this test when the predicate is extended"
            );
        }

        /// Operator removes the mount entirely from the workspace edit
        /// (or renames its dst). The existing record's dst is no longer
        /// in `edited_mounts`, so drift fires — operator must
        /// acknowledge with `--delete-isolated-state`.
        #[test]
        fn detect_drift_flags_record_when_dst_removed_from_edit() {
            let data = TempDir::new().unwrap();
            let cdir = data.path().join("jackin-x");
            std::fs::create_dir_all(&cdir).unwrap();
            write_records(
                &cdir,
                std::slice::from_ref(&record_for(
                    "jackin",
                    "jackin-x",
                    "/workspace/jackin",
                    "/old/src",
                )),
            )
            .unwrap();

            let paths = paths_for(data.path());
            // Edited mount list omits /workspace/jackin entirely.
            let edited = vec![mount(
                "/some/other/src",
                "/workspace/other",
                MountIsolation::Shared,
            )];
            let mut runner = FakeRunner::default();
            runner.capture_queue.push_back(String::new());
            runner.capture_queue.push_back(String::new());
            let det = detect_workspace_edit_drift(&paths, "jackin", &edited, &mut runner).unwrap();
            assert!(det.running_containers.is_empty());
            assert_eq!(
                det.stopped_records.len(),
                1,
                "removing the dst from the workspace must surface the existing record as drift",
            );
            assert_eq!(det.stopped_records[0].mount_dst, "/workspace/jackin");
        }
    }
}

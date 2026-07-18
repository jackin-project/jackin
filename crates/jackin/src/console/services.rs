// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console side-effect adapters.

pub(super) mod agents {
    pub(crate) async fn resolve_supported_for_console(
        paths: &jackin_core::JackinPaths,
        config: &jackin_config::AppConfig,
        role: &jackin_core::RoleSelector,
        runner: &mut impl jackin_docker::CommandRunner,
    ) -> anyhow::Result<Vec<jackin_core::Agent>> {
        jackin_runtime::runtime::resolve_supported_agents_for_console(paths, config, role, runner)
            .await
    }

    pub(crate) async fn load_inline_picker_choices(
        paths: &jackin_core::JackinPaths,
        config: &jackin_config::AppConfig,
        role: &jackin_core::RoleSelector,
        runner: &mut impl jackin_docker::CommandRunner,
    ) -> anyhow::Result<Option<Vec<jackin_core::Agent>>> {
        let agents = resolve_supported_for_console(paths, config, role, runner).await?;
        if agents.len() < 2 {
            return Ok(None);
        }
        Ok(Some(agents))
    }
}
pub(super) mod config {
    //! Non-TUI config persistence services.

    use jackin_config::GlobalMountRow;
    use jackin_config::WorkspaceConfig;
    use jackin_config::{AppConfig, RoleSource};
    use jackin_console::services::config_save::{
        WorkspaceSaveDiffOp, build_workspace_edit, workspace_save_diff_plan,
    };
    use jackin_core::JackinPaths;
    use jackin_core::WorkspaceName;

    pub(crate) use jackin_console::services::config_save::{SettingsSaveInput, save_settings};

    #[cfg(test)]
    mod tests;

    #[cfg(test)]
    pub(crate) fn upsert_role_source(
        config: &mut AppConfig,
        paths: &JackinPaths,
        key: &str,
        source: &RoleSource,
    ) -> anyhow::Result<()> {
        *config = upsert_role_source_on_disk(paths, key, source)?;
        Ok(())
    }

    fn upsert_role_source_on_disk(
        paths: &JackinPaths,
        key: &str,
        source: &RoleSource,
    ) -> anyhow::Result<AppConfig> {
        let mut editor_doc = jackin_config::ConfigEditor::open(paths)?;
        editor_doc.upsert_agent_source(key, source);
        Ok(editor_doc.save()?)
    }

    pub(crate) fn start_role_source_persist(
        paths: JackinPaths,
        origin: jackin_console::tui::subscriptions::RoleSourcePersistOrigin<RoleSource>,
    ) -> jackin_console::tui::runtime::BlockingSubscription<
        jackin_console::tui::state::ManagerConfigSaveResult,
    > {
        let (key, source) = match &origin {
            jackin_console::tui::subscriptions::RoleSourcePersistOrigin::RoleLoad {
                key,
                source,
                ..
            }
            | jackin_console::tui::subscriptions::RoleSourcePersistOrigin::TrustConfirm {
                key,
                source,
            } => (key.clone(), source.clone()),
        };
        jackin_console::tui::runtime::spawn_blocking_subscription(move || {
            let result = upsert_role_source_on_disk(&paths, &key, &source);
            jackin_console::tui::subscriptions::ConfigSaveResult::RoleSourcePersist {
                result,
                origin,
            }
        })
    }

    fn remove_workspace_from_disk(paths: &JackinPaths, name: &str) -> anyhow::Result<AppConfig> {
        let mut editor_doc = jackin_config::ConfigEditor::open(paths)?;
        editor_doc.remove_workspace(&WorkspaceName::parse(name).map_err(anyhow::Error::from)?)?;
        Ok(editor_doc.save()?)
    }

    pub(crate) fn start_remove_workspace(
        paths: JackinPaths,
        cwd: std::path::PathBuf,
        name: String,
    ) -> jackin_console::tui::runtime::BlockingSubscription<
        jackin_console::tui::state::ManagerConfigSaveResult,
    > {
        jackin_console::tui::runtime::spawn_blocking_subscription(move || {
            let result = remove_workspace_from_disk(&paths, &name);
            jackin_console::tui::subscriptions::ConfigSaveResult::RemoveWorkspace { result, cwd }
        })
    }

    #[cfg(test)]
    pub(crate) fn save_global_mounts(
        paths: &JackinPaths,
        original: &[GlobalMountRow],
        pending: &[GlobalMountRow],
    ) -> anyhow::Result<AppConfig> {
        AppConfig::validate_global_mount_rows(pending)?;
        let mut editor_doc = jackin_config::ConfigEditor::open(paths)?;
        for row in original {
            editor_doc.remove_mount(&row.name, row.scope.as_deref());
        }
        for row in pending {
            editor_doc.add_mount(&row.name, row.mount.clone(), row.scope.as_deref());
        }
        Ok(editor_doc.save()?)
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

    #[expect(
        clippy::useless_let_if_seq,
        reason = "documented residual allow; prefer expect when site is lint-true"
    )]
    pub(crate) fn save_workspace(
        paths: &JackinPaths,
        input: WorkspaceSaveInput<'_>,
    ) -> anyhow::Result<WorkspaceSaveResult> {
        let mut editor_doc = jackin_config::ConfigEditor::open(paths)?;
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
                    editor_doc.rename_workspace(
                        &WorkspaceName::parse(&current_name).map_err(anyhow::Error::from)?,
                        &WorkspaceName::parse(&new_name).map_err(anyhow::Error::from)?,
                    )?;
                    current_name.clone_from(&new_name);
                    rename_to = Some(new_name);
                }

                let mut edit = build_workspace_edit(input.original, input.pending);
                edit.remove_destinations = effective_removals;
                editor_doc.edit_workspace(
                    &WorkspaceName::parse(&current_name).map_err(anyhow::Error::from)?,
                    edit,
                )?;
                (rename_to, current_name)
            }
            WorkspaceSaveMode::Create { name } => {
                editor_doc.create_workspace(
                    &WorkspaceName::parse(&name).map_err(anyhow::Error::from)?,
                    input.pending.clone(),
                )?;
                (None, name)
            }
        };

        apply_workspace_save_diff_plan(
            &mut editor_doc,
            &WorkspaceName::parse(&current_name).map_err(anyhow::Error::from)?,
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

    pub(crate) fn start_workspace_save(
        paths: JackinPaths,
        mode: WorkspaceSaveMode,
        original: WorkspaceConfig,
        pending: WorkspaceConfig,
        exit_on_success: bool,
    ) -> jackin_console::tui::runtime::BlockingSubscription<
        jackin_console::tui::state::ManagerConfigSaveResult,
    > {
        jackin_console::tui::runtime::spawn_blocking_subscription(move || {
            let result = save_workspace(
                &paths,
                WorkspaceSaveInput {
                    mode,
                    original: &original,
                    pending: &pending,
                },
            )
            .map(
                |saved| jackin_console::tui::subscriptions::WorkspaceSaveResult {
                    config: saved.config,
                    current_name: saved.current_name,
                    pending_rename: saved.pending_rename,
                },
            );
            jackin_console::tui::subscriptions::ConfigSaveResult::Workspace {
                result,
                exit_on_success,
            }
        })
    }

    pub(crate) struct OwnedSettingsSaveInput {
        pub mounts_original: Vec<GlobalMountRow>,
        pub mounts_pending: Vec<GlobalMountRow>,
        pub env_original: jackin_console::tui::state::SettingsEnvConfig,
        pub env_pending: jackin_console::tui::state::SettingsEnvConfig,
        pub auth_pending: Vec<jackin_console::tui::state::SettingsAuthRow>,
        pub original_github_env: std::collections::BTreeMap<String, jackin_core::EnvValue>,
        pub github_env: std::collections::BTreeMap<String, jackin_core::EnvValue>,
        pub trust_pending: Vec<jackin_console::tui::state::SettingsTrustRow>,
        pub git_coauthor_trailer: bool,
        pub git_dco: bool,
    }

    impl OwnedSettingsSaveInput {
        fn as_borrowed(&self) -> SettingsSaveInput<'_> {
            SettingsSaveInput {
                mounts_original: &self.mounts_original,
                mounts_pending: &self.mounts_pending,
                env_original: &self.env_original,
                env_pending: &self.env_pending,
                auth_pending: &self.auth_pending,
                original_github_env: &self.original_github_env,
                github_env: &self.github_env,
                trust_pending: &self.trust_pending,
                git_coauthor_trailer: self.git_coauthor_trailer,
                git_dco: self.git_dco,
            }
        }
    }

    pub(crate) fn start_settings_save(
        paths: JackinPaths,
        input: OwnedSettingsSaveInput,
    ) -> jackin_console::tui::runtime::BlockingSubscription<
        jackin_console::tui::state::ManagerConfigSaveResult,
    > {
        jackin_console::tui::runtime::spawn_blocking_subscription(move || {
            let result = save_settings(&paths, input.as_borrowed());
            jackin_console::tui::subscriptions::ConfigSaveResult::Settings(result)
        })
    }

    fn apply_workspace_save_diff_plan(
        editor_doc: &mut jackin_config::ConfigEditor,
        workspace_name: &WorkspaceName,
        original: &WorkspaceConfig,
        pending: &WorkspaceConfig,
    ) -> anyhow::Result<()> {
        for op in workspace_save_diff_plan(workspace_name, original, pending) {
            match op {
                WorkspaceSaveDiffOp::WorkspaceAuthForward { agent, mode } => {
                    editor_doc.set_workspace_auth_forward(workspace_name, agent, mode);
                }
                WorkspaceSaveDiffOp::WorkspaceGithubAuthForward { mode } => {
                    editor_doc.set_workspace_github_auth_forward(workspace_name, mode);
                }
                WorkspaceSaveDiffOp::WorkspaceRoleAuthForward { role, agent, mode } => {
                    editor_doc.set_workspace_role_auth_forward(workspace_name, &role, agent, mode);
                }
                WorkspaceSaveDiffOp::WorkspaceRoleGithubAuthForward { role, mode } => {
                    editor_doc.set_workspace_role_github_auth_forward(workspace_name, &role, mode);
                }
                WorkspaceSaveDiffOp::WorkspaceSyncSourceDir { agent, source } => {
                    editor_doc.set_workspace_sync_source_dir(
                        workspace_name,
                        agent,
                        source.as_deref(),
                    );
                }
                WorkspaceSaveDiffOp::WorkspaceRoleSyncSourceDir {
                    role,
                    agent,
                    source,
                } => {
                    editor_doc.set_workspace_role_sync_source_dir(
                        workspace_name,
                        &role,
                        agent,
                        source.as_deref(),
                    );
                }
                WorkspaceSaveDiffOp::EnvSet { scope, key, value } => {
                    editor_doc.set_env_var(&scope, &key, value)?;
                }
                WorkspaceSaveDiffOp::EnvRemove { scope, key } => {
                    let _ = editor_doc.remove_env_var(&scope, &key);
                }
            }
        }
        Ok(())
    }
}

pub(super) mod instances {
    //! Non-TUI instance discovery services.

    use std::collections::{HashMap, HashSet};

    use anyhow::Context;
    use jackin_console::tui::state::ManagerInstanceRefreshSnapshot;
    use jackin_console::tui::subscriptions::instance_refresh_interval;
    use jackin_runtime::runtime::snapshot::SnapshotTransport;

    type SnapshotFetchResult = (
        String,
        anyhow::Result<(
            Option<jackin_runtime::runtime::snapshot::InstanceSnapshot>,
            SnapshotTransport,
        )>,
    );

    #[cfg(test)]
    mod tests;

    pub(crate) fn load_instance_refresh_snapshot(
        paths: &jackin_core::JackinPaths,
    ) -> Result<ManagerInstanceRefreshSnapshot, String> {
        let index = jackin_runtime::instance::InstanceIndex::read_or_rebuild(&paths.data_dir)
            .map_err(|error| error.to_string())?;
        let mut instances = index.instances;
        let running = running_role_containers_for_refresh(paths, &mut instances);
        let running_filter = running
            .as_ref()
            .map(|containers| containers.iter().cloned().collect::<HashSet<String>>());

        let mut sessions = HashMap::new();
        let mut session_errors = HashSet::new();
        let mut snapshot_targets: Vec<String> = Vec::new();
        let mut recovered_failure = false;

        for entry in &instances {
            if is_live_instance_status(entry.status) {
                let state_dir = paths.data_dir.join(&entry.container_base);
                match jackin_runtime::instance::InstanceManifest::read(&state_dir) {
                    Ok(manifest) if !manifest.sessions.is_empty() => {
                        sessions.insert(entry.container_base.clone(), manifest.sessions);
                    }
                    Ok(_) => {}
                    Err(_) => {
                        recovered_failure = true;
                        session_errors.insert(entry.container_base.clone());
                    }
                }
            }
            if should_snapshot_instance(entry, running_filter.as_ref()) {
                snapshot_targets.push(entry.container_base.clone());
            }
        }

        let mut snapshots = HashMap::new();
        let mut exec_fallback_seen = false;
        let snapshot_results = fetch_snapshots_parallel(paths, &snapshot_targets);
        for (container, result) in snapshot_results {
            match result {
                Ok((Some(snapshot), transport)) => {
                    exec_fallback_seen |= transport == SnapshotTransport::DockerExecFallback;
                    snapshots.insert(container, snapshot);
                }
                Ok((None, transport)) => {
                    exec_fallback_seen |= transport == SnapshotTransport::DockerExecFallback;
                }
                Err(_) => {
                    recovered_failure = true;
                }
            }
        }

        if recovered_failure {
            let _event = jackin_telemetry::record_recovered_degradation();
        }

        Ok(ManagerInstanceRefreshSnapshot {
            instances,
            sessions,
            session_errors,
            snapshots,
            next_interval: instance_refresh_interval(exec_fallback_seen),
        })
    }

    pub(crate) fn running_role_containers() -> anyhow::Result<Vec<String>> {
        let request = jackin_process::ExecRequest::new(
            "docker",
            [
                "ps",
                "--filter",
                "label=jackin.kind=role",
                "--format",
                "{{.Names}}",
            ],
        );
        // Instance refresh is launched through spawn_blocking_subscription;
        // keep the docker listing on the shared process transport.
        let output = crate::process_telemetry::exec_sync(&request)
            .context("starting live instance reconciliation")?;
        anyhow::ensure!(output.success, "live instance reconciliation failed");
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect())
    }

    fn running_role_containers_for_refresh(
        paths: &jackin_core::JackinPaths,
        instances: &mut Vec<jackin_runtime::instance::InstanceIndexEntry>,
    ) -> Option<Vec<String>> {
        let running = match running_role_containers() {
            Ok(running) => running,
            Err(error) => {
                jackin_diagnostics::emit_compact_line(
                    "error",
                    &live_instance_reconciliation_error_line(&format!("{error:#}")),
                );
                return None;
            }
        };
        overlay_running_instances(paths, instances, &running);
        Some(running)
    }

    fn is_live_instance_status(status: jackin_runtime::instance::InstanceStatus) -> bool {
        matches!(
            status,
            jackin_runtime::instance::InstanceStatus::Active
                | jackin_runtime::instance::InstanceStatus::Running
        )
    }

    fn should_snapshot_instance(
        entry: &jackin_runtime::instance::InstanceIndexEntry,
        running_containers: Option<&HashSet<String>>,
    ) -> bool {
        is_live_instance_status(entry.status)
            && running_containers.is_none_or(|running| running.contains(&entry.container_base))
    }

    fn live_instance_reconciliation_error_line(error: &str) -> String {
        format!("jackin: error: live instance reconciliation skipped: docker ps failed: {error}")
    }

    pub(crate) fn overlay_running_instances(
        paths: &jackin_core::JackinPaths,
        instances: &mut Vec<jackin_runtime::instance::InstanceIndexEntry>,
        running_containers: &[String],
    ) {
        if running_containers.is_empty() {
            return;
        }

        let mut known: HashSet<String> = instances
            .iter()
            .map(|entry| entry.container_base.clone())
            .collect();
        for container in running_containers {
            if let Some(entry) = instances
                .iter_mut()
                .find(|entry| entry.container_base == *container)
            {
                entry.status = jackin_runtime::instance::InstanceStatus::Running;
                continue;
            }

            let state_dir = paths.data_dir.join(container);
            let Some(manifest) =
                jackin_runtime::instance::InstanceManifest::read_optional_lossy(&state_dir)
            else {
                continue;
            };
            if !known.insert(container.clone()) {
                continue;
            }
            let mut entry = manifest.to_index_entry();
            entry.status = jackin_runtime::instance::InstanceStatus::Running;
            instances.push(entry);
        }
    }

    #[expect(
        clippy::excessive_nesting,
        reason = "Snapshot fan-out walks chunks of containers, each chunk \
                  spawns a thread, each thread joins a panic-payload match — \
                  the nesting mirrors the chunk → thread → join-result arms. \
                  Flattening requires extracting the per-chunk join to a helper; \
                  deferred-parallel-pass."
    )]
    fn fetch_snapshots_parallel(
        paths: &jackin_core::JackinPaths,
        targets: &[String],
    ) -> Vec<SnapshotFetchResult> {
        const SNAPSHOT_FANOUT_CHUNK: usize = 8;
        let mut results = Vec::with_capacity(targets.len());
        for chunk in targets.chunks(SNAPSHOT_FANOUT_CHUNK) {
            let chunk_results = std::thread::scope(|s| {
                #[expect(
                    clippy::needless_collect,
                    reason = "documented residual allow; prefer expect when site is lint-true"
                )]
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|container| {
                        let container = container.clone();
                        jackin_telemetry::spawn::thread_scoped_joined(s, move || {
                            let result =
                                jackin_runtime::runtime::snapshot::fetch_snapshot_with_transport(
                                    paths, &container,
                                );
                            (container, result)
                        })
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|h| match h.join() {
                        Ok(pair) => pair,
                        Err(panic_payload) => {
                            let detail = panic_payload
                                .downcast_ref::<&'static str>()
                                .map(|s| (*s).to_owned())
                                .or_else(|| panic_payload.downcast_ref::<String>().cloned())
                                .unwrap_or_else(|| "<non-string panic payload>".to_owned());
                            (
                                "<unknown-container>".to_owned(),
                                Err(anyhow::anyhow!("snapshot worker thread panicked: {detail}")),
                            )
                        }
                    })
                    .collect::<Vec<_>>()
            });
            results.extend(chunk_results);
        }
        results
    }
}
pub(super) mod role_load {
    use futures_util::FutureExt as _;
    use jackin_console::tui::runtime::BlockingSubscription;

    pub(crate) fn start_role_registration(
        paths: jackin_core::JackinPaths,
        selector: jackin_core::RoleSelector,
        git_url: String,
    ) -> BlockingSubscription<anyhow::Result<()>> {
        jackin_console::tui::runtime::spawn_named_async_subscription(
            "jackin-role-registration",
            async move {
                let mut runner = jackin_docker::ShellRunner {
                    debug: jackin_diagnostics::is_debug_mode(),
                };
                register_with_runner(
                    &paths,
                    &selector,
                    &git_url,
                    &mut runner,
                    jackin_diagnostics::is_debug_mode(),
                )
                .await
            },
        )
    }

    pub(crate) async fn register_with_runner(
        paths: &jackin_core::JackinPaths,
        selector: &jackin_core::RoleSelector,
        git_url: &str,
        runner: &mut impl jackin_docker::CommandRunner,
        debug: bool,
    ) -> anyhow::Result<()> {
        use jackin_telemetry::ResultTelemetryExt as _;

        let result = std::panic::AssertUnwindSafe(async {
            jackin_runtime::runtime::register_agent_repo(paths, selector, git_url, runner, debug)
                .await?;
            Ok::<_, anyhow::Error>(())
        })
        .catch_unwind()
        .await;

        match result {
            Ok(result) => Ok(result
                .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)?),
            Err(payload) => {
                let _event = jackin_telemetry::record_error(
                    jackin_telemetry::schema::enums::ErrorType::Panic,
                );
                let panic_message = panic_payload_message(payload.as_ref());
                Err(anyhow::anyhow!("role loader panicked: {panic_message}"))
            }
        }
    }

    fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
        if let Some(message) = payload.downcast_ref::<&str>() {
            return (*message).to_owned();
        }
        if let Some(message) = payload.downcast_ref::<String>() {
            return message.clone();
        }
        "role loader panicked with a non-string payload".to_owned()
    }
}

pub(super) mod workspace_save {
    use jackin_console::tui::runtime::BlockingSubscription;

    /// Start the Docker-backed drift check for an edited workspace.
    pub(crate) fn start_drift_check(
        paths: jackin_core::JackinPaths,
        workspace_name: String,
        prospective_mounts: Vec<jackin_config::MountConfig>,
    ) -> BlockingSubscription<anyhow::Result<jackin_runtime::runtime::drift::DriftDetection>> {
        jackin_console::tui::runtime::spawn_named_async_subscription(
            "jackin-drift-check",
            async move {
                async {
                    let docker = jackin_docker::docker_client::BollardDockerClient::connect()?;
                    let wn = jackin_core::WorkspaceName::parse(&workspace_name)
                        .map_err(anyhow::Error::from)?;
                    jackin_runtime::runtime::drift::detect_workspace_edit_drift(
                        &paths,
                        &wn,
                        &prospective_mounts,
                        &docker,
                    )
                    .await
                }
                .await
            },
        )
    }

    /// Start cleanup for isolated mount records removed by a workspace save.
    pub(crate) fn start_isolation_cleanup(
        paths: jackin_core::JackinPaths,
        records: Vec<jackin_runtime::isolation::state::IsolationRecord>,
    ) -> BlockingSubscription<anyhow::Result<()>> {
        jackin_console::tui::runtime::spawn_named_async_subscription(
            "jackin-isolation-cleanup",
            async move {
                async {
                    for rec in records {
                        let container_dir = paths.data_dir.join(&rec.container_name);
                        let mut runner = jackin_docker::ShellRunner::default();
                        jackin_runtime::isolation::cleanup::force_cleanup_isolated(
                            &rec,
                            &container_dir,
                            &mut runner,
                        )
                        .await?;
                    }
                    Ok(())
                }
                .await
            },
        )
    }
}

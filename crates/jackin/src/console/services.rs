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
    use jackin_config::{AppConfig, BootstrapReport, RoleSource};
    use jackin_console::services::config_save::{
        WorkspaceSaveDiffOp, build_workspace_edit, workspace_save_diff_plan,
    };
    use jackin_console::tui::screens::settings::model::AccountScanOutcome;
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
        let (mut editor_doc, bootstrap) = jackin_config::ConfigEditor::open_detailed(paths)?;
        emit_bootstrap_report(&bootstrap);
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
        let (mut editor_doc, bootstrap) = jackin_config::ConfigEditor::open_detailed(paths)?;
        emit_bootstrap_report(&bootstrap);
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
        let (mut editor_doc, bootstrap) = jackin_config::ConfigEditor::open_detailed(paths)?;
        emit_bootstrap_report(&bootstrap);
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
        let (mut editor_doc, bootstrap) = jackin_config::ConfigEditor::open_detailed(paths)?;
        emit_bootstrap_report(&bootstrap);
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
        pub auth_pending: std::collections::BTreeMap<String, jackin_config::AccountConfig>,
        pub auth_original: std::collections::BTreeMap<String, jackin_config::AccountConfig>,
        pub bindings_pending: std::collections::BTreeMap<jackin_core::Agent, String>,
        pub bindings_original: std::collections::BTreeMap<jackin_core::Agent, String>,
        pub github: jackin_config::GithubAuthConfig,
        pub original_github: jackin_config::GithubAuthConfig,
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
                auth_original: &self.auth_original,
                bindings_pending: &self.bindings_pending,
                bindings_original: &self.bindings_original,
                github: &self.github,
                original_github: &self.original_github,
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
            let result = save_settings_first_run_aware(&paths, &input);
            jackin_console::tui::subscriptions::ConfigSaveResult::Settings(result)
        })
    }

    /// Settings save with first-run bootstrap surfaced. The pre-open
    /// consumes any installer marker and runs the initial scan under the
    /// config lock; [`save_settings`] then applies the UI diff on top of
    /// the bootstrapped config (bootstrap IDs are absent from the UI
    /// originals, so they are preserved — and the returned config carries
    /// them back to the UI refresh path).
    fn save_settings_first_run_aware(
        paths: &JackinPaths,
        input: &OwnedSettingsSaveInput,
    ) -> anyhow::Result<AppConfig> {
        let (editor, bootstrap) = jackin_config::ConfigEditor::open_detailed(paths)?;
        drop(editor);
        emit_bootstrap_report(&bootstrap);
        save_settings(paths, input.as_borrowed())
    }

    /// Surface a first-run bootstrap report through operator diagnostics.
    /// The config-save channel carries `AppConfig` only, so the fresh
    /// flag, added IDs, and discovery issues ride the diagnostics surface
    /// instead. Secret-free: account IDs, counts, agents, error
    /// categories, and directories — never credential values or 1Password
    /// item IDs. Silent when the report is empty.
    fn emit_bootstrap_report(report: &BootstrapReport) {
        if report.fresh_install {
            let added = report.added_accounts.len();
            let ids = report.added_accounts.join(", ");
            jackin_diagnostics::emit_compact_line(
                "info",
                &format!("jackin: first-run account scan imported {added} account(s): {ids}"),
            );
        }
        for issue in &report.issues {
            let agent = issue.agent;
            let error = issue.error;
            jackin_diagnostics::emit_compact_line(
                "warning",
                &format!(
                    "jackin: account scan issue: {agent}: {error} ({})",
                    issue.directory.display()
                ),
            );
        }
    }

    /// Spawn the Settings account-scan worker. Discovery is blocking
    /// filesystem/Keychain I/O — it must never run on the UI thread.
    /// Candidates are returned unsaved; the Accounts tab joins them into
    /// the pending draft (Apply commits, Cancel preserves). The echoed
    /// `generation` lets the scan reducer ignore orphaned completions.
    pub(crate) fn start_account_scan(
        paths: JackinPaths,
        generation: u64,
    ) -> jackin_console::tui::runtime::BlockingSubscription<(u64, Result<AccountScanOutcome, String>)>
    {
        jackin_console::tui::runtime::spawn_blocking_subscription(move || {
            (generation, run_account_scan(&paths))
        })
    }

    /// Blocking scan body: open under the config lock (first-run aware),
    /// scan, drop without saving. Concurrent scans serialize on the lock;
    /// the loser dedupes against the winner's committed accounts. Error
    /// strings carry open/scan failures only (lock, IO, TOML shape) —
    /// never credential values.
    fn run_account_scan(paths: &JackinPaths) -> Result<AccountScanOutcome, String> {
        let (mut editor, open_report) = jackin_config::ConfigEditor::open_detailed(paths)
            .map_err(|error| format!("{error:#}"))?;
        let scan_report = editor
            .scan_for_accounts()
            .map_err(|error| format!("{error:#}"))?;
        drop(editor);
        Ok(AccountScanOutcome {
            fresh_install: open_report.fresh_install,
            committed: open_report.added,
            candidates: scan_report.added,
            issues: open_report
                .issues
                .into_iter()
                .chain(scan_report.issues)
                .collect(),
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
                WorkspaceSaveDiffOp::WorkspaceAccounts { accounts } => {
                    editor_doc.set_workspace_accounts(workspace_name, &accounts)?;
                }
                WorkspaceSaveDiffOp::WorkspaceAccountBinding { agent, account } => {
                    editor_doc.set_account_binding(
                        Some(workspace_name),
                        None,
                        agent,
                        account.as_deref(),
                    )?;
                }
                WorkspaceSaveDiffOp::WorkspaceRoleAccountBinding {
                    role,
                    agent,
                    account,
                } => {
                    editor_doc.set_account_binding(
                        Some(workspace_name),
                        Some(&role),
                        agent,
                        account.as_deref(),
                    )?;
                }
                WorkspaceSaveDiffOp::WorkspaceGithubAuthForward { mode } => {
                    editor_doc.set_workspace_github_auth_forward(workspace_name, mode);
                }
                WorkspaceSaveDiffOp::WorkspaceRoleGithubAuthForward { role, mode } => {
                    editor_doc.set_workspace_role_github_auth_forward(workspace_name, &role, mode);
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
        let mut admissions = HashMap::new();
        let mut snapshot_targets: Vec<String> = Vec::new();
        let mut recovered_failure = false;

        for entry in &instances {
            if is_live_instance_status(entry.status)
                && !record_live_manifest(
                    paths,
                    &entry.container_base,
                    &mut admissions,
                    &mut sessions,
                )
            {
                recovered_failure = true;
                session_errors.insert(entry.container_base.clone());
            }
            if should_snapshot_instance(entry, running_filter.as_ref()) {
                snapshot_targets.push(entry.container_base.clone());
            }
        }

        let mut snapshots = HashMap::new();
        let mut exec_fallback_seen = false;
        let snapshot_results = fetch_snapshots_parallel(paths, &snapshot_targets);
        for (container, result) in snapshot_results {
            recovered_failure |=
                apply_snapshot_result(container, result, &mut snapshots, &mut exec_fallback_seen);
        }

        if recovered_failure {
            let _event = jackin_telemetry::record_recovered_degradation();
        }

        Ok(ManagerInstanceRefreshSnapshot {
            instances,
            sessions,
            session_errors,
            admissions,
            snapshots,
            next_interval: instance_refresh_interval(exec_fallback_seen),
        })
    }

    fn record_live_manifest(
        paths: &jackin_core::JackinPaths,
        container_base: &str,
        admissions: &mut HashMap<
            String,
            Vec<jackin_console::services::launch::LiveInstanceAdmission>,
        >,
        sessions: &mut HashMap<String, Vec<jackin_core::SessionRecord>>,
    ) -> bool {
        let Ok(manifest) =
            jackin_runtime::instance::InstanceManifest::read(&paths.data_dir.join(container_base))
        else {
            return false;
        };

        admissions.insert(
            container_base.to_owned(),
            manifest
                .admitted_instances
                .iter()
                .map(live_instance_admission)
                .collect(),
        );
        if !manifest.sessions.is_empty() {
            sessions.insert(container_base.to_owned(), manifest.sessions);
        }
        true
    }

    fn live_instance_admission(
        admitted: &jackin_runtime::instance::AdmittedInstance,
    ) -> jackin_console::services::launch::LiveInstanceAdmission {
        jackin_console::services::launch::LiveInstanceAdmission {
            instance_id: admitted.config_id.clone(),
            agent: admitted.agent,
            account_id: admitted.account_id.clone(),
        }
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

    fn apply_snapshot_result(
        container: String,
        result: anyhow::Result<(
            Option<jackin_runtime::runtime::snapshot::InstanceSnapshot>,
            SnapshotTransport,
        )>,
        snapshots: &mut HashMap<String, jackin_runtime::runtime::snapshot::InstanceSnapshot>,
        exec_fallback_seen: &mut bool,
    ) -> bool {
        let Ok((snapshot, transport)) = result else {
            return true;
        };

        *exec_fallback_seen |= transport == SnapshotTransport::DockerExecFallback;
        if let Some(snapshot) = snapshot {
            snapshots.insert(container, snapshot);
        }
        false
    }

    fn fetch_snapshots_parallel(
        paths: &jackin_core::JackinPaths,
        targets: &[String],
    ) -> Vec<SnapshotFetchResult> {
        const SNAPSHOT_FANOUT_CHUNK: usize = 8;
        let mut results = Vec::with_capacity(targets.len());
        for chunk in targets.chunks(SNAPSHOT_FANOUT_CHUNK) {
            results.extend(fetch_snapshot_chunk(paths, chunk));
        }
        results
    }

    fn fetch_snapshot_chunk(
        paths: &jackin_core::JackinPaths,
        chunk: &[String],
    ) -> Vec<SnapshotFetchResult> {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for container in chunk {
                let container = container.clone();
                handles.push(jackin_telemetry::spawn::thread_scoped_joined(
                    scope,
                    move || {
                        let result =
                            jackin_runtime::runtime::snapshot::fetch_snapshot_with_transport(
                                paths, &container,
                            );
                        (container, result)
                    },
                ));
            }
            handles.into_iter().map(join_snapshot_worker).collect()
        })
    }

    fn join_snapshot_worker(
        handle: std::thread::ScopedJoinHandle<'_, SnapshotFetchResult>,
    ) -> SnapshotFetchResult {
        handle.join().unwrap_or_else(|panic_payload| {
            let detail = panic_payload
                .downcast_ref::<&'static str>()
                .map(|s| (*s).to_owned())
                .or_else(|| panic_payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_owned());
            (
                "<unknown-container>".to_owned(),
                Err(anyhow::anyhow!("snapshot worker thread panicked: {detail}")),
            )
        })
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

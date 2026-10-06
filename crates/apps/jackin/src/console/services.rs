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
pub(super) mod config;

pub(super) mod instances;
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

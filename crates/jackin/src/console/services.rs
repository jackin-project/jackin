//! Console side-effect adapters.

pub(super) mod agents {
    pub(crate) async fn resolve_supported_for_console(
        paths: &crate::paths::JackinPaths,
        config: &jackin_config::AppConfig,
        role: &jackin_core::RoleSelector,
        runner: &mut impl crate::docker::CommandRunner,
    ) -> anyhow::Result<Vec<jackin_core::Agent>> {
        crate::runtime::resolve_supported_agents_for_console(paths, config, role, runner).await
    }

    pub(crate) async fn load_inline_picker_choices(
        paths: &crate::paths::JackinPaths,
        config: &jackin_config::AppConfig,
        role: &jackin_core::RoleSelector,
        runner: &mut impl crate::docker::CommandRunner,
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
    use jackin_tui::runtime::BlockingSubscription;

    pub(crate) fn start_role_registration(
        paths: crate::paths::JackinPaths,
        selector: jackin_core::RoleSelector,
        git_url: String,
    ) -> BlockingSubscription<anyhow::Result<()>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut runner = crate::docker::ShellRunner {
                debug: crate::tui::is_debug_mode(),
            };
            let result = register_with_runner(
                &paths,
                &selector,
                &git_url,
                &mut runner,
                crate::tui::is_debug_mode(),
            )
            .await;
            drop(tx.send(result));
        });
        rx
    }

    pub(crate) async fn register_with_runner(
        paths: &crate::paths::JackinPaths,
        selector: &jackin_core::RoleSelector,
        git_url: &str,
        runner: &mut impl crate::docker::CommandRunner,
        debug: bool,
    ) -> anyhow::Result<()> {
        std::panic::AssertUnwindSafe(async {
            crate::runtime::register_agent_repo(paths, selector, git_url, runner, debug).await?;
            Ok::<_, anyhow::Error>(())
        })
        .catch_unwind()
        .await
        .unwrap_or_else(|payload| {
            let panic_message = panic_payload_message(payload.as_ref());
            Err(anyhow::anyhow!("role loader panicked: {panic_message}"))
        })
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
    use jackin_tui::runtime::BlockingSubscription;

    /// Start the Docker-backed drift check for an edited workspace.
    pub(crate) fn start_drift_check(
        paths: crate::paths::JackinPaths,
        workspace_name: String,
        prospective_mounts: Vec<jackin_config::MountConfig>,
    ) -> BlockingSubscription<anyhow::Result<crate::runtime::drift::DriftDetection>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = async {
                let docker = crate::docker_client::BollardDockerClient::connect()?;
                crate::runtime::drift::detect_workspace_edit_drift(
                    &paths,
                    &workspace_name,
                    &prospective_mounts,
                    &docker,
                )
                .await
            }
            .await;
            drop(tx.send(result));
        });
        rx
    }

    /// Start cleanup for isolated mount records removed by a workspace save.
    pub(crate) fn start_isolation_cleanup(
        paths: crate::paths::JackinPaths,
        records: Vec<jackin_runtime::isolation::state::IsolationRecord>,
    ) -> BlockingSubscription<anyhow::Result<()>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = async {
                for rec in records {
                    let container_dir = paths.data_dir.join(&rec.container_name);
                    let mut runner = crate::docker::ShellRunner::default();
                    jackin_runtime::isolation::cleanup::force_cleanup_isolated(
                        &rec,
                        &container_dir,
                        &mut runner,
                    )
                    .await?;
                }
                Ok(())
            }
            .await;
            drop(tx.send(result));
        });
        rx
    }
}

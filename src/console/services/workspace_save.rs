//! Non-TUI workspace save side-effect services.

use jackin_tui::runtime::BlockingSubscription;

/// Start the Docker-backed drift check for an edited workspace.
pub fn start_drift_check(
    paths: crate::paths::JackinPaths,
    workspace_name: String,
    prospective_mounts: Vec<crate::workspace::MountConfig>,
) -> BlockingSubscription<anyhow::Result<crate::config::DriftDetection>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let result = async {
            let docker = crate::docker_client::BollardDockerClient::connect()?;
            crate::config::detect_workspace_edit_drift(
                &paths,
                &workspace_name,
                &prospective_mounts,
                &docker,
            )
            .await
        }
        .await;
        let _ = tx.send(result);
    });
    rx
}

/// Start cleanup for isolated mount records removed by a workspace save.
pub fn start_isolation_cleanup(
    paths: crate::paths::JackinPaths,
    records: Vec<crate::isolation::state::IsolationRecord>,
) -> BlockingSubscription<anyhow::Result<()>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let result = async {
            for rec in records {
                let container_dir = paths.data_dir.join(&rec.container_name);
                let mut runner = crate::docker::ShellRunner::default();
                crate::isolation::cleanup::force_cleanup_isolated(
                    &rec,
                    &container_dir,
                    &mut runner,
                )
                .await?;
            }
            Ok(())
        }
        .await;
        let _ = tx.send(result);
    });
    rx
}

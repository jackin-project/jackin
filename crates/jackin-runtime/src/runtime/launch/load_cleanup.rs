// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `LoadCleanup` coordinator helper and atomic-write guard extracted from the
//! launch coordinator. All items re-exported from the parent to preserve
//! `super::` call sites in `launch_role_runtime` (via the write calls inside
//! the capsule socket prep) and in `launch_pipeline.rs`.

use std::path::{Path, PathBuf};

use jackin_core::{ContainerHandle, ContainerId, ContainerState, JackinPaths};
use jackin_docker::docker_client::DockerApi;

use crate::runtime::progress::launch_output;

fn record_cleanup_teardown_failure(body: &'static str) {
    let _error =
        jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::LaunchFailed);
    jackin_diagnostics::emit_operator_notice(body);
}

pub(crate) fn write_if_changed_atomic(
    path: &Path,
    tmp: &Path,
    bytes: &[u8],
) -> std::io::Result<()> {
    // Single-file bind mounts keep the original inode alive in running
    // containers. Skip the temp+rename when the content already matches so a
    // concurrent launch cannot invalidate getpwuid/getgrgid lookups in an
    // already-running container.
    let unchanged = std::fs::read(path).is_ok_and(|existing| existing == bytes);
    if !unchanged {
        std::fs::write(tmp, bytes)?;
        std::fs::rename(tmp, path)?;
    }
    Ok(())
}

/// Coordinates Docker resource teardown for a failed or completed launch.
#[derive(Debug)]
pub struct LoadCleanup {
    trusted_paths: JackinPaths,
    container_name: String,
    dind: String,
    role_handle_slot: std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>>,
    dind_handle_slot: std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>>,
    network_id_slot: std::sync::Arc<std::sync::Mutex<Option<jackin_core::NetworkId>>>,
    shared_custody:
        std::sync::Mutex<Option<std::sync::Arc<super::launch_dind::SharedDockerCreationCustody>>>,
    dind_required: std::sync::atomic::AtomicBool,
    certs_volume: String,
    /// Host-side bind-mount dir (`~/.jackin/sockets/<container>/`).
    /// Removed only when `armed` is true AND the cleanup fires on the
    /// launch-failure path — `clean_socket_dir` distinguishes that from
    /// post-session teardown where the operator may still want to
    /// inspect the just-written Capsule launch config. Post-session
    /// teardown paths flip `clean_socket_dir = false` before
    /// `cleanup.run()` (or call `disarm`); explicit cleanup commands
    /// (`jackin eject`, Purge from the console) sweep the directory via
    /// `cleanup::eject_role` / `purge_container_filesystem`.
    socket_dir: PathBuf,
    clean_socket_dir: bool,
    armed: bool,
}

impl LoadCleanup {
    /// Arm cleanup for the named role container + `DinD` + network + certs volume.
    pub fn new(
        paths: &JackinPaths,
        container_name: String,
        dind: String,
        certs_volume: String,
    ) -> anyhow::Result<Self> {
        let container_id = ContainerId::parse(&container_name)?;
        anyhow::ensure!(
            matches!(
                Path::new(container_id.as_str()).components().next(),
                Some(std::path::Component::Normal(_))
            ),
            "cleanup role must be a normal path component"
        );
        let socket_dir = paths
            .jackin_home
            .join("sockets")
            .join(container_id.as_str());
        Ok(Self {
            trusted_paths: paths.clone(),
            container_name,
            dind,
            role_handle_slot: std::sync::Arc::new(std::sync::Mutex::new(None)),
            dind_handle_slot: std::sync::Arc::new(std::sync::Mutex::new(None)),
            network_id_slot: std::sync::Arc::new(std::sync::Mutex::new(None)),
            shared_custody: std::sync::Mutex::new(None),
            dind_required: std::sync::atomic::AtomicBool::new(false),
            certs_volume,
            socket_dir,
            clean_socket_dir: true,
            armed: true,
        })
    }

    pub(crate) fn trusted_paths(&self) -> &JackinPaths {
        &self.trusted_paths
    }

    #[cfg(test)]
    pub(crate) fn socket_dir(&self) -> &Path {
        &self.socket_dir
    }

    pub(crate) const fn disarm(&mut self) {
        self.armed = false;
    }

    /// Switch off socket-dir cleanup for post-session teardown.
    /// docker-resource removal still runs (`cleanup.run` is reused for
    /// "session ended cleanly, tear down DinD/network/volume"); the
    /// host-side bind-mount dir is left for the operator to inspect
    /// and gets reaped by the next explicit eject / purge.
    pub(crate) const fn keep_socket_dir(&mut self) {
        self.clean_socket_dir = false;
    }

    pub(crate) fn bind_shared_custody(
        &self,
        custody: std::sync::Arc<super::launch_dind::SharedDockerCreationCustody>,
    ) {
        *self
            .shared_custody
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(custody);
    }

    pub(crate) fn shared_custody_handle(
        &self,
    ) -> Option<std::sync::Arc<super::launch_dind::SharedDockerCreationCustody>> {
        self.shared_custody
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Share the sidecar identity sink with the concurrent sidecar launch.
    pub(crate) fn dind_handle_slot(
        &self,
    ) -> std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>> {
        std::sync::Arc::clone(&self.dind_handle_slot)
    }

    /// Share the immutable network identity returned by the creating daemon call.
    pub(crate) fn network_id_slot(
        &self,
    ) -> std::sync::Arc<std::sync::Mutex<Option<jackin_core::NetworkId>>> {
        std::sync::Arc::clone(&self.network_id_slot)
    }

    pub(crate) fn set_network_id(&self, network: jackin_core::NetworkId) {
        *self
            .network_id_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(network);
    }

    /// Share the role identity sink with the launch that owns this cleanup.
    pub(crate) fn role_handle_slot(
        &self,
    ) -> std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>> {
        std::sync::Arc::clone(&self.role_handle_slot)
    }

    /// Bind cleanup to the role identity returned by Docker create.
    pub(crate) fn set_role_handle(&self, container: ContainerHandle) {
        *self
            .role_handle_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(container);
    }

    /// Bind cleanup to a sidecar identity already captured during adoption.
    pub(crate) fn set_dind_handle(&self, container: ContainerHandle) {
        self.dind_required
            .store(true, std::sync::atomic::Ordering::Release);
        *self
            .dind_handle_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(container);
    }

    /// Record whether this launch owns a `DinD` sidecar even before its create
    /// call returns an immutable handle. Cleanup uses this to distinguish a
    /// role-only network from shared sidecar resources.
    pub(crate) fn set_dind_required(&self, required: bool) {
        self.dind_required
            .store(required, std::sync::atomic::Ordering::Release);
    }

    /// Best-effort remove role/DinD containers, cert volume, network, and socket dir.
    pub async fn run(&self, docker: &impl DockerApi) {
        self.run_inner(docker, false).await;
    }

    /// Bind a caller-owned immutable role identity before teardown.
    pub(crate) async fn run_with_role_handle(
        &self,
        docker: &impl DockerApi,
        container: &ContainerHandle,
    ) {
        self.set_role_handle(container.clone());
        self.run(docker).await;
    }

    /// Best-effort cleanup after the role container has started and failed.
    ///
    /// Retain only terminal role-container evidence (`docker logs`/inspect and
    /// the launch config in the private socket directory). A live, transitional,
    /// missing, or uninspectable role is cleaned up fail-closed so this path
    /// cannot leave a running container or private socket endpoint behind.
    pub async fn run_preserving_evidence(&self, docker: &impl DockerApi) {
        let role_handle = self
            .role_handle_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let preserve_role_evidence = match role_handle.as_ref() {
            Some(container) => {
                let state = docker.inspect_container_by_id(container).await;
                preserves_failed_start_evidence(&state)
            }
            None => false,
        };
        self.run_inner(docker, preserve_role_evidence).await;
    }

    async fn run_inner(&self, docker: &impl DockerApi, preserve_role_evidence: bool) {
        if !self.armed {
            return;
        }

        let mut role_handle = self
            .role_handle_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();

        let dind_required = self
            .dind_required
            .load(std::sync::atomic::Ordering::Acquire);
        let mut dind_handle = self
            .dind_handle_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let captured_network = self
            .network_id_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let custody = self.shared_custody_handle();
        let admission: anyhow::Result<_> = async {
            anyhow::ensure!(
                role_handle
                    .as_ref()
                    .is_none_or(|handle| handle.name() == self.container_name),
                "captured role differs from rollback owner"
            );
            anyhow::ensure!(
                dind_handle
                    .as_ref()
                    .is_none_or(|handle| handle.name() == self.dind),
                "captured DinD differs from rollback owner"
            );
            let socket =
                if !preserve_role_evidence && self.clean_socket_dir {
                    Some(
                        crate::runtime::cleanup::admit_socket_removal(
                            self.trusted_paths(),
                            &self.container_name,
                        )
                        .await?,
                    )
                } else {
                    None
                };
            let shared_plan = if let Some(custody) = custody.as_ref() {
                let lifetime = custody.admitted_snapshot()?;
                let plan = crate::runtime::cleanup::reconcile_shared_lifetime(
                    self.trusted_paths(),
                    lifetime,
                    docker,
                )
                .await?;
                custody.replace_snapshot(plan.lifetime.clone())?;
                anyhow::ensure!(plan.lifetime.owner() == self.container_name,
                    "rollback owner differs from shared lifetime");
                anyhow::ensure!(plan.lifetime.dind_container_name().is_none_or(|name| name == self.dind),
                    "rollback DinD name differs from shared lifetime");
                anyhow::ensure!(plan.lifetime.certs_volume_name().is_none_or(|name| name == self.certs_volume),
                    "rollback certificate volume differs from shared lifetime");
                anyhow::ensure!(dind_required == plan.lifetime.dind_container_name().is_some(),
                    "rollback DinD requirement differs from durable lifetime");
                anyhow::ensure!(captured_network.as_ref().is_none_or(|id| Some(id) == plan.lifetime.network_id()),
                    "captured network differs from durable lifetime");
                if let Some(handle) = role_handle.as_ref() {
                    anyhow::ensure!(plan.role.as_ref().is_some_and(|owned| owned.id() == handle.id()),
                        "captured role differs from reconciled lifetime");
                }
                if let Some(handle) = dind_handle.as_ref() {
                    anyhow::ensure!(plan.dind.as_ref().is_some_and(|owned| owned.id() == handle.id()),
                        "captured DinD differs from reconciled lifetime");
                }
                Some(plan)
            } else {
                anyhow::ensure!(
                    !dind_required,
                    "certificate volume ownership is unavailable; retaining Docker custody"
                );
                None
            };
            let (volume, network) = shared_plan.as_ref().map_or((None, captured_network), |plan| {
                (plan.volume.clone(), plan.network.clone())
            });
            if let Some(plan) = shared_plan.as_ref() {
                crate::runtime::cleanup::ensure_shared_plan_current(self.trusted_paths(), plan, docker).await?;
            }
            Ok((shared_plan, volume, network, socket))
        }
        .await;
        let (mut shared_plan, volume, network, socket) = match admission {
            Ok(admitted) => admitted,
            Err(error) => {
                record_cleanup_teardown_failure("cleanup admission failed");
                launch_output().step_fail(&format!("cleanup retained Docker custody: {error}"));
                return;
            }
        };
        if let Some(plan) = shared_plan.as_ref() {
            role_handle = plan.role.clone();
            dind_handle = plan.dind.clone();
        }
        if !preserve_role_evidence
            && let Some(plan) = shared_plan.as_mut()
        {
            let retirement = plan.lifetime.begin_retirement(self.trusted_paths());
            match retirement {
                Ok(lifetime) => plan.lifetime = lifetime,
                Err(error) => {
                    record_cleanup_teardown_failure("cleanup retirement admission failed");
                    launch_output().step_fail(&format!("cleanup retained Docker custody: {error}"));
                    return;
                }
            }
            if let Err(error) = crate::runtime::cleanup::ensure_shared_plan_current(
                self.trusted_paths(), plan, docker,
            )
            .await
            {
                record_cleanup_teardown_failure("cleanup retirement verification failed");
                launch_output().step_fail(&format!("cleanup retained Docker custody: {error}"));
                return;
            }
        }

        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::Cleanup,
            "cancel_cleanup",
            None,
        );
        if let Some(run) = jackin_diagnostics::active_run() {
            run.compact("cleanup", "cancel cleanup started");
        }

        let mut container_cleanup_failed = false;
        if !preserve_role_evidence {
            let result = match role_handle.as_ref() {
                Some(container) => Some(docker.remove_container_by_id(container).await),
                None => None,
            };
            if let Some(Err(e)) = result {
                container_cleanup_failed = true;
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact("cleanup", &format!("cleanup failed (container): {e}"));
                }
                record_cleanup_teardown_failure("cleanup failed (container)");
                launch_output().step_fail(&format!("cleanup failed (container): {e}"));
            }
        }
        let dind_result = if dind_required {
            match dind_handle.as_ref() {
                Some(container) => Some(docker.remove_container_by_id(container).await),
                None => None,
            }
        } else {
            Some(Ok(()))
        };
        if let Some(Err(e)) = dind_result {
            container_cleanup_failed = true;
            if let Some(run) = jackin_diagnostics::active_run() {
                run.compact("cleanup", &format!("cleanup failed (dind): {e}"));
            }
            record_cleanup_teardown_failure("cleanup failed (dind)");
            launch_output().step_fail(&format!("cleanup failed (dind): {e}"));
        }
        if container_cleanup_failed {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Cleanup,
                "cancel_cleanup",
                None,
            );
            return;
        }
        if preserve_role_evidence {
            // A stopped role may be restarted by hardline. Keep its network,
            // DinD, certificates, socket, and active lifetime together; a
            // partial teardown would leave the preserved container unusable.
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Cleanup,
                "cancel_cleanup",
                None,
            );
            return;
        }
        let mut shared_cleanup_failed = false;
        if let Some(volume) = volume {
            if let Err(e) = docker.remove_volume(&volume).await {
                shared_cleanup_failed = true;
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact("cleanup", &format!("cleanup failed (certs volume): {e}"));
                }
                record_cleanup_teardown_failure("cleanup failed (certs volume)");
                launch_output().step_fail(&format!("cleanup failed (certs volume): {e}"));
            } else {
                match docker.inspect_volume_by_name(&volume).await {
                    Ok(None) => {}
                    Ok(Some(_)) => {
                        shared_cleanup_failed = true;
                        record_cleanup_teardown_failure("cleanup failed (certs volume verification)");
                        launch_output().step_fail(&format!("cleanup failed: certificate volume {volume} remains after removal"));
                    }
                    Err(error) => {
                        shared_cleanup_failed = true;
                        record_cleanup_teardown_failure("cleanup failed (certs volume verification)");
                        launch_output().step_fail(&format!("cleanup failed verifying certificate volume {volume}: {error}"));
                    }
                }
            }
        }
        // A captured network creation identity owns rollback independently
        // of whether either container creation reached its identity sink.
        if let Some(network) = network {
            if let Err(error) = docker.remove_network_by_id(&network).await {
                shared_cleanup_failed = true;
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact("cleanup", &format!("cleanup failed (network): {error}"));
                }
                record_cleanup_teardown_failure("cleanup failed (network)");
                launch_output().step_fail(&format!("cleanup failed (network): {error}"));
            }
        }
        let mut socket_cleanup_failed = false;
        if let Some(socket) = socket.filter(|_| !shared_cleanup_failed) {
            match crate::runtime::cleanup::remove_admitted_socket(socket).await {
                Ok(()) => {}
                Err(error) => {
                    socket_cleanup_failed = true;
                    record_cleanup_teardown_failure("cleanup failed (socket dir)");
                    if let Some(run) = jackin_diagnostics::active_run() {
                        run.compact(
                            "cleanup",
                            &format!(
                                "cleanup failed (socket dir {}): {error}",
                                self.socket_dir.display()
                            ),
                        );
                    }
                    launch_output().step_fail(&format!(
                        "cleanup failed (socket dir {}): {error}",
                        self.socket_dir.display()
                    ));
                }
            }
        }
        if !preserve_role_evidence
            && !container_cleanup_failed
            && !shared_cleanup_failed
            && !socket_cleanup_failed
            && let Some(plan) = shared_plan.as_ref()
        {
            if let Err(error) = plan.lifetime.retire(self.trusted_paths()) {
                record_cleanup_teardown_failure("cleanup lifetime retirement failed");
                launch_output().step_fail(&format!(
                    "cleanup completed but custody remains retiring: {error}"
                ));
            }
        }
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Cleanup,
            "cancel_cleanup",
            None,
        );
    }
}

fn preserves_failed_start_evidence(state: &ContainerState) -> bool {
    matches!(state, ContainerState::Stopped { .. } | ContainerState::Dead)
}

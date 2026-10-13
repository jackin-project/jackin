// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `LoadCleanup` coordinator helper and atomic-write guard extracted from the
//! launch coordinator. All items re-exported from the parent to preserve
//! `super::` call sites in `launch_role_runtime` (via the write calls inside
//! the capsule socket prep) and in `launch_pipeline.rs`.

use std::path::{Path, PathBuf};

use jackin_core::{ContainerHandle, ContainerState};
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
    container_name: String,
    dind: String,
    role_handle_slot: std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>>,
    dind_handle_slot: std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>>,
    dind_required: std::sync::atomic::AtomicBool,
    certs_volume: String,
    network: String,
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
    #[must_use]
    pub fn new(
        container_name: String,
        dind: String,
        certs_volume: String,
        network: String,
        socket_dir: PathBuf,
    ) -> Self {
        Self {
            container_name,
            dind,
            role_handle_slot: std::sync::Arc::new(std::sync::Mutex::new(None)),
            dind_handle_slot: std::sync::Arc::new(std::sync::Mutex::new(None)),
            dind_required: std::sync::atomic::AtomicBool::new(false),
            certs_volume,
            network,
            socket_dir,
            clean_socket_dir: true,
            armed: true,
        }
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

    /// Share the sidecar identity sink with the concurrent sidecar launch.
    pub(crate) fn dind_handle_slot(
        &self,
    ) -> std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>> {
        std::sync::Arc::clone(&self.dind_handle_slot)
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

        let role_handle = self
            .role_handle_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();

        let dind_required = self
            .dind_required
            .load(std::sync::atomic::Ordering::Acquire);

        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::Cleanup,
            "cancel_cleanup",
            None,
        );
        if let Some(run) = jackin_diagnostics::active_run() {
            run.compact("cleanup", "cancel cleanup started");
        }

        if !preserve_role_evidence {
            let result = match role_handle.as_ref() {
                Some(container) => Some(docker.remove_container_by_id(container).await),
                None => None,
            };
            if let Some(Err(e)) = result {
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact("cleanup", &format!("cleanup failed (container): {e}"));
                }
                record_cleanup_teardown_failure("cleanup failed (container)");
                launch_output().step_fail(&format!("cleanup failed (container): {e}"));
            }
        }
        let dind_handle = self
            .dind_handle_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let dind_result = if dind_required {
            match dind_handle.as_ref() {
                Some(container) => Some(docker.remove_container_by_id(container).await),
                None => None,
            }
        } else {
            Some(Ok(()))
        };
        if let Some(Err(e)) = dind_result {
            if let Some(run) = jackin_diagnostics::active_run() {
                run.compact("cleanup", &format!("cleanup failed (dind): {e}"));
            }
            record_cleanup_teardown_failure("cleanup failed (dind)");
            launch_output().step_fail(&format!("cleanup failed (dind): {e}"));
        }
        // `dind_required` is set only after this launch has admitted the
        // sidecar resource set. That ownership marker remains valid even when
        // create fails before Docker can return a sidecar ID; it authorizes
        // shared-resource cleanup but never authorizes a container operation.
        let resource_cleanup_authorized =
            role_handle.is_some() || dind_handle.is_some() || dind_required;
        if resource_cleanup_authorized {
            if let Err(e) = docker.remove_volume(&self.certs_volume).await {
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact("cleanup", &format!("cleanup failed (certs volume): {e}"));
                }
                record_cleanup_teardown_failure("cleanup failed (certs volume)");
                launch_output().step_fail(&format!("cleanup failed (certs volume): {e}"));
            }
            if let Err(e) = docker.remove_network(&self.network).await {
                if let Some(run) = jackin_diagnostics::active_run() {
                    run.compact("cleanup", &format!("cleanup failed (network): {e}"));
                }
                record_cleanup_teardown_failure("cleanup failed (network)");
                launch_output().step_fail(&format!("cleanup failed (network): {e}"));
            }
        } else if let Some(run) = jackin_diagnostics::active_run() {
            let missing_identity = if dind_required && dind_handle.is_none() {
                format!(
                    "role and DinD identities for {} / {}",
                    self.container_name, self.dind
                )
            } else {
                format!("role identity for {}", self.container_name)
            };
            run.compact(
                "cleanup",
                &format!("cleanup skipped (resources): no captured Docker {missing_identity}"),
            );
        }
        if !preserve_role_evidence && self.clean_socket_dir && role_handle.is_some() {
            match std::fs::remove_dir_all(&self.socket_dir) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
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

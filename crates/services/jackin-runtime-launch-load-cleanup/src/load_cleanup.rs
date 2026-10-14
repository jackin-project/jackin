// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use jackin_core::{ContainerHandle, ContainerState};
use jackin_docker::docker_client::DockerApi;

use jackin_runtime_progress::progress::launch_output;

fn record_cleanup_teardown_failure(body: &'static str) {
    let _error =
        jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::LaunchFailed);
    jackin_diagnostics::emit_operator_notice(body);
}

pub fn write_if_changed_atomic(path: &Path, tmp: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let unchanged = std::fs::read(path).is_ok_and(|existing| existing == bytes);
    if !unchanged {
        std::fs::write(tmp, bytes)?;
        std::fs::rename(tmp, path)?;
    }
    Ok(())
}

#[derive(Debug)]
pub struct LoadCleanup {
    container_name: String,
    dind: String,
    role_handle_slot: std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>>,
    dind_handle_slot: std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>>,
    dind_required: std::sync::atomic::AtomicBool,
    certs_volume: String,
    network: String,
    socket_dir: PathBuf,
    clean_socket_dir: bool,
    armed: bool,
}

impl LoadCleanup {
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

    pub const fn disarm(&mut self) {
        self.armed = false;
    }

    pub const fn keep_socket_dir(&mut self) {
        self.clean_socket_dir = false;
    }

    pub fn dind_handle_slot(&self) -> std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>> {
        std::sync::Arc::clone(&self.dind_handle_slot)
    }

    pub fn role_handle_slot(&self) -> std::sync::Arc<std::sync::Mutex<Option<ContainerHandle>>> {
        std::sync::Arc::clone(&self.role_handle_slot)
    }

    pub fn set_role_handle(&self, container: ContainerHandle) {
        *self
            .role_handle_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(container);
    }

    pub fn set_dind_handle(&self, container: ContainerHandle) {
        self.dind_required
            .store(true, std::sync::atomic::Ordering::Release);
        *self
            .dind_handle_slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(container);
    }

    pub fn set_dind_required(&self, required: bool) {
        self.dind_required
            .store(required, std::sync::atomic::Ordering::Release);
    }

    pub async fn run(&self, docker: &impl DockerApi) {
        self.run_inner(docker, false).await;
    }

    pub async fn run_with_role_handle(&self, docker: &impl DockerApi, container: &ContainerHandle) {
        self.set_role_handle(container.clone());
        self.run(docker).await;
    }

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

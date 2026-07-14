// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `LoadCleanup` coordinator helper and atomic-write guard extracted from the
//! launch coordinator. All items re-exported from the parent to preserve
//! `super::` call sites in `launch_role_runtime` (via the write calls inside
//! the capsule socket prep) and in `launch_pipeline.rs`.

use std::path::{Path, PathBuf};

use jackin_docker::docker_client::DockerApi;

use crate::runtime::progress::launch_output;

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
    pub const fn new(
        container_name: String,
        dind: String,
        certs_volume: String,
        network: String,
        socket_dir: PathBuf,
    ) -> Self {
        Self {
            container_name,
            dind,
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

    /// Best-effort remove role/DinD containers, cert volume, network, and socket dir.
    pub async fn run(&self, docker: &impl DockerApi) {
        if !self.armed {
            return;
        }

        jackin_diagnostics::active_timing_started("cleanup", "cancel_cleanup", None);
        if let Some(run) = jackin_diagnostics::active_run() {
            run.compact("cleanup", "cancel cleanup started");
        }

        if let Err(e) = docker.remove_container(&self.container_name).await {
            if let Some(run) = jackin_diagnostics::active_run() {
                run.compact("cleanup", &format!("cleanup failed (container): {e}"));
            }
            jackin_diagnostics::operation_error(
                "launch.cleanup",
                "cleanup_teardown_failed",
                "cleanup failed (container)",
                &[],
            );
            launch_output().step_fail(&format!("cleanup failed (container): {e}"));
        }
        if let Err(e) = docker.remove_container(&self.dind).await {
            if let Some(run) = jackin_diagnostics::active_run() {
                run.compact("cleanup", &format!("cleanup failed (dind): {e}"));
            }
            launch_output().step_fail(&format!("cleanup failed (dind): {e}"));
        }
        if let Err(e) = docker.remove_volume(&self.certs_volume).await {
            if let Some(run) = jackin_diagnostics::active_run() {
                run.compact("cleanup", &format!("cleanup failed (certs volume): {e}"));
            }
            launch_output().step_fail(&format!("cleanup failed (certs volume): {e}"));
        }
        if let Err(e) = docker.remove_network(&self.network).await {
            if let Some(run) = jackin_diagnostics::active_run() {
                run.compact("cleanup", &format!("cleanup failed (network): {e}"));
            }
            launch_output().step_fail(&format!("cleanup failed (network): {e}"));
        }
        if self.clean_socket_dir {
            match std::fs::remove_dir_all(&self.socket_dir) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
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
        jackin_diagnostics::active_timing_done("cleanup", "cancel_cleanup", None);
    }
}

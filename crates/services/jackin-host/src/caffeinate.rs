// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use anyhow::Context;
use fs4::{FileExt, TryLockError};

use jackin_core::CommandRunner;
use jackin_core::JackinPaths;
use jackin_docker::docker_client::DockerApi;
use jackin_process::{ExecRequest, StdioMode};

const PID_FILENAME: &str = "caffeinate.pid";
const LOCK_FILENAME: &str = "caffeinate.lock";

pub async fn reconcile(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) {
    reconcile_when_configured(paths, docker, runner, true).await;
}

pub async fn reconcile_when_configured(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    any_keep_awake_enabled: bool,
) {
    if !is_supported_platform() {
        return;
    }

    if let Err(err) = reconcile_inner(paths, docker, runner, any_keep_awake_enabled).await {
        jackin_diagnostics::emit_compact_line(
            "keep_awake",
            &format!("[jackin] keep_awake reconciler: {err}"),
        );
    }
}

const fn is_supported_platform() -> bool {
    cfg!(target_os = "macos")
}

async fn reconcile_inner(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    any_keep_awake_enabled: bool,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.data_dir).with_context(|| {
        format!(
            "creating data dir for caffeinate state: {}",
            paths.data_dir.display()
        )
    })?;

    let lock_path = paths.data_dir.join(LOCK_FILENAME);
    #[expect(
        clippy::disallowed_methods,
        reason = "caffeinate lock acquisition is runtime state setup, not frame rendering"
    )]
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("opening {}", lock_path.display()))?;

    match FileExt::try_lock(&lock_file) {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Ok(()),
        Err(TryLockError::Error(err)) => {
            return Err(anyhow::Error::new(err).context(format!("locking {}", lock_path.display())));
        }
    }

    let pid_path = paths.data_dir.join(PID_FILENAME);
    let current_pid = read_pid_file(&pid_path)?;
    if !any_keep_awake_enabled && current_pid.is_none() {
        return Ok(());
    }
    let want_running = any_keep_awake_enabled && count_keep_awake_agents(docker).await? > 0;
    let liveness = if let Some(pid) = current_pid {
        jackin_telemetry::spawn::joined_blocking(move || is_caffeinate_alive_at(pid))
            .await
            .unwrap_or(Liveness::Unknown)
    } else {
        Liveness::Gone
    };

    match (want_running, liveness) {
        (true, Liveness::Alive) => {}
        (true, Liveness::Gone) => {
            let pid = spawn_caffeinate(runner).await?;
            if let Err(err) = write_pid_file(&pid_path, pid) {
                if let Err(stop_err) = stop_caffeinate(runner, pid).await {
                    jackin_diagnostics::emit_compact_line(
                        "keep_awake",
                        &format!(
                            "[jackin] keep_awake: PID file write failed AND cleanup kill of newly-spawned caffeinate (PID {pid}) also failed: {stop_err}; manual `pkill caffeinate` may be required"
                        ),
                    );
                }
                return Err(err);
            }
        }
        (false, Liveness::Alive) => {
            if let Some(pid) = current_pid {
                stop_caffeinate(runner, pid).await?;
            }
            remove_pid_file_if_present(&pid_path)?;
        }
        (false, Liveness::Gone) => {
            remove_pid_file_if_present(&pid_path)?;
        }
        (_, Liveness::Unknown) => {
            let Some(pid) = current_pid else {
                return Ok(());
            };
            jackin_diagnostics::emit_compact_line(
                "keep_awake",
                &format!(
                    "[jackin] keep_awake: ps liveness check inconclusive for recorded PID {pid} — leaving caffeinate state untouched, will retry on next reconcile"
                ),
            );
        }
    }

    Ok(())
}

fn remove_pid_file_if_present(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(anyhow::Error::new(err).context(format!("removing {}", path.display()))),
    }
}

async fn count_keep_awake_agents(docker: &impl DockerApi) -> anyhow::Result<usize> {
    let rows = docker
        .list_containers(
            &[
                crate::naming::LABEL_MANAGED,
                crate::naming::LABEL_KEEP_AWAKE,
            ],
            false,
        )
        .await?;
    Ok(rows.len())
}

fn read_pid_file(path: &Path) -> anyhow::Result<Option<u32>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            let trimmed = contents.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            trimmed.parse::<u32>().map(Some).map_err(|e| {
                anyhow::Error::new(e).context(format!(
                    "PID file {} contains non-numeric data; refusing to overwrite to avoid orphaning caffeinate",
                    path.display()
                ))
            })
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
    }
}

fn write_pid_file(path: &Path, pid: u32) -> anyhow::Result<()> {
    std::fs::write(path, pid.to_string()).with_context(|| format!("writing {}", path.display()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Liveness {
    Alive,
    Gone,
    Unknown,
}

fn is_caffeinate_alive_at(pid: u32) -> Liveness {
    for _ in 0..2 {
        let request = ExecRequest::new("ps", ["-p", &pid.to_string(), "-o", "comm="])
            .stderr_mode(StdioMode::Null);
        if let Ok(output) = crate::process_telemetry::exec_sync(&request) {
            return classify_ps_comm_output(
                output.success,
                &String::from_utf8_lossy(&output.stdout),
            );
        }
    }
    Liveness::Unknown
}

fn classify_ps_comm_output(success: bool, stdout: &str) -> Liveness {
    if !success {
        return Liveness::Gone;
    }
    let basename = stdout.trim().rsplit('/').next().unwrap_or("");
    if basename == "caffeinate" {
        Liveness::Alive
    } else {
        Liveness::Gone
    }
}

async fn spawn_caffeinate(runner: &mut impl CommandRunner) -> anyhow::Result<u32> {
    let raw = runner
        .capture(
            "sh",
            &[
                "-c",
                "nohup caffeinate -imsu </dev/null >/dev/null 2>&1 & echo $!",
            ],
            None,
        )
        .await?;
    raw.parse::<u32>()
        .with_context(|| format!("parsing caffeinate PID from {raw:?}"))
}

async fn stop_caffeinate(runner: &mut impl CommandRunner, pid: u32) -> anyhow::Result<()> {
    runner
        .capture("kill", &[&pid.to_string()], None)
        .await
        .map(|_| ())
}

#[cfg(test)]
pub(super) fn pid_path_for_tests(paths: &JackinPaths) -> std::path::PathBuf {
    paths.data_dir.join(PID_FILENAME)
}

#[cfg(test)]
mod tests;

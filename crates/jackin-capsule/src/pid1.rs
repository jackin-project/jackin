// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! PID 1 responsibility inside the container: reap orphaned child processes.
//!
//! Not responsible for: spawning agent processes (see `session`) or daemon
//! lifecycle (see `daemon`).
//!
//! Key invariant: the shared child registry protects owned exit statuses.
//! Spawn registration and orphan reaping hold the same coordination lock.

use jackin_process::child_ownership;
/// PID 1 zombie reaping.
///
/// Linux: when a process whose parent has exited becomes an orphan, it is
/// re-parented to PID 1. PID 1 MUST call waitpid to reap those zombies or
/// they accumulate in the process table. Tokio does not do this automatically.
use std::sync::{Condvar, Mutex, OnceLock};

static REAPER_THREAD: OnceLock<std::thread::JoinHandle<()>> = OnceLock::new();
static REAPER_WAKE: OnceLock<(Mutex<bool>, Condvar)> = OnceLock::new();

fn reaper_wake() -> &'static (Mutex<bool>, Condvar) {
    REAPER_WAKE.get_or_init(|| (Mutex::new(false), Condvar::new()))
}

#[cfg(not(target_os = "linux"))]
use nix::sys::wait::WaitStatus;
use nix::sys::wait::{WaitPidFlag, waitpid};
use nix::unistd::Pid;

/// PID 1 zombie reaper. tokio's signal handler cannot cover this
/// because grandchildren of the daemon (agent-spawned helpers) re-parent
/// to PID 1 on parent death and only the init process can reap them.
/// Call once at startup; intentionally never joined — the thread dies
/// with PID 1.
pub fn install_child_reaper() {
    let open =
        jackin_telemetry::stream::phase(jackin_telemetry::schema::enums::StreamOperation::Open);
    let reaper = jackin_telemetry::spawn::thread_stream_named("zombie-reaper".into(), move || {
        loop {
            reap_zombies();
            let (ready, changed) = reaper_wake();
            let mut ready = ready
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !*ready {
                // Periodic sweeps also cover orphan exits whose SIGCHLD was
                // delivered to an earlier runtime/telemetry thread. The reaper
                // owns this OS thread; blocking never stalls a runtime worker.
                ready = changed
                    .wait_timeout(ready, std::time::Duration::from_millis(100))
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .0;
            }
            *ready = false;
        }
    });
    if let Ok(thread) = reaper {
        let _already_installed = REAPER_THREAD.set(thread);
        child_ownership::install_reaper_wakeup(wake_reaper);
        wake_reaper();
        jackin_telemetry::stream::complete_success(open);
    } else {
        record_io_error();
        jackin_telemetry::stream::complete_error(
            open,
            jackin_telemetry::schema::enums::ErrorType::IoError,
        );
    }
}

fn wake_reaper() {
    let (ready, changed) = reaper_wake();
    *ready
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
    changed.notify_one();
}

pub(crate) fn reap_zombies() {
    #[cfg(target_os = "linux")]
    {
        child_ownership::coordinate(reap_zombies_linux);
    }
    #[cfg(not(target_os = "linux"))]
    {
        child_ownership::coordinate(|registry| {
            // Platforms without Linux child enumeration cannot inspect
            // all unowned PIDs. Defer broad reaping while owners exist.
            if registry.is_empty() {
                reap_zombies_unfiltered();
            }
        });
    }
}

#[cfg(target_os = "linux")]
fn reap_zombies_linux(registry: &mut child_ownership::ChildRegistry) {
    // Enumerate every thread's direct/adopted children instead of peeking
    // P_ALL: a registered zombie at its head can otherwise hide unrelated
    // orphans indefinitely while descendants keep its owner's pipes open.
    let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
        record_io_error();
        return;
    };
    for task in tasks {
        let Ok(task) = task else {
            record_io_error();
            continue;
        };
        let children = match std::fs::read_to_string(task.path().join("children")) {
            Ok(children) => children,
            // A worker can exit between the directory listing and read.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_error) => {
                record_io_error();
                continue;
            }
        };
        for child in children.split_whitespace() {
            let Ok(pid) = child.parse::<u32>() else {
                record_io_error();
                continue;
            };
            if registry.contains(pid) {
                continue;
            }
            let Ok(pid) = i32::try_from(pid) else {
                record_io_error();
                continue;
            };
            match waitpid(Pid::from_raw(pid), Some(WaitPidFlag::WNOHANG)) {
                Ok(_) | Err(nix::errno::Errno::ECHILD) => {}
                Err(_error) => record_io_error(),
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn reap_zombies_unfiltered() {
    loop {
        match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => break,
            Ok(_) => {}
            Err(nix::errno::Errno::ECHILD) => break,
            Err(_error) => {
                record_io_error();
                break;
            }
        }
    }
}

fn record_io_error() {
    let _error =
        jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::IoError);
}

#[cfg(test)]
mod tests;

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use nix::sys::signal::{SigSet, Signal};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
use nix::sys::wait::{Id, waitid};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;

static MANAGED_CHILDREN: OnceLock<Mutex<HashSet<i32>>> = OnceLock::new();

fn managed_children() -> &'static Mutex<HashSet<i32>> {
    MANAGED_CHILDREN.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn register_managed_child(pid: u32) {
    let Ok(pid) = i32::try_from(pid) else {
        record_io_error();
        return;
    };
    match managed_children().lock() {
        Ok(mut children) => {
            children.insert(pid);
        }
        Err(_poisoned) => {
            record_panic_error();
        }
    }
}

pub fn unregister_managed_child(pid: u32) {
    let Ok(pid) = i32::try_from(pid) else {
        record_io_error();
        return;
    };
    match managed_children().lock() {
        Ok(mut children) => {
            children.remove(&pid);
        }
        Err(_poisoned) => {
            record_panic_error();
        }
    }
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn is_managed_child(pid: Pid) -> bool {
    match managed_children().lock() {
        Ok(children) => children.contains(&pid.as_raw()),
        Err(_poisoned) => {
            record_panic_error();
            false
        }
    }
}

pub fn install_sigchld_reaper() {
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGCHLD);
    if let Err(_error) = mask.thread_block() {
        record_io_error();
        return;
    }

    let open =
        jackin_telemetry::stream::phase(jackin_telemetry::schema::enums::StreamOperation::Open);
    let reaper = jackin_telemetry::spawn::thread_stream_named("zombie-reaper".into(), move || {
        let mut sigset = SigSet::empty();
        sigset.add(Signal::SIGCHLD);
        loop {
            match sigset.wait() {
                Ok(_) => reap_zombies(),
                Err(nix::errno::Errno::EINTR) => {
                    record_io_error();
                    let _retry = jackin_telemetry::record_retry_scheduled();
                }
                Err(_error) => {
                    record_io_error();
                    let _retry = jackin_telemetry::record_retry_scheduled();
                    #[expect(
                        clippy::disallowed_methods,
                        reason = "zombie reaper owns its OS thread and is not the multiplexer render thread"
                    )]
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    });
    if reaper.is_ok() {
        jackin_telemetry::stream::complete_success(open);
    } else {
        record_io_error();
        jackin_telemetry::stream::complete_error(
            open,
            jackin_telemetry::schema::enums::ErrorType::IoError,
        );
    }
}

pub(crate) fn reap_zombies() {
    #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
    {
        reap_zombies_linux();
    }
    #[cfg(not(all(target_os = "linux", not(target_env = "uclibc"))))]
    {
        reap_zombies_unfiltered();
    }
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn reap_zombies_linux() {
    let flags = WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT;
    let mut skipped: Option<HashSet<i32>> = None;
    loop {
        match waitid(Id::All, flags) {
            Ok(WaitStatus::StillAlive) | Err(nix::errno::Errno::ECHILD) => break,
            Ok(status) => {
                let Some(pid) = status.pid() else {
                    break;
                };
                if is_managed_child(pid) {
                    let set = skipped.get_or_insert_with(HashSet::new);
                    if !set.insert(pid.as_raw()) {
                        break;
                    }
                    continue;
                }
                match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::StillAlive) => break,
                    Ok(_) => {}
                    Err(nix::errno::Errno::ECHILD) => break,
                    Err(_error) => {
                        record_io_error();
                        break;
                    }
                }
            }
            Err(_error) => {
                record_io_error();
                break;
            }
        }
    }
}

#[cfg(not(all(target_os = "linux", not(target_env = "uclibc"))))]
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

fn record_panic_error() {
    let _error = jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::Panic);
}

#[cfg(test)]
mod tests;

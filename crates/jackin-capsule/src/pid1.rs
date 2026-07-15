// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! PID 1 responsibilities inside the container: reap orphaned child processes
//! and forward signals to managed children.
//!
//! Not responsible for: spawning agent processes (see `session`) or daemon
//! lifecycle (see `daemon`).
//!
//! Key invariant: every child registered via `register_managed_child` must
//! be reaped; unregistered orphans are reaped without SIGCHLD delivery.

/// PID 1 zombie reaping and signal forwarding.
///
/// Linux: when a process whose parent has exited becomes an orphan, it is
/// re-parented to PID 1. PID 1 MUST call waitpid to reap those zombies or
/// they accumulate in the process table. Tokio does not do this automatically.
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
        return;
    };
    match managed_children().lock() {
        Ok(mut children) => {
            children.insert(pid);
        }
        Err(_) => {
            // Poisoned mutex: every subsequent register/unregister silently
            // no-ops, and `is_managed_child` returns false for live pids —
            // the PID-1 reaper then races session owners for their children
            // (the exact bug `reap_zombies_does_not_steal_registered_session_child`
            // pins). Surface so the operator can restart the daemon.
            crate::clog!(
                "pid1: managed_children mutex poisoned; cannot register pid {pid}. Reaper may steal session children."
            );
        }
    }
}

pub fn unregister_managed_child(pid: u32) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    match managed_children().lock() {
        Ok(mut children) => {
            children.remove(&pid);
        }
        Err(_) => {
            crate::clog!("pid1: managed_children mutex poisoned; cannot unregister pid {pid}");
        }
    }
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn is_managed_child(pid: Pid) -> bool {
    managed_children()
        .lock()
        .is_ok_and(|children| children.contains(&pid.as_raw()))
}

/// PID 1 zombie reaper. tokio's signal handler cannot cover this
/// because grandchildren of the daemon (agent-spawned helpers) re-parent
/// to PID 1 on parent death and only the init process can reap them.
/// Call once at startup; intentionally never joined — the thread dies
/// with PID 1.
pub fn install_sigchld_reaper() {
    // Block SIGCHLD in the main thread; the dedicated reaper thread uses
    // sigwait so it wakes only on SIGCHLD without racing with tokio's signal
    // machinery. thread_block failure is a programming error (invalid
    // sigset) — expect rather than silently drop so the daemon does
    // not start with a half-installed handler.
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGCHLD);
    if let Err(error) = mask.thread_block() {
        crate::clog!("failed to block SIGCHLD on PID 1 main thread: {error}");
        return;
    }

    let reaper = jackin_telemetry::spawn::thread_stream_named("zombie-reaper".into(), move || {
        let mut sigset = SigSet::empty();
        sigset.add(Signal::SIGCHLD);
        loop {
            // Block until SIGCHLD arrives. sigwait can return EINTR
            // on signal-handler interrupt — sleep briefly so a tight
            // loop does not hammer the kernel queue, then retry. A
            // non-EINTR error is unexpected (corrupt sigset, ENOSYS
            // on a stripped kernel) and warrants a log line.
            match sigset.wait() {
                Ok(_) => reap_zombies(),
                Err(nix::errno::Errno::EINTR) => {}
                Err(e) => {
                    crate::clog!("zombie-reaper sigwait error: {e}; backing off 100ms");
                    #[expect(
                        clippy::disallowed_methods,
                        reason = "zombie reaper owns its OS thread and is not the multiplexer render thread"
                    )]
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    });
    if let Err(error) = reaper {
        crate::clog!("failed to spawn zombie-reaper thread: {error}");
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
    // WNOWAIT leaves a peeked child waitable, so waitid(Id::All) keeps
    // returning the same managed pid head-of-queue. Track skipped pids
    // and break only when the kernel re-presents one we already saw —
    // that means every remaining zombie is managed and orphans behind
    // them (if any) cannot be reached via Id::All peek.
    // Lazy: most SIGCHLD wakes find no zombies (ECHILD on the first
    // waitid). Skipping the HashSet allocation in that path keeps the
    // per-signal cost flat.
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
                    Err(e) => {
                        crate::clog!(
                            "pid1: waitpid({pid}) failed unexpectedly: {e} (errno={:?})",
                            e as i32
                        );
                        break;
                    }
                }
            }
            // ECHILD already matched above. Any other errno indicates a
            // kernel/libc bug (EINVAL, EFAULT) we cannot recover from —
            // log so triage isn't blind, then break to avoid spinning.
            Err(e) => {
                crate::clog!(
                    "pid1: waitid(Id::All) failed unexpectedly: {e} (errno={:?})",
                    e as i32
                );
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
            Err(e) => {
                crate::clog!(
                    "pid1: waitpid(-1) failed unexpectedly: {e} (errno={:?})",
                    e as i32
                );
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests;

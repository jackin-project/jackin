// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! PID 1 responsibilities inside Capsule: reap orphans and route SIGCHLD wakes.

use nix::sys::signal::{SigSet, Signal};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
use std::collections::HashSet;

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
use nix::sys::wait::{Id, waitid};

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
const SELECTIVE_PASS_BUDGET: usize = 256;
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
const SELECTIVE_EINTR_BUDGET: usize = 16;
const REAPER_RETRY_BACKOFF: std::time::Duration = std::time::Duration::from_millis(100);

fn wake_reaper() {
    if nix::sys::signal::kill(Pid::this(), Signal::SIGCHLD).is_err() {
        record_io_error();
    }
}

/// Start the process-lifetime SIGCHLD reaper.
pub fn install_sigchld_reaper() {
    if jackin_process::has_reaper_wakeup() {
        record_panic_error();
        return;
    }

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
                    backoff_reaper_retry();
                }
            }
        }
    });

    match reaper {
        Ok(_thread) => {
            if jackin_process::install_reaper_wakeup(wake_reaper).is_err() {
                record_panic_error();
                jackin_telemetry::stream::complete_error(
                    open,
                    jackin_telemetry::schema::enums::ErrorType::Panic,
                );
                return;
            }
            jackin_telemetry::stream::complete_success(open);
            // A child reservation may have been released before callback
            // publication, or SIGCHLD may have coalesced during thread start.
            reap_zombies();
        }
        Err(_error) => {
            record_io_error();
            jackin_telemetry::stream::complete_error(
                open,
                jackin_telemetry::schema::enums::ErrorType::IoError,
            );
        }
    }
}

pub(crate) fn reap_zombies() {
    #[cfg(any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    ))]
    {
        reap_zombies_selective();
    }
    #[cfg(not(any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )))]
    {
        // Keep the existing unfiltered behavior on platforms without an
        // admitted selective wait API. Strict spooled execution rejects these
        // targets before spawning its held anchor.
        reap_zombies_unfiltered();
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn reap_zombies_selective() {
    let mut observer = NoopReaperObserver;
    let pass = jackin_process::coordinate_children(|owned_children| {
        run_selective_pass(owned_children, &mut observer, &mut SystemReaperWaitBackend)
    });
    let poisoned = jackin_process::take_registry_poisoned();
    if pass.failure.is_some() {
        record_io_error();
    } else if poisoned {
        record_panic_error();
    }
    if pass.requeue && jackin_process::has_reaper_wakeup() {
        let _retry = jackin_telemetry::record_retry_scheduled();
        backoff_reaper_retry();
        wake_reaper();
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
pub(crate) trait ReaperObserver {
    fn selected(&mut self, _pid: Pid, _managed: bool) {}
    fn reaped(&mut self, _pid: Pid) {}
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
trait ReaperWaitBackend {
    fn peek(&mut self) -> nix::Result<Option<Pid>>;
    fn reap(&mut self, pid: Pid) -> nix::Result<WaitStatus>;
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
struct SystemReaperWaitBackend;

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl ReaperWaitBackend for SystemReaperWaitBackend {
    fn peek(&mut self) -> nix::Result<Option<Pid>> {
        peek_exited_child()
    }

    fn reap(&mut self, pid: Pid) -> nix::Result<WaitStatus> {
        waitpid(pid, Some(WaitPidFlag::WNOHANG))
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectivePass {
    failure: Option<nix::errno::Errno>,
    requeue: bool,
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl SelectivePass {
    const fn complete() -> Self {
        Self {
            failure: None,
            requeue: false,
        }
    }

    const fn retry(failure: Option<nix::errno::Errno>) -> Self {
        Self {
            failure,
            requeue: true,
        }
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
struct NoopReaperObserver;

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl ReaperObserver for NoopReaperObserver {}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[cfg(test)]
pub(crate) fn reap_zombies_selective_with_observer(
    observer: &mut impl ReaperObserver,
) -> Option<nix::errno::Errno> {
    jackin_process::coordinate_children(|owned_children| {
        run_selective_pass(owned_children, observer, &mut SystemReaperWaitBackend)
    })
    .failure
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn run_selective_pass(
    owned_children: &jackin_process::ChildRegistry,
    observer: &mut impl ReaperObserver,
    backend: &mut impl ReaperWaitBackend,
) -> SelectivePass {
    let mut seen = HashSet::new();
    for _ in 0..SELECTIVE_PASS_BUDGET {
        let pid = match retry_interrupted(|| backend.peek()) {
            Ok(Some(pid)) => pid,
            Ok(None) | Err(nix::errno::Errno::ECHILD) => return SelectivePass::complete(),
            Err(error) => return SelectivePass::retry(Some(error)),
        };
        let raw_pid = pid.as_raw();
        let is_managed =
            u32::try_from(raw_pid).is_ok_and(|child_pid| owned_children.contains(child_pid));
        observer.selected(pid, is_managed);
        if is_managed {
            if !seen.insert(raw_pid) {
                return SelectivePass::complete();
            }
            continue;
        }

        match retry_interrupted(|| backend.reap(pid)) {
            Ok(WaitStatus::StillAlive) | Err(nix::errno::Errno::ECHILD) => {
                if !seen.insert(raw_pid) {
                    return SelectivePass::complete();
                }
            }
            Ok(_) => observer.reaped(pid),
            Err(error) => return SelectivePass::retry(Some(error)),
        }
    }
    SelectivePass::retry(None)
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn retry_interrupted<T>(mut operation: impl FnMut() -> nix::Result<T>) -> nix::Result<T> {
    let mut retries = 0;
    loop {
        match operation() {
            Err(nix::errno::Errno::EINTR) if retries < SELECTIVE_EINTR_BUDGET => {
                retries += 1;
            }
            result => return result,
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "the dedicated PID 1 reaper waits between retrying a consumed signal or selective pass"
)]
fn backoff_reaper_retry() {
    std::thread::sleep(REAPER_RETRY_BACKOFF);
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
const TEST_CHILD_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(5);

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
#[derive(Clone, Copy)]
pub(crate) enum TestChildSelector {
    Specific,
    Any,
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
#[expect(
    clippy::disallowed_methods,
    reason = "bounded PID 1 test polling observes one known child without consuming its status"
)]
pub(crate) fn wait_for_child_exit(pid: u32, selector: TestChildSelector) -> std::io::Result<()> {
    let raw_pid = i32::try_from(pid).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "known child PID is outside the waitid range",
        )
    })?;
    let child_pid = Pid::from_raw(raw_pid);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
        let selected = {
            // Linux can target the known child directly for both test roles.
            let _selector = selector;
            match waitid(
                Id::Pid(child_pid),
                WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
            ) {
                Ok(WaitStatus::StillAlive) => false,
                Ok(status) => status.pid() == Some(child_pid),
                Err(error) => {
                    return Err(std::io::Error::other(format!(
                        "peek known child {child_pid}: {error}"
                    )));
                }
            }
        };
        #[cfg(target_os = "macos")]
        let selected = match selector {
            TestChildSelector::Specific => {
                jackin_process_directory::peek_specific_exited_child(pid).map_err(|error| {
                    std::io::Error::other(format!("peek known child {child_pid}: {error}"))
                })?
            }
            TestChildSelector::Any => {
                jackin_process_directory::peek_exited_child().map_err(|error| {
                    std::io::Error::other(format!("peek known child {child_pid}: {error}"))
                })? == Some(pid)
            }
        };
        if selected {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("child {child_pid} did not exit before deadline"),
            ));
        }
        std::thread::sleep(TEST_CHILD_POLL_INTERVAL.min(remaining));
    }
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn peek_exited_child() -> nix::Result<Option<Pid>> {
    let flags = WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT;
    match waitid(Id::All, flags)? {
        WaitStatus::StillAlive => Ok(None),
        status => Ok(status.pid()),
    }
}

#[cfg(target_os = "macos")]
fn peek_exited_child() -> nix::Result<Option<Pid>> {
    jackin_process_directory::peek_exited_child()
        .map_err(|error| match error.raw_os_error() {
            Some(code) => nix::errno::Errno::from_raw(code),
            None => nix::errno::Errno::EIO,
        })
        .and_then(|pid| match pid {
            Some(pid) => i32::try_from(pid)
                .map(Pid::from_raw)
                .map(Some)
                .map_err(|_error| nix::errno::Errno::EINVAL),
            None => Ok(None),
        })
}

#[cfg(not(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
)))]
fn reap_zombies_unfiltered() {
    loop {
        match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) | Err(nix::errno::Errno::ECHILD) => break,
            Ok(_) => {}
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

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! PID 1 responsibilities inside Capsule: reap orphans and route SIGCHLD wakes.

use nix::sys::signal::{SigSet, Signal};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};
#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
use std::sync::{Arc, OnceLock};
#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
use std::time::Instant;

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

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
pub(crate) const REAPER_TEST_EVENT_CAPACITY: usize = 256;

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReaperTestWaitStatus {
    Exited(i32),
    Signaled(i32),
    StillAlive,
    Other,
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReaperTestEventKind {
    SigwaitSucceeded {
        signal: i32,
    },
    PeekEintrInjected {
        ordinal: usize,
    },
    Selected {
        pid: i32,
        managed: bool,
    },
    TargetedWaitResult {
        pid: i32,
        status: ReaperTestWaitStatus,
    },
    TargetedWaitError {
        pid: i32,
        errno: i32,
    },
    Reaped {
        pid: i32,
    },
    PassDisposition {
        failure_errno: Option<i32>,
        requeue: bool,
    },
    BackoffStarted,
    BackoffCompleted,
    WakeAttempt {
        process_pid: i32,
        signal: i32,
        succeeded: bool,
        errno: Option<i32>,
    },
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReaperTestEvent {
    pub(crate) elapsed: std::time::Duration,
    pub(crate) kind: ReaperTestEventKind,
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
struct ReaperTestState {
    started: Instant,
    events: SyncSender<ReaperTestEvent>,
    overflow: Arc<AtomicBool>,
    injection_total: usize,
    injection_remaining: AtomicUsize,
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
static REAPER_TEST_STATE: OnceLock<ReaperTestState> = OnceLock::new();

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
const REAPER_TEST_DIAGNOSTIC_UNRECORDED: i32 = i32::MIN;

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
struct ReaperTestDiagnosticState {
    thread_entered: AtomicBool,
    thread_sigchld_blocked: AtomicBool,
    thread_mask_error: AtomicI32,
    sigwait_entries: AtomicUsize,
    sigwait_successes: AtomicUsize,
    sigwait_errors: AtomicUsize,
    last_sigwait_error: AtomicI32,
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
static REAPER_TEST_DIAGNOSTIC: ReaperTestDiagnosticState = ReaperTestDiagnosticState {
    thread_entered: AtomicBool::new(false),
    thread_sigchld_blocked: AtomicBool::new(false),
    thread_mask_error: AtomicI32::new(REAPER_TEST_DIAGNOSTIC_UNRECORDED),
    sigwait_entries: AtomicUsize::new(0),
    sigwait_successes: AtomicUsize::new(0),
    sigwait_errors: AtomicUsize::new(0),
    last_sigwait_error: AtomicI32::new(REAPER_TEST_DIAGNOSTIC_UNRECORDED),
};

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReaperTestDiagnosticSnapshot {
    pub(crate) thread_entered: bool,
    pub(crate) thread_sigchld_blocked: Option<bool>,
    pub(crate) thread_mask_error: Option<i32>,
    pub(crate) sigwait_entries: usize,
    pub(crate) sigwait_successes: usize,
    pub(crate) sigwait_errors: usize,
    pub(crate) last_sigwait_error: Option<i32>,
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
pub(crate) fn reaper_test_diagnostic_snapshot() -> ReaperTestDiagnosticSnapshot {
    let thread_entered = REAPER_TEST_DIAGNOSTIC
        .thread_entered
        .load(Ordering::Acquire);
    let thread_mask_error = REAPER_TEST_DIAGNOSTIC
        .thread_mask_error
        .load(Ordering::Relaxed);
    let thread_mask_error =
        (thread_mask_error != REAPER_TEST_DIAGNOSTIC_UNRECORDED).then_some(thread_mask_error);
    let sigwait_entries = REAPER_TEST_DIAGNOSTIC
        .sigwait_entries
        .load(Ordering::Acquire);
    let sigwait_successes = REAPER_TEST_DIAGNOSTIC
        .sigwait_successes
        .load(Ordering::Acquire);
    let sigwait_errors = REAPER_TEST_DIAGNOSTIC
        .sigwait_errors
        .load(Ordering::Acquire);
    let last_sigwait_error = REAPER_TEST_DIAGNOSTIC
        .last_sigwait_error
        .load(Ordering::Relaxed);
    let last_sigwait_error =
        (last_sigwait_error != REAPER_TEST_DIAGNOSTIC_UNRECORDED).then_some(last_sigwait_error);

    ReaperTestDiagnosticSnapshot {
        thread_entered,
        thread_sigchld_blocked: (thread_entered && thread_mask_error.is_none()).then(|| {
            REAPER_TEST_DIAGNOSTIC
                .thread_sigchld_blocked
                .load(Ordering::Relaxed)
        }),
        thread_mask_error,
        sigwait_entries,
        sigwait_successes,
        sigwait_errors,
        last_sigwait_error,
    }
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
fn record_reaper_test_thread_entry() {
    match SigSet::thread_get_mask() {
        Ok(mask) => REAPER_TEST_DIAGNOSTIC
            .thread_sigchld_blocked
            .store(mask.contains(Signal::SIGCHLD), Ordering::Relaxed),
        Err(error) => REAPER_TEST_DIAGNOSTIC
            .thread_mask_error
            .store(error as i32, Ordering::Relaxed),
    }
    REAPER_TEST_DIAGNOSTIC
        .thread_entered
        .store(true, Ordering::Release);
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
fn record_reaper_test_sigwait_error(error: nix::errno::Errno) {
    REAPER_TEST_DIAGNOSTIC
        .last_sigwait_error
        .store(error as i32, Ordering::Relaxed);
    REAPER_TEST_DIAGNOSTIC
        .sigwait_errors
        .fetch_add(1, Ordering::Release);
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
pub(crate) struct ReaperTestCapture {
    receiver: Receiver<ReaperTestEvent>,
    overflow: Arc<AtomicBool>,
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
impl ReaperTestCapture {
    pub(crate) fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<ReaperTestEvent, std::sync::mpsc::RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    pub(crate) fn try_recv(&self) -> Result<ReaperTestEvent, std::sync::mpsc::TryRecvError> {
        self.receiver.try_recv()
    }

    pub(crate) fn overflowed(&self) -> bool {
        self.overflow.load(Ordering::Acquire)
    }
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
pub(crate) fn arm_reaper_test_capture(injected_peek_eintr: usize) -> ReaperTestCapture {
    assert!(
        injected_peek_eintr == 0 || injected_peek_eintr == SELECTIVE_EINTR_BUDGET + 1,
        "isolated fixtures may arm only zero or one finite exhausted EINTR sequence"
    );
    let (events, receiver) = sync_channel(REAPER_TEST_EVENT_CAPACITY);
    let overflow = Arc::new(AtomicBool::new(false));
    let state = ReaperTestState {
        started: Instant::now(),
        events,
        overflow: Arc::clone(&overflow),
        injection_total: injected_peek_eintr,
        injection_remaining: AtomicUsize::new(injected_peek_eintr),
    };
    assert!(
        REAPER_TEST_STATE.set(state).is_ok(),
        "one isolated fixture capture is allowed per worker process"
    );
    ReaperTestCapture { receiver, overflow }
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
fn record_reaper_test_event(kind: ReaperTestEventKind) {
    let Some(state) = REAPER_TEST_STATE.get() else {
        return;
    };
    let event = ReaperTestEvent {
        elapsed: state.started.elapsed(),
        kind,
    };
    if state.events.try_send(event).is_err() {
        state.overflow.store(true, Ordering::Release);
    }
}

#[cfg(all(
    test,
    any(
        all(target_os = "linux", not(target_env = "uclibc")),
        target_os = "macos"
    )
))]
fn injected_test_peek_eintr() -> Option<usize> {
    let state = REAPER_TEST_STATE.get()?;
    let mut remaining = state.injection_remaining.load(Ordering::Acquire);
    loop {
        if remaining == 0 {
            return None;
        }
        match state.injection_remaining.compare_exchange_weak(
            remaining,
            remaining - 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                let ordinal = state.injection_total - remaining + 1;
                record_reaper_test_event(ReaperTestEventKind::PeekEintrInjected { ordinal });
                return Some(ordinal);
            }
            Err(current) => remaining = current,
        }
    }
}

fn wake_reaper() {
    let process_pid = Pid::this();
    let result = nix::sys::signal::kill(process_pid, Signal::SIGCHLD);
    #[cfg(all(
        test,
        any(
            all(target_os = "linux", not(target_env = "uclibc")),
            target_os = "macos"
        )
    ))]
    record_reaper_test_event(ReaperTestEventKind::WakeAttempt {
        process_pid: process_pid.as_raw(),
        signal: Signal::SIGCHLD as i32,
        succeeded: result.is_ok(),
        errno: result.as_ref().err().map(|error| *error as i32),
    });
    if result.is_err() {
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
        #[cfg(all(
            test,
            any(
                all(target_os = "linux", not(target_env = "uclibc")),
                target_os = "macos"
            )
        ))]
        record_reaper_test_thread_entry();
        let mut sigset = SigSet::empty();
        sigset.add(Signal::SIGCHLD);
        loop {
            #[cfg(all(
                test,
                any(
                    all(target_os = "linux", not(target_env = "uclibc")),
                    target_os = "macos"
                )
            ))]
            REAPER_TEST_DIAGNOSTIC
                .sigwait_entries
                .fetch_add(1, Ordering::Relaxed);
            match sigset.wait() {
                Ok(signal) => {
                    #[cfg(all(
                        test,
                        any(
                            all(target_os = "linux", not(target_env = "uclibc")),
                            target_os = "macos"
                        )
                    ))]
                    REAPER_TEST_DIAGNOSTIC
                        .sigwait_successes
                        .fetch_add(1, Ordering::Relaxed);
                    #[cfg(all(
                        test,
                        any(
                            all(target_os = "linux", not(target_env = "uclibc")),
                            target_os = "macos"
                        )
                    ))]
                    record_reaper_test_event(ReaperTestEventKind::SigwaitSucceeded {
                        signal: signal as i32,
                    });
                    let _ = signal;
                    reap_zombies();
                }
                Err(nix::errno::Errno::EINTR) => {
                    #[cfg(all(
                        test,
                        any(
                            all(target_os = "linux", not(target_env = "uclibc")),
                            target_os = "macos"
                        )
                    ))]
                    record_reaper_test_sigwait_error(nix::errno::Errno::EINTR);
                    record_io_error();
                    let _retry = jackin_telemetry::record_retry_scheduled();
                }
                Err(error) => {
                    #[cfg(all(
                        test,
                        any(
                            all(target_os = "linux", not(target_env = "uclibc")),
                            target_os = "macos"
                        )
                    ))]
                    record_reaper_test_sigwait_error(error);
                    #[cfg(not(all(
                        test,
                        any(
                            all(target_os = "linux", not(target_env = "uclibc")),
                            target_os = "macos"
                        )
                    )))]
                    let _ = error;
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
    #[cfg(test)]
    record_reaper_test_event(ReaperTestEventKind::PassDisposition {
        failure_errno: pass.failure.map(|error| error as i32),
        requeue: pass.requeue,
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
        #[cfg(test)]
        if injected_test_peek_eintr().is_some() {
            return Err(nix::errno::Errno::EINTR);
        }
        peek_exited_child()
    }

    fn reap(&mut self, pid: Pid) -> nix::Result<WaitStatus> {
        let result = waitpid(pid, Some(WaitPidFlag::WNOHANG));
        #[cfg(test)]
        match &result {
            Ok(status) => record_reaper_test_event(ReaperTestEventKind::TargetedWaitResult {
                pid: pid.as_raw(),
                status: match status {
                    WaitStatus::Exited(_, code) => ReaperTestWaitStatus::Exited(*code),
                    WaitStatus::Signaled(_, signal, _) => {
                        ReaperTestWaitStatus::Signaled(*signal as i32)
                    }
                    WaitStatus::StillAlive => ReaperTestWaitStatus::StillAlive,
                    _ => ReaperTestWaitStatus::Other,
                },
            }),
            Err(error) => record_reaper_test_event(ReaperTestEventKind::TargetedWaitError {
                pid: pid.as_raw(),
                errno: *error as i32,
            }),
        }
        result
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
impl ReaperObserver for NoopReaperObserver {
    fn selected(&mut self, pid: Pid, managed: bool) {
        #[cfg(test)]
        record_reaper_test_event(ReaperTestEventKind::Selected {
            pid: pid.as_raw(),
            managed,
        });
        #[cfg(not(test))]
        let _ = (pid, managed);
    }

    fn reaped(&mut self, pid: Pid) {
        #[cfg(test)]
        record_reaper_test_event(ReaperTestEventKind::Reaped { pid: pid.as_raw() });
        #[cfg(not(test))]
        let _ = pid;
    }
}

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
    #[cfg(all(
        test,
        any(
            all(target_os = "linux", not(target_env = "uclibc")),
            target_os = "macos"
        )
    ))]
    record_reaper_test_event(ReaperTestEventKind::BackoffStarted);
    std::thread::sleep(REAPER_RETRY_BACKOFF);
    #[cfg(all(
        test,
        any(
            all(target_os = "linux", not(target_env = "uclibc")),
            target_os = "macos"
        )
    ))]
    record_reaper_test_event(ReaperTestEventKind::BackoffCompleted);
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
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
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
        let remaining = deadline.saturating_duration_since(Instant::now());
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

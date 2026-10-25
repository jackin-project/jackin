// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use nix::sys::signal::{SigSet, Signal};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;

#[cfg(target_os = "macos")]
use tokio::signal::unix::{Signal as TokioSignal, SignalKind};

#[cfg(target_os = "macos")]
use std::sync::OnceLock;
#[cfg(target_os = "macos")]
use std::sync::atomic::Ordering;
#[cfg(target_os = "macos")]
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

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

#[cfg(target_os = "macos")]
const DARWIN_REAPER_STARTUP_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);
#[cfg(target_os = "macos")]
const DARWIN_REAPER_UNPREPARED: u8 = 0;
#[cfg(target_os = "macos")]
const DARWIN_REAPER_PREPARING: u8 = 1;
#[cfg(target_os = "macos")]
const DARWIN_REAPER_REGISTRATION_ATTEMPTED: u8 = 2;
#[cfg(target_os = "macos")]
const DARWIN_REAPER_WORKER_READY: u8 = 3;
#[cfg(target_os = "macos")]
const DARWIN_REAPER_ACTIVATING: u8 = 4;
#[cfg(target_os = "macos")]
const DARWIN_REAPER_ACTIVE: u8 = 5;

#[cfg(target_os = "macos")]
static DARWIN_REAPER_STATE: std::sync::atomic::AtomicU8 =
    std::sync::atomic::AtomicU8::new(DARWIN_REAPER_UNPREPARED);

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum DarwinReaperPhase {
    ListenerPolledReady,
    Active,
}

#[cfg(target_os = "macos")]
struct DarwinReaperOwner {
    _worker: std::thread::JoinHandle<()>,
    activation: Option<tokio::sync::oneshot::Sender<()>>,
    acknowledgements: Receiver<DarwinReaperPhase>,
}

#[cfg(target_os = "macos")]
type DarwinReaperOwnerSlot = std::sync::Mutex<Option<DarwinReaperOwner>>;

#[cfg(target_os = "macos")]
static DARWIN_REAPER_OWNER: OnceLock<DarwinReaperOwnerSlot> = OnceLock::new();

fn wake_reaper() {
    let process_pid = Pid::this();
    let result = nix::sys::signal::kill(process_pid, Signal::SIGCHLD);

    if result.is_err() {
        record_io_error();
    }
}

#[cfg(target_os = "macos")]
fn darwin_reaper_preflight() -> anyhow::Result<()> {
    anyhow::ensure!(
        !jackin_process::has_reaper_wakeup(),
        "SIGCHLD reaper wake callback is already installed"
    );
    let eligibility = jackin_process_directory::tokio_sigchld_eligibility()?;
    anyhow::ensure!(
        eligibility == jackin_process_directory::TokioSigchldEligibility::Eligible,
        "SIGCHLD action is not eligible for Tokio observation: {eligibility:?}"
    );
    let mask = SigSet::thread_get_mask()
        .map_err(|error| anyhow::anyhow!("query initiating SIGCHLD mask: {error}"))?;
    anyhow::ensure!(
        !mask.contains(Signal::SIGCHLD),
        "initiating thread blocks SIGCHLD"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn claim_darwin_reaper_preparation() -> anyhow::Result<()> {
    if let Err(state) = DARWIN_REAPER_STATE.compare_exchange(
        DARWIN_REAPER_UNPREPARED,
        DARWIN_REAPER_PREPARING,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        if state >= DARWIN_REAPER_REGISTRATION_ATTEMPTED {
            darwin_reaper_fatal("duplicate prepare after registration was attempted");
        }
        anyhow::bail!("Darwin SIGCHLD owner preparation is already in progress");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn darwin_reaper_owner_slot() -> &'static DarwinReaperOwnerSlot {
    DARWIN_REAPER_OWNER.get_or_init(|| std::sync::Mutex::new(None))
}

#[cfg(target_os = "macos")]
fn ensure_darwin_owner_slot_empty(slot: &DarwinReaperOwnerSlot) -> anyhow::Result<()> {
    let existing = slot
        .lock()
        .map_err(|_| anyhow::anyhow!("Darwin SIGCHLD owner lock is poisoned"))?
        .is_some();
    if existing {
        DARWIN_REAPER_STATE.store(DARWIN_REAPER_UNPREPARED, Ordering::Release);
        anyhow::bail!("Darwin SIGCHLD owner already exists");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn reset_darwin_reaper_before_registration() {
    DARWIN_REAPER_STATE.store(DARWIN_REAPER_UNPREPARED, Ordering::Release);
}

#[cfg(target_os = "macos")]
#[expect(
    clippy::exit,
    reason = "irreversible SIGCHLD registration requires process-fatal worker failure even while main is blocked"
)]
fn darwin_reaper_fatal(message: &str) -> ! {
    use std::io::Write as _;

    let _reported = writeln!(
        std::io::stderr().lock(),
        "PID1_DARWIN_TOKIO_FATAL pid={} error={message}",
        std::process::id()
    );
    std::process::exit(1)
}

#[cfg(target_os = "macos")]
struct DarwinRegistrationFatalGuard {
    armed: bool,
}

#[cfg(target_os = "macos")]
impl Drop for DarwinRegistrationFatalGuard {
    fn drop(&mut self) {
        if self.armed {
            darwin_reaper_fatal("startup failed after Tokio SIGCHLD registration was attempted");
        }
    }
}

#[cfg(target_os = "macos")]
struct DarwinWorkerFatalGuard;

#[cfg(target_os = "macos")]
impl Drop for DarwinWorkerFatalGuard {
    fn drop(&mut self) {
        darwin_reaper_fatal("dedicated Tokio signal worker returned or unwound");
    }
}

#[cfg(target_os = "macos")]
#[expect(
    clippy::print_stderr,
    reason = "isolated Darwin qualification requires the actual post-registration PID before fatal startup failure"
)]
fn register_darwin_signal_listener(
    runtime: &tokio::runtime::Runtime,
) -> anyhow::Result<(TokioSignal, DarwinRegistrationFatalGuard)> {
    DARWIN_REAPER_STATE.store(DARWIN_REAPER_REGISTRATION_ATTEMPTED, Ordering::Release);
    let fatal_guard = DarwinRegistrationFatalGuard { armed: true };
    let listener = {
        let _entered = runtime.enter();
        tokio::signal::unix::signal(SignalKind::child())?
    };
    Ok((listener, fatal_guard))
}

#[cfg(target_os = "macos")]
fn spawn_darwin_reaper_worker(
    runtime: tokio::runtime::Runtime,
    listener: TokioSignal,
    activation: tokio::sync::oneshot::Receiver<()>,
    acknowledgements: SyncSender<DarwinReaperPhase>,
) -> anyhow::Result<std::thread::JoinHandle<()>> {
    Ok(jackin_telemetry::spawn::thread_stream_named(
        "zombie-reaper".into(),
        move || {
            let _fatal_guard = DarwinWorkerFatalGuard;
            let result = runtime.block_on(run_darwin_reaper_worker(
                listener,
                activation,
                acknowledgements,
            ));
            match result {
                Ok(()) => darwin_reaper_fatal("dedicated Tokio signal worker returned"),
                Err(error) => {
                    darwin_reaper_fatal(&format!("dedicated Tokio worker failed: {error:#}"))
                }
            }
        },
    )?)
}

#[cfg(target_os = "macos")]
fn publish_darwin_reaper_owner(
    slot: &DarwinReaperOwnerSlot,
    worker: std::thread::JoinHandle<()>,
    activation: tokio::sync::oneshot::Sender<()>,
    acknowledgements: Receiver<DarwinReaperPhase>,
) {
    let mut slot = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if slot.is_some() {
        darwin_reaper_fatal("owner slot was occupied after registration");
    }
    *slot = Some(DarwinReaperOwner {
        _worker: worker,
        activation: Some(activation),
        acknowledgements,
    });
}

#[cfg(target_os = "macos")]
fn wait_for_darwin_listener_ready(slot: &DarwinReaperOwnerSlot) {
    let mut slot = slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let owner = slot
        .as_mut()
        .unwrap_or_else(|| darwin_reaper_fatal("published owner disappeared before readiness"));
    match owner
        .acknowledgements
        .recv_timeout(DARWIN_REAPER_STARTUP_DEADLINE)
    {
        Ok(DarwinReaperPhase::ListenerPolledReady) => {}
        Ok(DarwinReaperPhase::Active) => {
            darwin_reaper_fatal("worker became active before main published the wake callback");
        }
        Err(error) => darwin_reaper_fatal(&format!("listener readiness failed: {error}")),
    }
}

#[cfg(target_os = "macos")]
async fn run_darwin_reaper_worker(
    mut signal: TokioSignal,
    activation: tokio::sync::oneshot::Receiver<()>,
    acknowledgements: SyncSender<DarwinReaperPhase>,
) -> anyhow::Result<()> {
    use std::future::poll_fn;
    use std::task::Poll;

    let listener_polled = poll_fn(|context| match signal.poll_recv(context) {
        Poll::Pending => Poll::Ready(true),
        Poll::Ready(Some(())) => Poll::Ready(true),
        Poll::Ready(None) => Poll::Ready(false),
    })
    .await;
    anyhow::ensure!(
        listener_polled,
        "Tokio SIGCHLD stream ended during readiness poll"
    );
    acknowledgements
        .send(DarwinReaperPhase::ListenerPolledReady)
        .map_err(|_| anyhow::anyhow!("SIGCHLD readiness receiver was lost"))?;

    let mut activation = activation;
    loop {
        tokio::select! {
            result = &mut activation => {
                result.map_err(|_| anyhow::anyhow!("SIGCHLD activation sender was lost"))?;
                break;
            }
            notification = signal.recv() => {
                if notification.is_none() {
                    anyhow::bail!("Tokio SIGCHLD stream ended before activation");
                }
            }
        }
    }

    reap_zombies();
    DARWIN_REAPER_STATE.store(DARWIN_REAPER_ACTIVE, Ordering::Release);
    acknowledgements
        .send(DarwinReaperPhase::Active)
        .map_err(|_| anyhow::anyhow!("SIGCHLD active receiver was lost"))?;

    loop {
        if signal.recv().await.is_none() {
            anyhow::bail!("Tokio SIGCHLD stream ended while the owner was active");
        }
        reap_zombies();
    }
}

#[cfg(target_os = "macos")]
pub fn prepare_darwin_reaper() -> anyhow::Result<()> {
    claim_darwin_reaper_preparation()?;
    let owner_slot = darwin_reaper_owner_slot();
    ensure_darwin_owner_slot_empty(owner_slot)?;
    if let Err(error) = darwin_reaper_preflight() {
        reset_darwin_reaper_before_registration();
        return Err(error);
    }

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            reset_darwin_reaper_before_registration();
            return Err(error.into());
        }
    };
    let (activation_sender, activation_receiver) = tokio::sync::oneshot::channel();
    let (acknowledgements_sender, acknowledgements_receiver) = sync_channel(2);

    let (listener, mut fatal_guard) = register_darwin_signal_listener(&runtime)?;
    let worker = spawn_darwin_reaper_worker(
        runtime,
        listener,
        activation_receiver,
        acknowledgements_sender,
    )?;
    publish_darwin_reaper_owner(
        owner_slot,
        worker,
        activation_sender,
        acknowledgements_receiver,
    );
    wait_for_darwin_listener_ready(owner_slot);
    DARWIN_REAPER_STATE.store(DARWIN_REAPER_WORKER_READY, Ordering::Release);
    fatal_guard.armed = false;
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn activate_darwin_reaper() {
    if DARWIN_REAPER_STATE
        .compare_exchange(
            DARWIN_REAPER_WORKER_READY,
            DARWIN_REAPER_ACTIVATING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        darwin_reaper_fatal("activation did not follow worker readiness");
    }
    let owner_slot = DARWIN_REAPER_OWNER
        .get()
        .unwrap_or_else(|| darwin_reaper_fatal("prepared owner slot is missing"));
    let mut slot = owner_slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let owner = slot
        .as_mut()
        .unwrap_or_else(|| darwin_reaper_fatal("prepared owner is missing"));
    if jackin_process::install_reaper_wakeup(wake_reaper).is_err() {
        darwin_reaper_fatal("existing SIGCHLD wake callback could not be published");
    }
    let activation = owner
        .activation
        .take()
        .unwrap_or_else(|| darwin_reaper_fatal("activation was already sent"));
    if activation.send(()).is_err() {
        darwin_reaper_fatal("dedicated worker lost its activation receiver");
    }
    match owner
        .acknowledgements
        .recv_timeout(DARWIN_REAPER_STARTUP_DEADLINE)
    {
        Ok(DarwinReaperPhase::Active)
            if DARWIN_REAPER_STATE.load(Ordering::Acquire) == DARWIN_REAPER_ACTIVE => {}
        Ok(_) => darwin_reaper_fatal("worker returned an unexpected activation phase"),
        Err(error) => darwin_reaper_fatal(&format!("owner activation failed: {error}")),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn require_darwin_reaper_active() -> anyhow::Result<()> {
    anyhow::ensure!(
        DARWIN_REAPER_STATE.load(Ordering::Acquire) == DARWIN_REAPER_ACTIVE,
        "daemon startup requires the active Darwin Tokio SIGCHLD owner"
    );
    Ok(())
}

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
                Ok(signal) => {
                    let _ = signal;
                    reap_zombies();
                }
                Err(nix::errno::Errno::EINTR) => {
                    record_io_error();
                    let _retry = jackin_telemetry::record_retry_scheduled();
                }
                Err(_) => {
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
impl ReaperObserver for NoopReaperObserver {
    fn selected(&mut self, pid: Pid, managed: bool) {
        let _ = (pid, managed);
    }

    fn reaped(&mut self, pid: Pid) {
        let _ = pid;
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
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

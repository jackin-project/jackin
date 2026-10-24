// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Isolated direct-child tests for PID 1's selective reaper.

use super::*;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
use std::io::{self, Write as IoWrite};
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
use std::path::{Path, PathBuf};
#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
use std::process::ExitStatus;
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
use std::time::Instant;

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
use nix::sys::signal::SigmaskHow;

const WORKER_ENV: &str = "JACKIN_PID1_TEST_WORKER";
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
const FIXTURE_DIRECTORY_ENV: &str = "JACKIN_PID1_TEST_FIXTURE_DIRECTORY";
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
const DIAGNOSTIC_PYTHON_ENV: &str = "JACKIN_PID1_DIAGNOSTIC_PYTHON";
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
const DIAGNOSTIC_SCRIPT_ENV: &str = "JACKIN_PID1_DIAGNOSTIC_PREEXEC_SCRIPT";
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
const DIAGNOSTIC_PREEXEC_PID_ENV: &str = "JACKIN_PID1_DIAGNOSTIC_PREEXEC_PID";
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
const SIGWAIT_DIAGNOSTIC_WORKER: &str = "pid1::tests::worker_production_sigwait_requeue";
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
const CAUGHT_SIGCHLD_DIAGNOSTIC_WORKER: &str =
    "pid1::tests::worker_caught_sigchld_requeue_diagnostic";
#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
const FIXTURE_REAPER_PID_ENV: &str = "JACKIN_PID1_TEST_REAPER_PID";
#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
const FIXTURE_INTERMEDIATE_PID_ENV: &str = "JACKIN_PID1_TEST_INTERMEDIATE_PID";
const MAX_WORKER_DIAGNOSTIC_BYTES: u64 = 64 * 1024;
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
const FIXTURE_CHILD_DEADLINE: Duration = Duration::from_secs(5);
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
const FIXTURE_PROOF_DEADLINE: Duration = Duration::from_secs(6);
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
const FIXTURE_POLL_INTERVAL: Duration = Duration::from_millis(5);
#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
const MAX_FIXTURE_RECORD_BYTES: u64 = 256;

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
struct PreexecDiagnosticRequest {
    python: PathBuf,
    script: PathBuf,
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn preexec_diagnostic_request(name: &str) -> io::Result<Option<PreexecDiagnosticRequest>> {
    let python = std::env::var_os(DIAGNOSTIC_PYTHON_ENV);
    let script = std::env::var_os(DIAGNOSTIC_SCRIPT_ENV);
    match (python, script) {
        (None, None) => Ok(None),
        (Some(python), Some(script)) => {
            if name != SIGWAIT_DIAGNOSTIC_WORKER && name != CAUGHT_SIGCHLD_DIAGNOSTIC_WORKER {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "pre-exec diagnostic request targets an unexpected PID 1 worker",
                ));
            }
            let python = PathBuf::from(python);
            let script = PathBuf::from(script);
            if !python.is_absolute() || !script.is_absolute() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "pre-exec diagnostic paths must be absolute",
                ));
            }
            Ok(Some(PreexecDiagnosticRequest { python, script }))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "pre-exec diagnostic request requires both Python and script paths",
        )),
    }
}

fn run_worker(name: &str) {
    let executable = std::env::current_exe().expect("test executable path");
    let mut stdout_capture = tempfile::tempfile().expect("create PID 1 worker stdout capture");
    let mut stderr_capture = tempfile::tempfile().expect("create PID 1 worker stderr capture");
    let mut child = Command::new(executable)
        .args(["--exact", name, "--nocapture"])
        .env(WORKER_ENV, name)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            stdout_capture
                .try_clone()
                .expect("clone PID 1 worker stdout capture"),
        ))
        .stderr(Stdio::from(
            stderr_capture
                .try_clone()
                .expect("clone PID 1 worker stderr capture"),
        ))
        .spawn()
        .expect("spawn isolated PID 1 test worker");
    match crate::util::poll_owned_child_until_timeout(
        &mut child,
        Duration::from_secs(8),
        Duration::from_millis(5),
    )
    .expect("poll isolated PID 1 test worker")
    {
        crate::util::ChildPollOutcome::Exited(status) => {
            let diagnostics = if status.success() {
                String::new()
            } else {
                worker_failure_diagnostics(
                    name,
                    &format!("failed with status {status}"),
                    &mut stdout_capture,
                    &mut stderr_capture,
                )
            };
            assert!(
                status.success(),
                "isolated PID 1 worker failed: {status}\n{diagnostics}"
            );
        }
        crate::util::ChildPollOutcome::TimedOut => {
            stop_known_child(&mut child);
            let diagnostics = worker_failure_diagnostics(
                name,
                "timed out after 8 seconds",
                &mut stdout_capture,
                &mut stderr_capture,
            );
            panic!("isolated PID 1 worker exceeded its test deadline\n{diagnostics}");
        }
    }
}

fn worker_failure_diagnostics(
    name: &str,
    disposition: &str,
    stdout_capture: &mut File,
    stderr_capture: &mut File,
) -> String {
    let mut diagnostics = format!("isolated PID 1 worker {name} {disposition}");
    append_worker_stream_diagnostic(&mut diagnostics, "stdout", stdout_capture);
    append_worker_stream_diagnostic(&mut diagnostics, "stderr", stderr_capture);
    diagnostics
}

fn append_worker_stream_diagnostic(diagnostics: &mut String, stream: &str, capture: &mut File) {
    let length = match capture.metadata() {
        Ok(metadata) => {
            let length = metadata.len();
            let _unused = write!(diagnostics, "\n--- {stream} ({length} bytes) ---\n");
            Some(length)
        }
        Err(error) => {
            let _unused = write!(
                diagnostics,
                "\n--- {stream} (length unavailable: metadata failed: {error}) ---\n"
            );
            None
        }
    };

    if let Err(error) = capture.seek(SeekFrom::Start(0)) {
        let _unused = writeln!(diagnostics, "[diagnostic seek failed: {error}]");
        return;
    }

    let mut bytes = Vec::new();
    let read_result = (&mut *capture)
        .take(MAX_WORKER_DIAGNOSTIC_BYTES)
        .read_to_end(&mut bytes);
    diagnostics.push_str(&String::from_utf8_lossy(&bytes));
    if let Err(error) = read_result {
        let _unused = write!(diagnostics, "\n[diagnostic read failed: {error}]\n");
    }
    if length.is_some_and(|length| length > MAX_WORKER_DIAGNOSTIC_BYTES) {
        let _unused = write!(
            diagnostics,
            "\n[truncated; replayed the first {MAX_WORKER_DIAGNOSTIC_BYTES} bytes]\n"
        );
    }
}

fn stop_known_child(child: &mut Child) {
    if child
        .try_wait()
        .expect("poll known worker handle")
        .is_none()
    {
        child.kill().expect("kill known worker handle");
        child.wait().expect("reap known worker handle");
    }
}

fn worker_selected(name: &str) -> bool {
    std::env::var(WORKER_ENV).ok().as_deref() == Some(name)
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn sigwait_preexec_diagnostic_active() -> io::Result<bool> {
    let python = std::env::var_os(DIAGNOSTIC_PYTHON_ENV);
    let script = std::env::var_os(DIAGNOSTIC_SCRIPT_ENV);
    let preexec_pid = std::env::var_os(DIAGNOSTIC_PREEXEC_PID_ENV);
    match (python, script, preexec_pid) {
        (None, None, None) => Ok(false),
        (Some(python), Some(script), Some(preexec_pid)) => {
            let worker = std::env::var(WORKER_ENV).map_err(|error| {
                io::Error::other(format!("read pre-exec diagnostic worker role: {error}"))
            })?;
            if worker != SIGWAIT_DIAGNOSTIC_WORKER && worker != CAUGHT_SIGCHLD_DIAGNOSTIC_WORKER {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "pre-exec diagnostic is active for an unexpected PID 1 worker",
                ));
            }
            if !PathBuf::from(python).is_absolute() || !PathBuf::from(script).is_absolute() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "pre-exec diagnostic paths must remain absolute in the worker",
                ));
            }
            let preexec_pid = preexec_pid
                .into_string()
                .map_err(|_| io::Error::other("pre-exec PID marker is not UTF-8"))?
                .parse::<u32>()
                .map_err(|error| {
                    io::Error::other(format!("invalid pre-exec PID marker: {error}"))
                })?;
            let worker_pid = std::process::id();
            let matched = preexec_pid == worker_pid;
            writeln!(
                io::stderr().lock(),
                "PID1_DIAGNOSTIC_EXEC_ID mode=preexec-python marker_pid={preexec_pid} rust_pid={worker_pid} matched={matched} worker={worker}"
            )?;
            if matched {
                Ok(true)
            } else {
                Err(io::Error::other(format!(
                    "pre-exec Python PID {preexec_pid} differs from Rust worker PID {worker_pid}"
                )))
            }
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "pre-exec diagnostic requires both paths and the PID marker",
        )),
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn sigwait_fixture_diagnostic_active() -> io::Result<bool> {
    let preexec_python_active = sigwait_preexec_diagnostic_active()?;
    Ok(preexec_python_active || worker_selected(CAUGHT_SIGCHLD_DIAGNOSTIC_WORKER))
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn wait_until_exited(pid: u32) {
    wait_for_child_exit(pid, TestChildSelector::Specific)
        .expect("known child exits before the selector deadline");
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
struct SigchldMaskGuard {
    previous: SigSet,
    armed: bool,
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl SigchldMaskGuard {
    fn block_for_spawn() -> io::Result<Self> {
        let mut mask = SigSet::empty();
        mask.add(Signal::SIGCHLD);
        let previous = mask
            .thread_swap_mask(SigmaskHow::SIG_BLOCK)
            .map_err(|error| io::Error::other(format!("block SIGCHLD for spawn: {error}")))?;
        Ok(Self {
            previous,
            armed: true,
        })
    }

    fn restore(&mut self) -> io::Result<()> {
        if self.armed {
            self.previous
                .thread_set_mask()
                .map_err(|error| io::Error::other(format!("restore SIGCHLD mask: {error}")))?;
            self.armed = false;
        }
        Ok(())
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl Drop for SigchldMaskGuard {
    fn drop(&mut self) {
        let restoration = if self.armed { self.restore() } else { Ok(()) };
        if let Err(error) = restoration {
            let _reported = writeln!(
                io::stderr().lock(),
                "failed to restore the fixture thread's SIGCHLD mask: {error}"
            );
        }
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn ensure_sigchld_blocked() -> io::Result<()> {
    let mask = SigSet::thread_get_mask()
        .map_err(|error| io::Error::other(format!("read inherited signal mask: {error}")))?;
    if mask.contains(Signal::SIGCHLD) {
        Ok(())
    } else {
        Err(io::Error::other(
            "fixture worker did not inherit blocked SIGCHLD",
        ))
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
struct FixtureTimer {
    runtime: tokio::runtime::Runtime,
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl FixtureTimer {
    fn new() -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(|error| io::Error::other(format!("create fixture timer: {error}")))?;
        Ok(Self { runtime })
    }

    fn deadline_after(timeout: Duration) -> io::Result<Instant> {
        Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "fixture deadline overflow"))
    }

    fn wait_for_file(&self, path: &Path, deadline: Instant) -> io::Result<String> {
        let path = path.to_path_buf();
        self.runtime.block_on(async move {
            loop {
                match read_fixture_record(&path) {
                    Ok(contents) => return Ok(contents),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!(
                            "fixture file {} did not appear before deadline",
                            path.display()
                        ),
                    ));
                }
                tokio::time::sleep(FIXTURE_POLL_INTERVAL.min(remaining)).await;
            }
        })
    }

    fn wait_for_reaper_startup(
        &self,
        deadline: Instant,
    ) -> io::Result<ReaperTestDiagnosticSnapshot> {
        self.runtime.block_on(async {
            loop {
                let snapshot = reaper_test_diagnostic_snapshot();
                if snapshot.thread_entered
                    && (snapshot.thread_mask_error.is_some()
                        || snapshot.thread_sigchld_blocked != Some(true)
                        || snapshot.sigwait_errors > 0)
                {
                    emit_reaper_diagnostic("readiness-error", snapshot, 0, false);
                    return Err(io::Error::other(format!(
                        "production reaper startup diagnostic failed: {snapshot:?}"
                    )));
                }
                if snapshot.thread_entered && snapshot.sigwait_entries > 0 {
                    return Ok(snapshot);
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    emit_reaper_diagnostic("readiness-timeout", snapshot, 0, false);
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!("production reaper startup diagnostic timed out: {snapshot:?}"),
                    ));
                }
                tokio::time::sleep(FIXTURE_POLL_INTERVAL.min(remaining)).await;
            }
        })
    }

    #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
    fn wait_for_registered_child(
        &self,
        child: &mut Child,
        deadline: Instant,
    ) -> io::Result<Option<ExitStatus>> {
        self.runtime.block_on(async {
            loop {
                let status = try_wait_registered_fixture_child(child)?;
                if let Some(status) = status {
                    return Ok(Some(status));
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Ok(None);
                }
                tokio::time::sleep(FIXTURE_POLL_INTERVAL.min(remaining)).await;
            }
        })
    }

    #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
    fn wait_for_adoption(
        &self,
        intermediate_pid: u32,
        expected_reaper_pid: u32,
        deadline: Instant,
    ) -> io::Result<u32> {
        self.runtime.block_on(async {
            loop {
                let actual_parent = pid_to_u32(nix::unistd::getppid())?;
                if actual_parent != intermediate_pid && actual_parent != expected_reaper_pid {
                    return Err(io::Error::other(format!(
                        "grandchild PPID changed to unexpected process {actual_parent}"
                    )));
                }
                if actual_parent != intermediate_pid {
                    return Ok(actual_parent);
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "grandchild did not observe the subreaper adoption before deadline",
                    ));
                }
                tokio::time::sleep(FIXTURE_POLL_INTERVAL.min(remaining)).await;
            }
        })
    }
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn try_wait_registered_fixture_child(child: &mut Child) -> io::Result<Option<ExitStatus>> {
    let child_pid = child.id();
    jackin_process::coordinate_children(|registry| {
        if !registry.contains(child_pid) {
            return Err(io::Error::other(
                "registered fixture child lost its owner reservation",
            ));
        }
        child.try_wait()
    })
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn fixture_directory() -> io::Result<PathBuf> {
    std::env::var_os(FIXTURE_DIRECTORY_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("isolated worker fixture directory is missing"))
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn write_fixture_record(path: &Path, contents: &str) -> io::Result<()> {
    if contents.len() as u64 > MAX_FIXTURE_RECORD_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "fixture rendezvous record exceeds its byte limit",
        ));
    }
    let temporary = path.with_extension("pending");
    let fd = nix::fcntl::open(
        &temporary,
        nix::fcntl::OFlag::O_WRONLY
            | nix::fcntl::OFlag::O_CREAT
            | nix::fcntl::OFlag::O_EXCL
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::from_bits_truncate(0o600),
    )
    .map_err(|error| io::Error::from_raw_os_error(error as i32))?;
    let mut file = File::from(fd);
    file.write_all(contents.as_bytes())?;
    drop(file);
    std::fs::rename(&temporary, path)
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
struct FixtureRelease {
    path: PathBuf,
    released: bool,
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl FixtureRelease {
    fn new(directory: &Path, name: &str) -> Self {
        Self {
            path: directory.join(name),
            released: false,
        }
    }

    fn release(&mut self) -> io::Result<()> {
        if !self.released {
            write_fixture_record(&self.path, "release\n")?;
            self.released = true;
        }
        Ok(())
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl Drop for FixtureRelease {
    fn drop(&mut self) {
        let release = if self.released {
            Ok(())
        } else {
            self.release()
        };
        if let Err(error) = release {
            let _reported = writeln!(
                io::stderr().lock(),
                "failed to release fixture gate {}: {error}",
                self.path.display()
            );
        }
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn read_fixture_record(path: &Path) -> io::Result<String> {
    let fd = nix::fcntl::open(
        path,
        nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .map_err(|error| io::Error::from_raw_os_error(error as i32))?;
    let file = File::from(fd);
    if file.metadata()?.len() > MAX_FIXTURE_RECORD_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "fixture rendezvous record exceeds its byte limit",
        ));
    }
    let mut contents = String::new();
    file.take(MAX_FIXTURE_RECORD_BYTES + 1)
        .read_to_string(&mut contents)?;
    if contents.len() as u64 > MAX_FIXTURE_RECORD_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "fixture rendezvous record exceeds its byte limit",
        ));
    }
    Ok(contents)
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn fixture_pids(contents: &str) -> io::Result<Vec<u32>> {
    contents
        .lines()
        .map(|line| {
            line.parse::<u32>()
                .map_err(|error| io::Error::other(format!("invalid fixture PID: {error}")))
        })
        .collect()
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn pid_to_u32(pid: Pid) -> io::Result<u32> {
    u32::try_from(pid.as_raw())
        .map_err(|error| io::Error::other(format!("native PID is outside u32 range: {error}")))
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn spawn_fixture_role(
    name: &str,
    directory: &Path,
    extra_environment: &[(&str, String)],
) -> io::Result<Child> {
    let executable = std::env::current_exe()?;
    let mut command = Command::new(executable);
    command
        .args(["--exact", name, "--nocapture"])
        .env(WORKER_ENV, name)
        .env(FIXTURE_DIRECTORY_ENV, directory)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    for (key, value) in extra_environment {
        command.env(*key, value);
    }
    let mut mask = SigchldMaskGuard::block_for_spawn()?;
    let child = command.spawn()?;
    if let Err(error) = mask.restore() {
        drop(child);
        return Err(error);
    }
    Ok(child)
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
struct OwnedFixtureChild {
    child: Child,
    registration: Option<jackin_process::ChildRegistration>,
    cleanup_enabled: bool,
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl OwnedFixtureChild {
    fn unregistered(child: Child) -> Self {
        Self {
            child,
            registration: None,
            cleanup_enabled: true,
        }
    }

    #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
    fn registered(child: Child, registration: jackin_process::ChildRegistration) -> Self {
        Self {
            child,
            registration: Some(registration),
            cleanup_enabled: true,
        }
    }

    fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
    fn id(&self) -> u32 {
        self.child.id()
    }

    #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
    fn release_registration(&mut self) {
        self.registration.take();
    }

    #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
    fn detach_without_wait(mut self) {
        self.cleanup_enabled = false;
    }

    fn cleanup_bounded(&mut self) -> io::Result<()> {
        if !self.cleanup_enabled {
            return Ok(());
        }
        self.cleanup_enabled = false;
        let result = if self.registration.is_some() {
            #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
            {
                self.cleanup_registered_bounded()
            }
            #[cfg(not(all(target_os = "linux", not(target_env = "uclibc"))))]
            {
                Err(io::Error::other(
                    "registered fixture child cleanup is only supported on Linux",
                ))
            }
        } else {
            self.cleanup_unregistered_bounded()
        };
        self.registration.take();
        result
    }

    fn cleanup_unregistered_bounded(&mut self) -> io::Result<()> {
        let should_kill = matches!(self.child.try_wait(), Ok(None));
        let kill_error = if should_kill {
            self.child.kill().err()
        } else {
            None
        };
        let result = crate::util::poll_owned_child_until_timeout(
            &mut self.child,
            Duration::from_secs(2),
            FIXTURE_POLL_INTERVAL,
        );
        match result {
            Err(error) => Err(io::Error::other(format!(
                "poll fixture child during bounded cleanup failed: {error}; kill error: {kill_error:?}"
            ))),
            Ok(crate::util::ChildPollOutcome::Exited(_)) => Ok(()),
            Ok(crate::util::ChildPollOutcome::TimedOut) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "owned fixture child did not exit before cleanup deadline; kill error: {kill_error:?}"
                ),
            )),
        }
    }

    #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
    fn cleanup_registered_bounded(&mut self) -> io::Result<()> {
        let child_pid = self.child.id();
        let status_or_killed = jackin_process::coordinate_children(|registry| {
            if !registry.contains(child_pid) {
                return Err(io::Error::other(
                    "registered fixture child lost its owner reservation before cleanup",
                ));
            }
            match self.child.try_wait()? {
                Some(status) => Ok(Some(status)),
                None => self.child.kill().map(|()| None),
            }
        })?;
        if status_or_killed.is_some() {
            return Ok(());
        }

        let deadline = FixtureTimer::deadline_after(Duration::from_secs(2))?;
        let timer = FixtureTimer::new()?;
        match timer.wait_for_registered_child(&mut self.child, deadline)? {
            Some(_) => Ok(()),
            None => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "registered fixture child did not exit before bounded cleanup deadline",
            )),
        }
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl Drop for OwnedFixtureChild {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup_bounded() {
            let _reported = writeln!(
                io::stderr().lock(),
                "bounded fixture-child cleanup failed: {error}"
            );
        }
    }
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn spawn_registered_fixture_role(
    name: &str,
    directory: &Path,
    extra_environment: &[(&str, String)],
) -> io::Result<OwnedFixtureChild> {
    let executable = std::env::current_exe()?;
    let mut command = Command::new(executable);
    command
        .args(["--exact", name, "--nocapture"])
        .env(WORKER_ENV, name)
        .env(FIXTURE_DIRECTORY_ENV, directory)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    for (key, value) in extra_environment {
        command.env(*key, value);
    }

    let mut mask = SigchldMaskGuard::block_for_spawn()?;
    let spawn_result = jackin_process::coordinate_children(|registry| {
        let slot = registry
            .prepare_registration()
            .map_err(|error| io::Error::other(format!("reserve intermediate child: {error}")))?;
        let child = command.spawn()?;
        let registration = registry.register_prepared(child.id(), slot);
        Ok(OwnedFixtureChild::registered(child, registration))
    });
    match spawn_result {
        Ok(mut child) => {
            if let Err(error) = mask.restore() {
                let cleanup = child.cleanup_bounded();
                return Err(io::Error::other(format!(
                    "restore SIGCHLD mask after registered spawn: {error}; cleanup: {cleanup:?}"
                )));
            }
            Ok(child)
        }
        Err(error) => {
            mask.restore()?;
            Err(error)
        }
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn emit_reaper_diagnostic(
    stage: &str,
    snapshot: ReaperTestDiagnosticSnapshot,
    causal_event_count: usize,
    overflowed: bool,
) {
    let _reported = writeln!(
        io::stderr().lock(),
        "PID1_DIAGNOSTIC stage={stage} thread_entered={} thread_sigchld_blocked={:?} thread_mask_error={:?} sigwait_entries={} sigwait_successes={} sigwait_errors={} last_sigwait_error={:?} causal_event_count={causal_event_count} overflow={overflowed}",
        snapshot.thread_entered,
        snapshot.thread_sigchld_blocked,
        snapshot.thread_mask_error,
        snapshot.sigwait_entries,
        snapshot.sigwait_successes,
        snapshot.sigwait_errors,
        snapshot.last_sigwait_error,
    );
}

#[cfg(target_os = "macos")]
fn emit_sigchld_action_diagnostic(
    phase: &str,
    child_pid: Option<u32>,
    reaper_snapshot: ReaperTestDiagnosticSnapshot,
    causal_event_count: usize,
    overflowed: bool,
) -> io::Result<()> {
    match jackin_process_directory::current_sigchld_action() {
        Ok(action) => writeln!(
            io::stderr().lock(),
            "PID1_SIGCHLD_ACTION phase={phase} rust_pid={} worker_sigchld_mask=blocked child_pid={child_pid:?} disposition={:?} flags_raw={} flags_hex={:#010x} reaper_snapshot={reaper_snapshot:?} causal_event_count={causal_event_count} overflow={overflowed}",
            std::process::id(),
            action.disposition,
            action.flags,
            action.flags,
        ),
        Err(error) => {
            let _reported = writeln!(
                io::stderr().lock(),
                "PID1_SIGCHLD_ACTION phase={phase} rust_pid={} worker_sigchld_mask=blocked child_pid={child_pid:?} query_error_kind={:?} query_error_raw_os_error={:?} query_error={error} reaper_snapshot={reaper_snapshot:?} causal_event_count={causal_event_count} overflow={overflowed}",
                std::process::id(),
                error.kind(),
                error.raw_os_error(),
            );
            Err(error)
        }
    }
}

#[cfg(target_os = "macos")]
fn diagnose_sigchld_wait_failure(child_pid: u32, events: &ReaperEventLog) {
    let reaper_snapshot = reaper_test_diagnostic_snapshot();
    if let Err(error) = emit_sigchld_action_diagnostic(
        "event-wait-failure",
        Some(child_pid),
        reaper_snapshot,
        events.events.len(),
        events.capture.overflowed(),
    ) {
        let _reported = writeln!(
            io::stderr().lock(),
            "PID1_SIGCHLD_ACTION_OBSERVATION phase=event-wait-failure query_error_before_child_peek error_kind={:?} raw_os_error={:?} error={error}",
            error.kind(),
            error.raw_os_error(),
        );
    }

    let (
        observation,
        before,
        after,
        event_count_before,
        event_count_after,
        overflow_before,
        overflow_after,
    ) = jackin_process::coordinate_children(|_registry| {
        let before = reaper_test_diagnostic_snapshot();
        let event_count_before = events.events.len();
        let overflow_before = events.capture.overflowed();
        let observation = jackin_process_directory::peek_specific_exited_child(child_pid);
        let after = reaper_test_diagnostic_snapshot();
        let event_count_after = events.events.len();
        let overflow_after = events.capture.overflowed();
        (
            observation,
            before,
            after,
            event_count_before,
            event_count_after,
            overflow_before,
            overflow_after,
        )
    });
    let result = match &observation {
        Ok(true) => "waitable=true".to_owned(),
        Ok(false) => "waitable=false".to_owned(),
        Err(error) => format!(
            "waitable=error error_kind={:?} raw_os_error={:?} error={error}",
            error.kind(),
            error.raw_os_error(),
        ),
    };
    let _reported = writeln!(
        io::stderr().lock(),
        "PID1_FAILURE_BOUNDARY child_pid={child_pid} peek_result={result} counters_before={before:?} counters_after={after:?} event_count_before={event_count_before} event_count_after={event_count_after} overflow_before={overflow_before} overflow_after={overflow_after}"
    );
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
struct ReaperDiagnosticGuard {
    causal_event_count: usize,
    overflowed: bool,
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl ReaperDiagnosticGuard {
    fn new(snapshot: ReaperTestDiagnosticSnapshot) -> Self {
        let mut diagnostic = Self {
            causal_event_count: 0,
            overflowed: false,
        };
        diagnostic.report("startup", snapshot, 0, false);
        diagnostic
    }

    fn report(
        &mut self,
        stage: &str,
        snapshot: ReaperTestDiagnosticSnapshot,
        causal_event_count: usize,
        overflowed: bool,
    ) {
        self.causal_event_count = causal_event_count;
        self.overflowed = overflowed;
        emit_reaper_diagnostic(stage, snapshot, causal_event_count, overflowed);
    }

    fn update_events(&mut self, causal_event_count: usize, overflowed: bool) {
        self.causal_event_count = causal_event_count;
        self.overflowed = overflowed;
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl Drop for ReaperDiagnosticGuard {
    fn drop(&mut self) {
        emit_reaper_diagnostic(
            "drop",
            reaper_test_diagnostic_snapshot(),
            self.causal_event_count,
            self.overflowed,
        );
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
struct ReaperEventLog {
    capture: ReaperTestCapture,
    events: Vec<ReaperTestEvent>,
    reported: bool,
    diagnostic: Option<ReaperDiagnosticGuard>,
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl ReaperEventLog {
    fn new(capture: ReaperTestCapture) -> Self {
        Self {
            capture,
            events: Vec::with_capacity(REAPER_TEST_EVENT_CAPACITY),
            reported: false,
            diagnostic: None,
        }
    }

    fn with_diagnostic(capture: ReaperTestCapture, diagnostic: ReaperDiagnosticGuard) -> Self {
        Self {
            capture,
            events: Vec::with_capacity(REAPER_TEST_EVENT_CAPACITY),
            reported: false,
            diagnostic: Some(diagnostic),
        }
    }

    fn report_diagnostic(&mut self, stage: &str) {
        if let Some(diagnostic) = &mut self.diagnostic {
            diagnostic.report(
                stage,
                reaper_test_diagnostic_snapshot(),
                self.events.len(),
                self.capture.overflowed(),
            );
        }
    }

    fn update_diagnostic_events(&mut self, overflowed: bool) {
        if let Some(diagnostic) = &mut self.diagnostic {
            diagnostic.update_events(self.events.len(), overflowed);
        }
    }

    fn wait_until(
        &mut self,
        deadline: Instant,
        predicate: impl Fn(&[ReaperTestEvent]) -> bool,
    ) -> io::Result<()> {
        while !predicate(&self.events) {
            if self.events.len() >= REAPER_TEST_EVENT_CAPACITY {
                return Err(io::Error::other(
                    "bounded PID 1 event observation reached its limit",
                ));
            }
            if self.capture.overflowed() {
                return Err(io::Error::other("bounded PID 1 event channel overflowed"));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "production PID 1 event sequence did not complete before deadline",
                ));
            }
            match self.capture.recv_timeout(remaining) {
                Ok(event) => {
                    self.events.push(event);
                    self.update_diagnostic_events(self.capture.overflowed());
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "production PID 1 event sequence did not complete before deadline",
                    ));
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other(
                        "production PID 1 event channel disconnected",
                    ));
                }
            }
        }
        if self.capture.overflowed() {
            return Err(io::Error::other("bounded PID 1 event channel overflowed"));
        }
        Ok(())
    }

    fn events(&self) -> &[ReaperTestEvent] {
        &self.events
    }

    #[cfg(target_os = "macos")]
    fn drain_available(&mut self) -> io::Result<()> {
        while self.events.len() < REAPER_TEST_EVENT_CAPACITY {
            match self.capture.try_recv() {
                Ok(event) => {
                    self.events.push(event);
                    self.update_diagnostic_events(self.capture.overflowed());
                }
                Err(
                    std::sync::mpsc::TryRecvError::Empty
                    | std::sync::mpsc::TryRecvError::Disconnected,
                ) => break,
            }
        }
        let extra = if self.events.len() == REAPER_TEST_EVENT_CAPACITY {
            self.capture.try_recv().ok()
        } else {
            None
        };
        let overflowed = self.capture.overflowed() || extra.is_some();
        self.update_diagnostic_events(overflowed);
        if overflowed {
            Err(io::Error::other(
                "bounded PID 1 event observation overflowed while draining",
            ))
        } else {
            Ok(())
        }
    }

    fn require_quiet(&mut self, quiet_period: Duration) -> io::Result<()> {
        if self.events.len() >= REAPER_TEST_EVENT_CAPACITY {
            return Err(io::Error::other(
                "bounded PID 1 event observation reached its limit",
            ));
        }
        match self.capture.recv_timeout(quiet_period) {
            Ok(event) => {
                self.events.push(event);
                self.update_diagnostic_events(self.capture.overflowed());
                Err(io::Error::other("unexpected extra production PID 1 event"))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if self.capture.overflowed() => Err(
                io::Error::other("bounded PID 1 event channel overflowed during quiet period"),
            ),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(()),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(io::Error::other(
                "production PID 1 event channel disconnected",
            )),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        while self.events.len() < REAPER_TEST_EVENT_CAPACITY {
            match self.capture.try_recv() {
                Ok(event) => self.events.push(event),
                Err(
                    std::sync::mpsc::TryRecvError::Empty
                    | std::sync::mpsc::TryRecvError::Disconnected,
                ) => break,
            }
        }
        let extra = self.capture.try_recv().ok();
        let overflowed = self.capture.overflowed() || extra.is_some();
        self.update_diagnostic_events(overflowed);
        self.reported = true;
        if overflowed {
            eprintln!("PID1_EVENT overflow=true");
        }
        for event in &self.events {
            eprintln!(
                "PID1_EVENT elapsed_us={} kind={:?}",
                event.elapsed.as_micros(),
                event.kind
            );
        }
        if let Some(event) = extra {
            eprintln!(
                "PID1_EVENT beyond_limit elapsed_us={} kind={:?}",
                event.elapsed.as_micros(),
                event.kind
            );
        }
        if overflowed {
            Err(io::Error::other("bounded PID 1 event channel overflowed"))
        } else {
            Ok(())
        }
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
impl Drop for ReaperEventLog {
    fn drop(&mut self) {
        if !self.reported {
            let _reported = self.flush();
        }
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn reaper_completed_reap(events: &[ReaperTestEvent], pid: i32) -> bool {
    let selected_index = events.iter().position(|event| {
        event.kind
            == (ReaperTestEventKind::Selected {
                pid,
                managed: false,
            })
    });
    let wait_index = events.iter().position(|event| {
        matches!(
            event.kind,
            ReaperTestEventKind::TargetedWaitResult {
                pid: waited_pid,
                status: ReaperTestWaitStatus::Exited(0),
            } if waited_pid == pid
        )
    });
    let reaped_index = events
        .iter()
        .position(|event| event.kind == (ReaperTestEventKind::Reaped { pid }));
    let Some(selected_index) = selected_index else {
        return false;
    };
    let Some(wait_index) = wait_index else {
        return false;
    };
    let Some(reaped_index) = reaped_index else {
        return false;
    };
    selected_index < wait_index
        && wait_index < reaped_index
        && events.iter().enumerate().any(|(index, event)| {
            index > reaped_index
                && event.kind
                    == (ReaperTestEventKind::PassDisposition {
                        failure_errno: None,
                        requeue: false,
                    })
        })
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn assert_targeted_echild(pid: u32) -> io::Result<()> {
    let raw_pid = i32::try_from(pid).map_err(|error| {
        io::Error::other(format!("fixture PID is outside waitpid range: {error}"))
    })?;
    match waitpid(Pid::from_raw(raw_pid), Some(WaitPidFlag::WNOHANG)) {
        Err(nix::errno::Errno::ECHILD) => Ok(()),
        Err(error) => Err(io::Error::other(format!(
            "targeted WNOHANG after production reap returned {error}"
        ))),
        Ok(status) => Err(io::Error::other(format!(
            "targeted WNOHANG after production reap returned {status:?}"
        ))),
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn run_sigwait_requeue_fixture(directory: &Path) -> io::Result<()> {
    let diagnostic_mode = sigwait_fixture_diagnostic_active()?;
    let diagnostic_timer = if diagnostic_mode {
        Some(FixtureTimer::new()?)
    } else {
        None
    };
    let diagnostic_deadline = if diagnostic_mode {
        Some(FixtureTimer::deadline_after(FIXTURE_PROOF_DEADLINE)?)
    } else {
        None
    };
    if let Err(error) = ensure_sigchld_blocked() {
        if diagnostic_mode {
            emit_reaper_diagnostic(
                "fixture-mask-error",
                reaper_test_diagnostic_snapshot(),
                0,
                false,
            );
        }
        return Err(error);
    }
    #[cfg(target_os = "macos")]
    if diagnostic_mode {
        emit_sigchld_action_diagnostic(
            "pre-reaper-install",
            None,
            reaper_test_diagnostic_snapshot(),
            0,
            false,
        )?;
    }
    install_sigchld_reaper();
    if !jackin_process::has_reaper_wakeup() {
        if diagnostic_mode {
            emit_reaper_diagnostic("install-error", reaper_test_diagnostic_snapshot(), 0, false);
        }
        return Err(io::Error::other(
            "production PID 1 reaper did not install its wake callback",
        ));
    }
    let startup_snapshot = match (&diagnostic_timer, diagnostic_deadline) {
        (Some(timer), Some(deadline)) => Some(timer.wait_for_reaper_startup(deadline)?),
        (None, None) => None,
        _ => {
            return Err(io::Error::other(
                "incomplete pre-exec diagnostic timer state",
            ));
        }
    };
    let diagnostic_guard = startup_snapshot.map(ReaperDiagnosticGuard::new);
    let capture = arm_reaper_test_capture(SELECTIVE_EINTR_BUDGET + 1);
    let mut events = match diagnostic_guard {
        Some(diagnostic) => ReaperEventLog::with_diagnostic(capture, diagnostic),
        None => ReaperEventLog::new(capture),
    };
    let timer = match diagnostic_timer {
        Some(timer) => timer,
        None => FixtureTimer::new()?,
    };
    let deadline = match diagnostic_deadline {
        Some(deadline) => deadline,
        None => FixtureTimer::deadline_after(FIXTURE_PROOF_DEADLINE)?,
    };
    let mut child_release = FixtureRelease::new(directory, "causal-child.release");
    let child = spawn_fixture_role("pid1::tests::worker_sigwait_causal_child", directory, &[])?;
    let child_pid = child.id();
    let ready = timer.wait_for_file(&directory.join("causal-child.ready"), deadline)?;
    let ready_pids = fixture_pids(&ready)?;
    if ready_pids.len() != 1 || ready_pids.first() != Some(&child_pid) {
        return Err(io::Error::other(format!(
            "causal child readiness PID {ready_pids:?} does not match spawned child {child_pid}"
        )));
    }
    eprintln!(
        "PID1_SIGWAIT_CHILD pid={child_pid} ready_pid={}",
        ready_pids[0]
    );
    events.report_diagnostic("before-child-release");
    #[cfg(target_os = "macos")]
    if diagnostic_mode {
        emit_sigchld_action_diagnostic(
            "before-child-release",
            Some(child_pid),
            reaper_test_diagnostic_snapshot(),
            events.events.len(),
            events.capture.overflowed(),
        )?;
    }
    child_release.release()?;
    let reaper_pid = i32::try_from(child_pid).map_err(|error| {
        io::Error::other(format!("causal child PID does not fit waitpid: {error}"))
    })?;
    let wait_result = events.wait_until(deadline, |observed| {
        reaper_completed_reap(observed, reaper_pid)
    });
    #[cfg(target_os = "macos")]
    if wait_result.is_err() && diagnostic_mode {
        diagnose_sigchld_wait_failure(child_pid, &events);
    }
    wait_result?;
    assert_sigwait_requeue_events(events.events(), reaper_pid);
    assert_targeted_echild(child_pid)?;
    events.require_quiet(Duration::from_millis(25))?;
    events.flush()?;
    assert_sigwait_requeue_events(events.events(), reaper_pid);
    drop(child);
    Ok(())
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn assert_sigwait_requeue_events(events: &[ReaperTestEvent], child_pid: i32) {
    let mut index = 0;
    let first_signal = events.get(index).map(|event| event.kind);
    assert_eq!(
        first_signal,
        Some(ReaperTestEventKind::SigwaitSucceeded {
            signal: Signal::SIGCHLD as i32,
        })
    );
    index += 1;
    for ordinal in 1..=SELECTIVE_EINTR_BUDGET + 1 {
        assert_eq!(
            events.get(index).map(|event| event.kind),
            Some(ReaperTestEventKind::PeekEintrInjected { ordinal })
        );
        index += 1;
    }
    assert_eq!(
        events.get(index).map(|event| event.kind),
        Some(ReaperTestEventKind::PassDisposition {
            failure_errno: Some(nix::errno::Errno::EINTR as i32),
            requeue: true,
        })
    );
    index += 1;
    assert_eq!(
        events.get(index).map(|event| event.kind),
        Some(ReaperTestEventKind::BackoffStarted)
    );
    let backoff_started = events.get(index).map(|event| event.elapsed);
    index += 1;
    assert_eq!(
        events.get(index).map(|event| event.kind),
        Some(ReaperTestEventKind::BackoffCompleted)
    );
    let backoff_completed = events.get(index).map(|event| event.elapsed);
    index += 1;
    assert!(matches!(
        events.get(index).map(|event| event.kind),
        Some(ReaperTestEventKind::WakeAttempt {
            process_pid,
            signal,
            succeeded: true,
            errno: None,
        }) if process_pid == std::process::id() as i32
            && signal == Signal::SIGCHLD as i32
    ));
    index += 1;
    assert_eq!(
        events.get(index).map(|event| event.kind),
        Some(ReaperTestEventKind::SigwaitSucceeded {
            signal: Signal::SIGCHLD as i32,
        })
    );
    index += 1;
    assert_eq!(
        events.get(index).map(|event| event.kind),
        Some(ReaperTestEventKind::Selected {
            pid: child_pid,
            managed: false,
        })
    );
    index += 1;
    assert_eq!(
        events.get(index).map(|event| event.kind),
        Some(ReaperTestEventKind::TargetedWaitResult {
            pid: child_pid,
            status: ReaperTestWaitStatus::Exited(0),
        })
    );
    index += 1;
    assert_eq!(
        events.get(index).map(|event| event.kind),
        Some(ReaperTestEventKind::Reaped { pid: child_pid })
    );
    index += 1;
    assert_eq!(
        events.get(index).map(|event| event.kind),
        Some(ReaperTestEventKind::PassDisposition {
            failure_errno: None,
            requeue: false,
        })
    );
    index += 1;
    assert_eq!(
        index,
        events.len(),
        "unexpected production reaper event count"
    );
    assert!(
        backoff_completed
            .zip(backoff_started)
            .is_some_and(|(end, start)| end.saturating_sub(start) >= REAPER_RETRY_BACKOFF),
        "production retry backoff was shorter than its configured interval"
    );
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn run_causal_child_fixture() -> io::Result<()> {
    ensure_sigchld_blocked()?;
    let directory = fixture_directory()?;
    let timer = FixtureTimer::new()?;
    let pid = std::process::id();
    write_fixture_record(&directory.join("causal-child.ready"), &format!("{pid}\n"))?;
    let deadline = FixtureTimer::deadline_after(FIXTURE_CHILD_DEADLINE)?;
    let _release = timer.wait_for_file(&directory.join("causal-child.release"), deadline)?;
    Ok(())
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn env_fixture_pid(name: &str) -> io::Result<u32> {
    std::env::var(name)
        .map_err(|error| io::Error::other(format!("read fixture PID {name}: {error}")))?
        .parse::<u32>()
        .map_err(|error| io::Error::other(format!("parse fixture PID {name}: {error}")))
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn run_subreaper_fixture(directory: &Path) -> io::Result<()> {
    ensure_sigchld_blocked()?;
    nix::sys::prctl::set_child_subreaper(true)
        .map_err(|error| io::Error::other(format!("enable test subreaper: {error}")))?;
    if !nix::sys::prctl::get_child_subreaper()
        .map_err(|error| io::Error::other(format!("verify test subreaper: {error}")))?
    {
        return Err(io::Error::other("kernel did not enable test subreaper"));
    }
    eprintln!("PID1_SUBREAPER_ENABLED pid={}", std::process::id());
    install_sigchld_reaper();
    if !jackin_process::has_reaper_wakeup() {
        return Err(io::Error::other(
            "production PID 1 reaper did not install its wake callback",
        ));
    }
    let mut events = ReaperEventLog::new(arm_reaper_test_capture(0));
    let timer = FixtureTimer::new()?;
    let deadline = FixtureTimer::deadline_after(FIXTURE_PROOF_DEADLINE)?;
    let reaper_pid = std::process::id();
    let extra_environment = [(FIXTURE_REAPER_PID_ENV, reaper_pid.to_string())];
    let mut intermediate_release = FixtureRelease::new(directory, "intermediate.release");
    let mut grandchild_release = FixtureRelease::new(directory, "grandchild.release");
    let mut intermediate = spawn_registered_fixture_role(
        "pid1::tests::worker_adoption_intermediate",
        directory,
        &extra_environment,
    )?;
    let intermediate_pid = intermediate.id();
    let ready = timer.wait_for_file(&directory.join("intermediate.ready"), deadline)?;
    let ready_pids = fixture_pids(&ready)?;
    if ready_pids.len() != 3
        || ready_pids.first() != Some(&intermediate_pid)
        || ready_pids.get(2) != Some(&intermediate_pid)
    {
        return Err(io::Error::other(format!(
            "intermediate readiness did not prove its grandchild's real initial PPID: {ready_pids:?}"
        )));
    }
    let grandchild_pid = ready_pids[1];
    let grandchild_ready = timer.wait_for_file(&directory.join("grandchild.ready"), deadline)?;
    let grandchild_ready_pids = fixture_pids(&grandchild_ready)?;
    if grandchild_ready_pids.len() != 2
        || grandchild_ready_pids.first() != Some(&grandchild_pid)
        || grandchild_ready_pids.get(1) != Some(&ready_pids[2])
    {
        return Err(io::Error::other(format!(
            "grandchild's direct readiness record did not match the intermediate handoff: {grandchild_ready_pids:?}"
        )));
    }
    intermediate_release.release()?;
    let status = timer.wait_for_registered_child(intermediate.child_mut(), deadline)?;
    let Some(status) = status else {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "registered intermediate did not exit before owner-wait deadline",
        ));
    };
    if !status.success() {
        return Err(io::Error::other(format!(
            "registered intermediate exited unsuccessfully: {status}"
        )));
    }
    intermediate.release_registration();

    let adopted = timer.wait_for_file(&directory.join("grandchild.adopted"), deadline)?;
    let adopted_pids = fixture_pids(&adopted)?;
    if adopted_pids.len() != 3
        || adopted_pids.first() != Some(&grandchild_pid)
        || adopted_pids.get(1) != Some(&intermediate_pid)
        || adopted_pids.get(2) != Some(&reaper_pid)
    {
        return Err(io::Error::other(format!(
            "grandchild PPID transition did not match the live subreaper: {adopted_pids:?}"
        )));
    }
    eprintln!(
        "PID1_ADOPTION intermediate_pid={intermediate_pid} grandchild_pid={grandchild_pid} initial_ppid={} adopted_ppid={} subreaper_pid={reaper_pid}",
        adopted_pids[1], adopted_pids[2]
    );
    grandchild_release.release()?;
    let grandchild_raw = i32::try_from(grandchild_pid).map_err(|error| {
        io::Error::other(format!("grandchild PID does not fit waitpid: {error}"))
    })?;
    events.wait_until(deadline, |observed| {
        reaper_completed_reap(observed, grandchild_raw)
    })?;
    assert_targeted_echild(grandchild_pid)?;
    events.flush()?;
    Ok(())
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn run_intermediate_fixture() -> io::Result<()> {
    ensure_sigchld_blocked()?;
    let directory = fixture_directory()?;
    let reaper_pid = env_fixture_pid(FIXTURE_REAPER_PID_ENV)?;
    let intermediate_pid = std::process::id();
    let extra_environment = [
        (FIXTURE_INTERMEDIATE_PID_ENV, intermediate_pid.to_string()),
        (FIXTURE_REAPER_PID_ENV, reaper_pid.to_string()),
    ];
    let child = spawn_fixture_role(
        "pid1::tests::worker_adoption_grandchild",
        &directory,
        &extra_environment,
    )?;
    let grandchild = OwnedFixtureChild::unregistered(child);
    let grandchild_pid = grandchild.id();
    let timer = FixtureTimer::new()?;
    let deadline = FixtureTimer::deadline_after(FIXTURE_CHILD_DEADLINE)?;
    let ready = timer.wait_for_file(&directory.join("grandchild.ready"), deadline)?;
    let ready_pids = fixture_pids(&ready)?;
    if ready_pids.len() != 2
        || ready_pids.first() != Some(&grandchild_pid)
        || ready_pids.get(1) != Some(&intermediate_pid)
    {
        return Err(io::Error::other(format!(
            "grandchild did not report its real direct parent PID: {ready_pids:?}"
        )));
    }
    write_fixture_record(
        &directory.join("intermediate.ready"),
        &format!("{intermediate_pid}\n{grandchild_pid}\n{}\n", ready_pids[1]),
    )?;
    let _release = timer.wait_for_file(&directory.join("intermediate.release"), deadline)?;
    if reaper_pid == intermediate_pid {
        return Err(io::Error::other(
            "intermediate and subreaper PIDs unexpectedly match",
        ));
    }
    grandchild.detach_without_wait();
    Ok(())
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn run_grandchild_fixture() -> io::Result<()> {
    ensure_sigchld_blocked()?;
    let directory = fixture_directory()?;
    let expected_intermediate = env_fixture_pid(FIXTURE_INTERMEDIATE_PID_ENV)?;
    let expected_reaper = env_fixture_pid(FIXTURE_REAPER_PID_ENV)?;
    let pid = std::process::id();
    let initial_parent = pid_to_u32(nix::unistd::getppid())?;
    if initial_parent != expected_intermediate {
        return Err(io::Error::other(format!(
            "grandchild initial PPID {initial_parent} differs from intermediate {expected_intermediate}"
        )));
    }
    write_fixture_record(
        &directory.join("grandchild.ready"),
        &format!("{pid}\n{initial_parent}\n"),
    )?;
    let timer = FixtureTimer::new()?;
    let deadline = FixtureTimer::deadline_after(FIXTURE_CHILD_DEADLINE)?;
    let adopted_parent =
        timer.wait_for_adoption(expected_intermediate, expected_reaper, deadline)?;
    write_fixture_record(
        &directory.join("grandchild.adopted"),
        &format!("{pid}\n{initial_parent}\n{adopted_parent}\n"),
    )?;
    let _release = timer.wait_for_file(&directory.join("grandchild.release"), deadline)?;
    Ok(())
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn run_causal_worker(name: &str, directory: &Path) -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let mut stdout_capture = tempfile::tempfile()?;
    let mut stderr_capture = tempfile::tempfile()?;
    let diagnostic = preexec_diagnostic_request(name)?;
    let mut command = if let Some(request) = diagnostic {
        let mut command = Command::new(request.python);
        command
            .args(["-I", "-S", "-B"])
            .arg(request.script)
            .arg(&executable)
            .args(["--exact", name, "--nocapture"]);
        command
    } else {
        let mut command = Command::new(&executable);
        command.args(["--exact", name, "--nocapture"]);
        command
    };
    command
        .env(WORKER_ENV, name)
        .env(FIXTURE_DIRECTORY_ENV, directory)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_capture.try_clone()?))
        .stderr(Stdio::from(stderr_capture.try_clone()?));
    let mut mask = SigchldMaskGuard::block_for_spawn()?;
    let child = command.spawn()?;
    let mut child = OwnedFixtureChild::unregistered(child);
    if let Err(error) = mask.restore() {
        let cleanup = child.cleanup_bounded();
        return Err(io::Error::other(format!(
            "restore SIGCHLD mask after worker spawn: {error}; cleanup: {cleanup:?}"
        )));
    }
    let outcome = crate::util::poll_owned_child_until_timeout(
        child.child_mut(),
        Duration::from_secs(8),
        FIXTURE_POLL_INTERVAL,
    );
    match outcome {
        Ok(crate::util::ChildPollOutcome::Exited(status)) => {
            let diagnostics = worker_failure_diagnostics(
                name,
                &format!("completed with status {status}"),
                &mut stdout_capture,
                &mut stderr_capture,
            );
            eprintln!("{diagnostics}");
            if status.success() {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "isolated PID 1 fixture worker failed\n{diagnostics}"
                )))
            }
        }
        Ok(crate::util::ChildPollOutcome::TimedOut) => {
            let cleanup = child.cleanup_bounded();
            let diagnostics = worker_failure_diagnostics(
                name,
                "timed out after 8 seconds",
                &mut stdout_capture,
                &mut stderr_capture,
            );
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "isolated PID 1 fixture worker exceeded its deadline: {cleanup:?}\n{diagnostics}"
                ),
            ))
        }
        Err(error) => {
            let cleanup = child.cleanup_bounded();
            let diagnostics = worker_failure_diagnostics(
                name,
                &format!("child polling failed: {error}; cleanup: {cleanup:?}"),
                &mut stdout_capture,
                &mut stderr_capture,
            );
            Err(io::Error::other(format!(
                "isolated PID 1 fixture worker could not be supervised\n{diagnostics}"
            )))
        }
    }
}

#[test]
fn reap_zombies_returns_when_no_children() {
    run_worker("pid1::tests::worker_no_children");
}

#[test]
fn worker_no_children() {
    if worker_selected("pid1::tests::worker_no_children") {
        reap_zombies();
    }
}

#[test]
fn waitpid_wnohang_returns_echild_after_owner_wait() {
    run_worker("pid1::tests::worker_waitpid_after_wait");
}

#[test]
fn worker_waitpid_after_wait() {
    if !worker_selected("pid1::tests::worker_waitpid_after_wait") {
        return;
    }
    let mut child = Command::new("/bin/sh")
        .args(["-c", ":"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn known direct child");
    let pid = Pid::from_raw(i32::try_from(child.id()).expect("child PID fits i32"));
    assert!(child.wait().expect("owner wait").success());
    assert_eq!(
        waitpid(pid, Some(WaitPidFlag::WNOHANG)).expect_err("waited status is consumed"),
        nix::errno::Errno::ECHILD
    );
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[test]
fn reap_zombies_does_not_steal_registered_session_child() {
    run_worker("pid1::tests::worker_preserves_registered_child");
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[test]
fn worker_preserves_registered_child() {
    if !worker_selected("pid1::tests::worker_preserves_registered_child") {
        return;
    }
    let (mut child, registration) = jackin_process::coordinate_children(|registry| {
        let slot = registry
            .prepare_registration()
            .expect("preallocate child reservation");
        let child = Command::new("/bin/sh")
            .args(["-c", ":"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn registered child while coordinated");
        let registration = registry.register_prepared(child.id(), slot);
        (child, registration)
    });
    wait_until_exited(child.id());
    reap_zombies();
    wait_until_exited(child.id());
    assert!(child.wait().expect("registered owner wait").success());
    drop(registration);
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[test]
fn live_managed_child_does_not_defer_other_unowned_reap() {
    run_worker("pid1::tests::worker_reaps_unowned_with_live_owner");
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[test]
fn worker_reaps_unowned_with_live_owner() {
    if !worker_selected("pid1::tests::worker_reaps_unowned_with_live_owner") {
        return;
    }
    let (mut managed, registration) = jackin_process::coordinate_children(|registry| {
        let slot = registry
            .prepare_registration()
            .expect("preallocate child reservation");
        let child = Command::new("/bin/sleep")
            .arg("5")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn known live managed child");
        let registration = registry.register_prepared(child.id(), slot);
        (child, registration)
    });
    let mut unowned = Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn unowned direct child");
    let unowned_pid = Pid::from_raw(i32::try_from(unowned.id()).expect("child PID fits i32"));
    wait_until_exited(unowned.id());

    reap_zombies();

    assert_eq!(
        waitpid(unowned_pid, Some(WaitPidFlag::WNOHANG))
            .expect_err("reaper consumed unowned child while managed child remains live"),
        nix::errno::Errno::ECHILD
    );
    assert_eq!(
        unowned
            .wait()
            .expect_err("reaper-owned Child handle reports ECHILD")
            .raw_os_error(),
        Some(nix::errno::Errno::ECHILD as i32)
    );
    managed.kill().expect("kill known managed child handle");
    managed.wait().expect("reap known managed child handle");
    drop(registration);
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
static TEST_WAKE_SENDER: OnceLock<SyncSender<()>> = OnceLock::new();

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn test_reaper_wakeup() {
    if let Some(sender) = TEST_WAKE_SENDER.get() {
        let _sent = sender.send(());
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[test]
fn released_owned_head_wakes_reaper_for_later_unowned_child() {
    run_worker("pid1::tests::worker_release_wakeup");
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[test]
fn worker_release_wakeup() {
    if !worker_selected("pid1::tests::worker_release_wakeup") {
        return;
    }
    let (wake_tx, wake_rx) = sync_channel(1);
    TEST_WAKE_SENDER
        .set(wake_tx)
        .expect("isolated worker installs one local wake sender");
    jackin_process::install_reaper_wakeup(test_reaper_wakeup)
        .expect("install isolated test-local wake callback");

    let (mut managed, registration) = jackin_process::coordinate_children(|registry| {
        let slot = registry
            .prepare_registration()
            .expect("preallocate child reservation");
        let child = Command::new("/bin/sh")
            .args(["-c", ":"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn managed head");
        let registration = registry.register_prepared(child.id(), slot);
        (child, registration)
    });
    wait_until_exited(managed.id());
    reap_zombies();
    wait_until_exited(managed.id());

    let mut later = Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn unowned child while managed zombie remains registered");
    let later_pid = Pid::from_raw(i32::try_from(later.id()).expect("PID fits i32"));
    wait_until_exited(later.id());
    wait_until_exited(managed.id());

    assert!(
        managed
            .wait()
            .expect("managed owner consumes its status")
            .success()
    );
    let managed_id = managed.id();
    assert!(jackin_process::coordinate_children(
        |registry| registry.contains(managed_id)
    ));
    assert_eq!(
        waitpid(
            Pid::from_raw(i32::try_from(managed_id).expect("managed PID fits i32")),
            Some(WaitPidFlag::WNOHANG),
        )
        .expect_err("owner consumed managed status while retaining its token"),
        nix::errno::Errno::ECHILD
    );

    let reaper = std::thread::spawn(move || {
        wake_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("release callback requeues the pending reap pass");
        reap_zombies();
    });
    registration.release_to_reaper();
    reaper
        .join()
        .expect("callback-driven production reaper pass");
    assert_eq!(
        waitpid(
            Pid::from_raw(i32::try_from(managed_id).expect("managed PID fits i32")),
            Some(WaitPidFlag::WNOHANG),
        )
        .expect_err("managed owner status remains consumed after callback"),
        nix::errno::Errno::ECHILD
    );
    assert_eq!(
        waitpid(later_pid, Some(WaitPidFlag::WNOHANG))
            .expect_err("callback-driven pass consumes the already eligible child"),
        nix::errno::Errno::ECHILD
    );
    assert_eq!(
        later
            .wait()
            .expect_err("reaper-owned later Child handle reports ECHILD")
            .raw_os_error(),
        Some(nix::errno::Errno::ECHILD as i32)
    );
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[test]
fn cross_thread_release_wakes_only_after_registry_unlock() {
    run_worker("pid1::tests::worker_cross_thread_release_defers_wakeup");
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[test]
fn worker_cross_thread_release_defers_wakeup() {
    if !worker_selected("pid1::tests::worker_cross_thread_release_defers_wakeup") {
        return;
    }
    let (wake_tx, wake_rx) = sync_channel(1);
    TEST_WAKE_SENDER
        .set(wake_tx)
        .expect("isolated worker installs one local wake sender");
    jackin_process::install_reaper_wakeup(test_reaper_wakeup)
        .expect("install isolated test-local wake callback");
    let registration = jackin_process::coordinate_children(|registry| {
        registry.register(u32::MAX).expect("reserve known test PID")
    });
    let (dropped_tx, dropped_rx) = std::sync::mpsc::channel();

    jackin_process::coordinate_children(|registry| {
        assert!(registry.contains(u32::MAX));
        let dropper = std::thread::spawn(move || {
            registration.release_to_reaper();
            let _sent = dropped_tx.send(());
        });
        dropped_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("other thread releases its reservation without waiting on the lock");
        assert!(matches!(
            wake_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        dropper.join().expect("reservation drop thread");
    });

    wake_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("last reservation wakes only after coordinate unlock");
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
fn run_fixture_worker_in_private_directory(name: &str) -> io::Result<()> {
    let directory = tempfile::tempdir()?;
    run_causal_worker(name, directory.path())
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[test]
fn production_sigwait_requeues_eintr_exhaustion_and_reaps_child() -> io::Result<()> {
    run_fixture_worker_in_private_directory("pid1::tests::worker_production_sigwait_requeue")
}

#[cfg(target_os = "macos")]
#[test]
fn diagnostic_caught_sigchld_preserves_strict_production_requeue_sequence() -> io::Result<()> {
    run_fixture_worker_in_private_directory(CAUGHT_SIGCHLD_DIAGNOSTIC_WORKER)
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[test]
fn worker_production_sigwait_requeue() -> io::Result<()> {
    if worker_selected("pid1::tests::worker_production_sigwait_requeue") {
        let directory = fixture_directory()?;
        run_sigwait_requeue_fixture(&directory)
    } else {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
#[test]
fn worker_caught_sigchld_requeue_diagnostic() -> io::Result<()> {
    if !worker_selected(CAUGHT_SIGCHLD_DIAGNOSTIC_WORKER) {
        return Ok(());
    }

    ensure_sigchld_blocked()?;
    let preexec_python_active = sigwait_preexec_diagnostic_active()?;
    let execution_mode = if preexec_python_active {
        "preexec-python"
    } else {
        "direct-rust"
    };
    writeln!(
        io::stderr().lock(),
        "PID1_DIAGNOSTIC_EXEC_ID mode={execution_mode} marker_verified={preexec_python_active} worker={CAUGHT_SIGCHLD_DIAGNOSTIC_WORKER} rust_pid={}",
        std::process::id(),
    )?;

    let baseline = jackin_process_directory::current_sigchld_action()?;
    let expected_flags = libc::SA_RESTART | libc::SA_SIGINFO;
    let baseline_matches = baseline.disposition
        == jackin_process_directory::SigchldDisposition::Default
        && baseline.flags == expected_flags;
    writeln!(
        io::stderr().lock(),
        "PID1_SIGCHLD_TREATMENT phase=worker-baseline disposition={:?} flags_raw={} flags_hex={:#010x} expected_flags_raw={} supported_default_baseline={baseline_matches}",
        baseline.disposition,
        baseline.flags,
        baseline.flags,
        expected_flags,
    )?;
    if !baseline_matches {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "unsupported caught SIGCHLD treatment baseline: disposition={:?}, flags_raw={}, expected_flags_raw={expected_flags}",
                baseline.disposition, baseline.flags
            ),
        ));
    }

    let mut guard =
        jackin_process_directory::CaughtSigchldDiagnosticGuard::install_for_diagnostic_worker()?;
    let proof = (|| {
        let caught = jackin_process_directory::current_sigchld_action()?;
        let caught_matches = caught.disposition
            == jackin_process_directory::SigchldDisposition::OpaqueCaught
            && caught.flags == expected_flags;
        let _reported = writeln!(
            io::stderr().lock(),
            "PID1_SIGCHLD_TREATMENT phase=worker-caught disposition={:?} flags_raw={} flags_hex={:#010x} expected_flags_raw={} coarse_action_match={caught_matches}; complete_match_was_verified_by_guard=true",
            caught.disposition,
            caught.flags,
            caught.flags,
            expected_flags,
        );
        if !caught_matches {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "caught SIGCHLD action did not match its opaque disposition and preserved flags",
            ));
        }
        let directory = fixture_directory()?;
        run_sigwait_requeue_fixture(&directory)
    })();

    let restoration = guard.restore();
    match &restoration {
        Ok(()) => {
            let _reported = writeln!(
                io::stderr().lock(),
                "PID1_SIGCHLD_TREATMENT phase=worker-explicit-restore restored_exact=true"
            );
        }
        Err(error) => {
            let _reported = writeln!(
                io::stderr().lock(),
                "PID1_SIGCHLD_TREATMENT phase=worker-explicit-restore restored_exact=false error_kind={:?} raw_os_error={:?} error={error}",
                error.kind(),
                error.raw_os_error(),
            );
        }
    }

    match proof {
        Ok(()) => restoration,
        Err(primary) => {
            if let Err(error) = &restoration {
                let _reported = writeln!(
                    io::stderr().lock(),
                    "PID1_SIGCHLD_TREATMENT phase=proof-and-restore-failure primary_kind={:?} primary_raw_os_error={:?} restoration_kind={:?} restoration_raw_os_error={:?} restoration_error={error}",
                    primary.kind(),
                    primary.raw_os_error(),
                    error.kind(),
                    error.raw_os_error(),
                );
            }
            Err(primary)
        }
    }
}

#[cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]
#[test]
fn worker_sigwait_causal_child() -> io::Result<()> {
    if worker_selected("pid1::tests::worker_sigwait_causal_child") {
        run_causal_child_fixture()
    } else {
        Ok(())
    }
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[test]
fn production_reaper_reaps_adopted_orphan_after_getppid_change() -> io::Result<()> {
    run_fixture_worker_in_private_directory("pid1::tests::worker_production_subreaper")
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[test]
fn worker_production_subreaper() -> io::Result<()> {
    if worker_selected("pid1::tests::worker_production_subreaper") {
        let directory = fixture_directory()?;
        run_subreaper_fixture(&directory)
    } else {
        Ok(())
    }
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[test]
fn worker_adoption_intermediate() -> io::Result<()> {
    if worker_selected("pid1::tests::worker_adoption_intermediate") {
        run_intermediate_fixture()
    } else {
        Ok(())
    }
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[test]
fn worker_adoption_grandchild() -> io::Result<()> {
    if worker_selected("pid1::tests::worker_adoption_grandchild") {
        run_grandchild_fixture()
    } else {
        Ok(())
    }
}

mod selective_pass_faults;

#[cfg(target_os = "macos")]
const DARWIN_FIXTURE_ROLE_ENV: &str = "JACKIN_PID1_TEST_DARWIN_FIXTURE_ROLE";
#[cfg(target_os = "macos")]
const DARWIN_GATED_CHILD_WORKER: &str = "pid1::tests::worker_darwin_tokio_gated_child";
#[cfg(target_os = "macos")]
const DARWIN_COEXIST_WORKER: &str =
    "pid1::tests::worker_darwin_tokio_reaper_coexists_with_registered_child_wait";
#[cfg(target_os = "macos")]
const DARWIN_EINTR_WORKER: &str =
    "pid1::tests::worker_darwin_tokio_reaper_requeues_real_eintr_exhaustion";
#[cfg(target_os = "macos")]
const DARWIN_HANDOFF_WORKER: &str =
    "pid1::tests::worker_darwin_tokio_registered_child_timeout_and_cancel_handoff";
#[cfg(target_os = "macos")]
const DARWIN_MASK_WORKER: &str =
    "pid1::tests::worker_darwin_tokio_reaper_rejects_blocked_startup_mask";
#[cfg(target_os = "macos")]
const DARWIN_FATAL_WORKER: &str =
    "pid1::tests::worker_darwin_tokio_reaper_fails_closed_after_registered_worker_panic";

#[cfg(target_os = "macos")]
fn run_darwin_ordinary_worker(name: &str, expected_exit: i32) -> io::Result<String> {
    let executable = std::env::current_exe()?;
    let directory = tempfile::tempdir()?;
    let mut stdout_capture = tempfile::tempfile()?;
    let mut stderr_capture = tempfile::tempfile()?;
    let mut child = Command::new(executable)
        .args(["--exact", name, "--nocapture"])
        .env(WORKER_ENV, name)
        .env(FIXTURE_DIRECTORY_ENV, directory.path())
        .env_remove(DIAGNOSTIC_PYTHON_ENV)
        .env_remove(DIAGNOSTIC_SCRIPT_ENV)
        .env_remove(DIAGNOSTIC_PREEXEC_PID_ENV)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_capture.try_clone()?))
        .stderr(Stdio::from(stderr_capture.try_clone()?))
        .spawn()?;
    let outcome = crate::util::poll_owned_child_until_timeout(
        &mut child,
        Duration::from_secs(8),
        FIXTURE_POLL_INTERVAL,
    );
    match outcome {
        Ok(crate::util::ChildPollOutcome::Exited(status)) => {
            let diagnostics = worker_failure_diagnostics(
                name,
                &format!("completed with status {status}"),
                &mut stdout_capture,
                &mut stderr_capture,
            );
            eprintln!("{diagnostics}");
            if status.code() != Some(expected_exit) {
                return Err(io::Error::other(format!(
                    "isolated Darwin SIGCHLD worker {name} exited with {:?}, expected {expected_exit}\n{diagnostics}",
                    status.code()
                )));
            }
            Ok(diagnostics)
        }
        Ok(crate::util::ChildPollOutcome::TimedOut) => {
            stop_known_child(&mut child);
            let diagnostics = worker_failure_diagnostics(
                name,
                "timed out after 8 seconds",
                &mut stdout_capture,
                &mut stderr_capture,
            );
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("isolated Darwin SIGCHLD worker timed out\n{diagnostics}"),
            ))
        }
        Err(error) => {
            stop_known_child(&mut child);
            let diagnostics = worker_failure_diagnostics(
                name,
                &format!("child polling failed: {error}"),
                &mut stdout_capture,
                &mut stderr_capture,
            );
            Err(io::Error::other(format!(
                "isolated Darwin SIGCHLD worker could not be supervised\n{diagnostics}"
            )))
        }
    }
}

#[cfg(target_os = "macos")]
fn check_darwin_worker_baseline() -> io::Result<()> {
    for key in [
        DIAGNOSTIC_PYTHON_ENV,
        DIAGNOSTIC_SCRIPT_ENV,
        DIAGNOSTIC_PREEXEC_PID_ENV,
    ] {
        if std::env::var_os(key).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("ordinary Darwin worker inherited diagnostic variable {key}"),
            ));
        }
    }
    let action = jackin_process_directory::current_sigchld_action()?;
    if action.disposition != jackin_process_directory::SigchldDisposition::Default {
        return Err(io::Error::other(format!(
            "ordinary Darwin worker did not start with default SIGCHLD action: {:?}",
            action.disposition
        )));
    }
    let eligibility = jackin_process_directory::tokio_sigchld_eligibility()?;
    if eligibility != jackin_process_directory::TokioSigchldEligibility::Eligible {
        return Err(io::Error::other(format!(
            "ordinary Darwin worker has ineligible SIGCHLD action: {eligibility:?}"
        )));
    }
    let mask = SigSet::thread_get_mask()
        .map_err(|error| io::Error::other(format!("query ordinary worker mask: {error}")))?;
    if mask.contains(Signal::SIGCHLD) {
        return Err(io::Error::other(
            "ordinary Darwin worker started with SIGCHLD blocked",
        ));
    }
    writeln!(
        io::stderr().lock(),
        "PID1_DARWIN_TOKIO_EXEC backend=tokio-unix-signal worker_pid={} disposition=Default sigchld_blocked=false diagnostic_preexec=false",
        std::process::id()
    )?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn start_darwin_reaper_for_test() -> io::Result<()> {
    check_darwin_worker_baseline()?;
    prepare_darwin_reaper()
        .map_err(|error| io::Error::other(format!("prepare Darwin reaper: {error:#}")))?;
    activate_darwin_reaper();
    require_darwin_reaper_active()
        .map_err(|error| io::Error::other(format!("require active Darwin reaper: {error:#}")))?;
    writeln!(
        io::stderr().lock(),
        "PID1_DARWIN_TOKIO phase=active worker_pid={}",
        std::process::id()
    )?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn spawn_darwin_gated_child(directory: &Path, role: &str) -> io::Result<Child> {
    let executable = std::env::current_exe()?;
    Command::new(executable)
        .args(["--exact", DARWIN_GATED_CHILD_WORKER, "--nocapture"])
        .env(WORKER_ENV, DARWIN_GATED_CHILD_WORKER)
        .env(FIXTURE_DIRECTORY_ENV, directory)
        .env(DARWIN_FIXTURE_ROLE_ENV, role)
        .env_remove(DIAGNOSTIC_PYTHON_ENV)
        .env_remove(DIAGNOSTIC_SCRIPT_ENV)
        .env_remove(DIAGNOSTIC_PREEXEC_PID_ENV)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

#[cfg(target_os = "macos")]
fn run_darwin_gated_child() -> io::Result<()> {
    let directory = fixture_directory()?;
    let role = std::env::var(DARWIN_FIXTURE_ROLE_ENV)
        .map_err(|error| io::Error::other(format!("read Darwin fixture role: {error}")))?;
    if !matches!(
        role.as_str(),
        "managed" | "unowned" | "success" | "timeout" | "cancel"
    ) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unknown Darwin fixture role",
        ));
    }
    let pid = std::process::id();
    write_fixture_record(
        &directory.join(format!("darwin-{role}.ready")),
        &format!("{pid}\n"),
    )?;
    let timer = FixtureTimer::new()?;
    let deadline = FixtureTimer::deadline_after(FIXTURE_CHILD_DEADLINE)?;
    let release_path = directory.join(format!("darwin-{role}.release"));
    let disposition = match timer.wait_for_file(&release_path, deadline) {
        Ok(_) => "released\n",
        Err(error) if error.kind() == io::ErrorKind::TimedOut => "expired\n",
        Err(error) => return Err(error),
    };
    write_fixture_record(
        &directory.join(format!("darwin-{role}.disposition")),
        disposition,
    )
}

#[cfg(target_os = "macos")]
async fn read_darwin_fixture_record(path: PathBuf, deadline: Instant) -> io::Result<String> {
    loop {
        match read_fixture_record(&path) {
            Ok(contents) => return Ok(contents),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "Darwin fixture record {} missed its deadline",
                    path.display()
                ),
            ));
        }
        tokio::time::sleep(FIXTURE_POLL_INTERVAL.min(remaining)).await;
    }
}

#[cfg(target_os = "macos")]
async fn assert_darwin_fixture_disposition(
    directory: &Path,
    role: &str,
    expected: &str,
    deadline: Instant,
) -> io::Result<()> {
    let observed = read_darwin_fixture_record(
        directory.join(format!("darwin-{role}.disposition")),
        deadline,
    )
    .await?;
    if observed == expected {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Darwin fixture {role} disposition was {observed:?}, expected {expected:?}"
        )))
    }
}

#[cfg(target_os = "macos")]
fn assert_darwin_cancel_fixture_has_no_disposition(directory: &Path) -> io::Result<()> {
    let path = directory.join("darwin-cancel.disposition");
    match read_fixture_record(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(contents) => Err(io::Error::other(format!(
            "cancelled Darwin fixture wrote an unexpected disposition {contents:?}"
        ))),
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "macos")]
async fn wait_darwin_worker_pending(
    pending: tokio::sync::oneshot::Receiver<()>,
    deadline: Instant,
) -> io::Result<()> {
    match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), pending).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) => Err(io::Error::other(
            "managed process task lost its Pending observer",
        )),
        Err(_) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "managed process task did not reach its first Pending poll",
        )),
    }
}

#[cfg(target_os = "macos")]
fn spawn_observed_darwin_exec(
    request: jackin_process::ExecRequest,
) -> (
    tokio::task::JoinHandle<anyhow::Result<jackin_process::ExecResult>>,
    tokio::sync::oneshot::Receiver<()>,
) {
    use std::future::Future as _;

    let (pending_sender, pending_receiver) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let mut future = Box::pin(crate::process_telemetry::exec_async_as(
            &request,
            jackin_telemetry::schema::enums::ProcessExecutableName::JackinCapsule,
        ));
        let mut pending_sender = Some(pending_sender);
        std::future::poll_fn(move |context| match future.as_mut().poll(context) {
            std::task::Poll::Pending => {
                if let Some(sender) = pending_sender.take() {
                    let _sent = sender.send(());
                }
                std::task::Poll::Pending
            }
            std::task::Poll::Ready(result) => std::task::Poll::Ready(result),
        })
        .await
    });
    (task, pending_receiver)
}

#[cfg(target_os = "macos")]
fn darwin_exec_request(
    directory: &Path,
    role: &str,
    timeout: Option<Duration>,
) -> io::Result<jackin_process::ExecRequest> {
    use std::ffi::OsString;

    let executable = std::env::current_exe()?;
    let environment = [
        (
            OsString::from(WORKER_ENV),
            OsString::from(DARWIN_GATED_CHILD_WORKER),
        ),
        (
            OsString::from(FIXTURE_DIRECTORY_ENV),
            directory.as_os_str().to_os_string(),
        ),
        (
            OsString::from(DARWIN_FIXTURE_ROLE_ENV),
            OsString::from(role),
        ),
    ];
    let mut request = jackin_process::ExecRequest::new(
        executable,
        ["--exact", DARWIN_GATED_CHILD_WORKER, "--nocapture"],
    )
    .envs(environment)
    .env_remove([
        DIAGNOSTIC_PYTHON_ENV,
        DIAGNOSTIC_SCRIPT_ENV,
        DIAGNOSTIC_PREEXEC_PID_ENV,
    ])
    .stdout_mode(jackin_process::StdioMode::Null)
    .stderr_mode(jackin_process::StdioMode::Null)
    .retry(jackin_process::RetryPolicy::none());
    if let Some(timeout) = timeout {
        request = request.timeout(timeout);
    }
    Ok(request)
}

#[cfg(target_os = "macos")]
struct DarwinManagedTask {
    task: Option<tokio::task::JoinHandle<anyhow::Result<jackin_process::ExecResult>>>,
    pending: Option<tokio::sync::oneshot::Receiver<()>>,
    gate: FixtureRelease,
    role: String,
}

#[cfg(target_os = "macos")]
impl DarwinManagedTask {
    fn spawn(directory: &Path, role: &str, timeout: Option<Duration>) -> io::Result<Self> {
        let request = darwin_exec_request(directory, role, timeout)?;
        let (task, pending) = spawn_observed_darwin_exec(request);
        Ok(Self {
            task: Some(task),
            pending: Some(pending),
            gate: FixtureRelease::new(directory, &format!("darwin-{role}.release")),
            role: role.to_owned(),
        })
    }

    async fn wait_started(&mut self, directory: &Path, deadline: Instant) -> io::Result<u32> {
        let pending = self
            .pending
            .take()
            .ok_or_else(|| io::Error::other("managed process Pending observer was consumed"))?;
        wait_darwin_worker_pending(pending, deadline).await?;
        let ready = read_darwin_fixture_record(
            directory.join(format!("darwin-{}.ready", self.role)),
            deadline,
        )
        .await?;
        let pids = fixture_pids(&ready)?;
        if pids.len() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("managed fixture recorded unexpected PIDs: {pids:?}"),
            ));
        }
        let pid = pids[0];
        let registered = jackin_process::coordinate_children(|registry| registry.contains(pid));
        if !registered {
            return Err(io::Error::other(format!(
                "managed fixture PID {pid} has no live reservation"
            )));
        }
        if self
            .task
            .as_ref()
            .is_none_or(tokio::task::JoinHandle::is_finished)
        {
            return Err(io::Error::other(
                "managed process task finished while its fixture gate remained closed",
            ));
        }
        Ok(pid)
    }

    fn release(&mut self) -> io::Result<()> {
        self.gate.release()
    }

    async fn take_result(&mut self, deadline: Instant) -> io::Result<jackin_process::ExecResult> {
        let joined = {
            let task = self
                .task
                .as_mut()
                .ok_or_else(|| io::Error::other("managed process task result was already taken"))?;
            tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), task).await
        };
        match joined {
            Ok(joined) => {
                drop(self.task.take());
                match joined {
                    Ok(Ok(result)) => Ok(result),
                    Ok(Err(error)) => Err(io::Error::other(format!(
                        "managed Capsule exec failed: {error:#}"
                    ))),
                    Err(error) => Err(io::Error::other(format!(
                        "managed Capsule task failed: {error}"
                    ))),
                }
            }
            Err(_) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "managed Capsule exec missed its proof deadline",
            )),
        }
    }

    async fn cancel_and_join(&mut self) -> io::Result<()> {
        let joined = {
            let task = self
                .task
                .as_mut()
                .ok_or_else(|| io::Error::other("managed process task was already consumed"))?;
            task.abort();
            tokio::time::timeout(Duration::from_secs(1), task).await
        };
        match joined {
            Ok(Err(error)) if error.is_cancelled() => {
                drop(self.task.take());
                Ok(())
            }
            Ok(Ok(_)) => {
                drop(self.task.take());
                Err(io::Error::other(
                    "managed process completed before cancellation was observed",
                ))
            }
            Ok(Err(error)) => {
                drop(self.task.take());
                Err(io::Error::other(format!(
                    "managed process cancellation failed: {error}"
                )))
            }
            Err(_) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "managed process task did not join after cancellation",
            )),
        }
    }

    async fn cleanup(&mut self) -> io::Result<()> {
        let release = self.gate.release();
        let joined = if let Some(task) = self.task.as_mut() {
            task.abort();
            Some(tokio::time::timeout(Duration::from_secs(1), task).await)
        } else {
            None
        };
        let task_cleanup = match joined {
            Some(Ok(Ok(_))) => {
                drop(self.task.take());
                Ok(())
            }
            Some(Ok(Err(error))) if error.is_cancelled() => {
                drop(self.task.take());
                Ok(())
            }
            Some(Ok(Err(error))) => {
                drop(self.task.take());
                Err(io::Error::other(format!(
                    "join managed fixture cleanup: {error}"
                )))
            }
            Some(Err(_)) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "managed fixture cleanup did not join",
            )),
            None => Ok(()),
        };
        release.and(task_cleanup)
    }
}

#[cfg(target_os = "macos")]
impl Drop for DarwinManagedTask {
    fn drop(&mut self) {
        let _released = self.gate.release();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

#[cfg(target_os = "macos")]
struct DarwinFixtureChild {
    child: Child,
    gate: FixtureRelease,
    cleanup_enabled: bool,
}

#[cfg(target_os = "macos")]
impl DarwinFixtureChild {
    fn spawn(directory: &Path, role: &str) -> io::Result<Self> {
        let child = spawn_darwin_gated_child(directory, role)?;
        Ok(Self {
            child,
            gate: FixtureRelease::new(directory, &format!("darwin-{role}.release")),
            cleanup_enabled: true,
        })
    }

    fn id(&self) -> u32 {
        self.child.id()
    }

    fn release(&mut self) -> io::Result<()> {
        self.gate.release()
    }

    fn cleanup_bounded(&mut self) -> io::Result<()> {
        if !self.cleanup_enabled {
            return Ok(());
        }
        self.cleanup_enabled = false;
        let release = self.gate.release();
        let pid = self.child.id();
        let disposition = jackin_process::coordinate_children(|registry| {
            if registry.contains(pid) {
                return Err(io::Error::other(
                    "unmanaged Darwin fixture unexpectedly has a child reservation",
                ));
            }
            match self.child.try_wait() {
                Ok(Some(_)) => Ok(false),
                Err(error) if error.raw_os_error() == Some(nix::errno::Errno::ECHILD as i32) => {
                    Ok(false)
                }
                Err(error) => Err(error),
                Ok(None) => match self.child.kill() {
                    Ok(()) => Ok(true),
                    Err(error) if error.raw_os_error() == Some(nix::errno::Errno::ESRCH as i32) => {
                        Ok(true)
                    }
                    Err(error) => Err(error),
                },
            }
        });
        let should_poll = match disposition {
            Ok(should_poll) => should_poll,
            Err(error) => {
                return Err(io::Error::other(format!(
                    "coordinate bounded unmanaged fixture cleanup: {error}; gate release: {release:?}"
                )));
            }
        };
        if !should_poll {
            return release;
        }
        let poll = match crate::util::poll_owned_child_until_timeout(
            &mut self.child,
            Duration::from_secs(2),
            FIXTURE_POLL_INTERVAL,
        ) {
            Ok(crate::util::ChildPollOutcome::Exited(_)) => Ok(()),
            Ok(crate::util::ChildPollOutcome::TimedOut) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "unmanaged Darwin fixture did not exit during bounded cleanup",
            )),
            Err(error) if error.raw_os_error() == Some(nix::errno::Errno::ECHILD as i32) => Ok(()),
            Err(error) => Err(io::Error::other(format!(
                "poll unmanaged Darwin fixture during cleanup: {error}"
            ))),
        };
        release.and(poll)
    }
}

#[cfg(target_os = "macos")]
impl Drop for DarwinFixtureChild {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup_bounded() {
            let _reported = writeln!(
                io::stderr().lock(),
                "bounded Darwin fixture cleanup failed: {error}"
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn assert_darwin_exec_success(result: &jackin_process::ExecResult) -> io::Result<()> {
    if result.success && !result.timed_out && result.code == Some(0) {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "managed Darwin fixture had unexpected result: {result:?}"
        )))
    }
}

#[cfg(target_os = "macos")]
fn ensure_darwin_registry_released(pid: u32) -> io::Result<()> {
    if jackin_process::coordinate_children(|registry| registry.contains(pid)) {
        return Err(io::Error::other(format!(
            "Darwin fixture PID {pid} retained its reservation after disposition"
        )));
    }
    assert_targeted_echild(pid)
}

#[cfg(target_os = "macos")]
async fn wait_darwin_pid_absent(pid: u32, deadline: Instant) -> io::Result<()> {
    let raw_pid = i32::try_from(pid)
        .map_err(|error| io::Error::other(format!("fixture PID does not fit i32: {error}")))?;
    loop {
        match nix::sys::signal::kill(Pid::from_raw(raw_pid), None) {
            Err(nix::errno::Errno::ESRCH) => return ensure_darwin_registry_released(pid),
            Ok(()) | Err(nix::errno::Errno::EINTR) => {}
            Err(error) => {
                return Err(io::Error::other(format!(
                    "no-signal process observation for fixture PID {pid} failed: {error}"
                )));
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("cancelled fixture PID {pid} remained present"),
            ));
        }
        tokio::time::sleep(FIXTURE_POLL_INTERVAL.min(remaining)).await;
    }
}

#[cfg(target_os = "macos")]
fn darwin_tokio_reaped(events: &[ReaperTestEvent], pid: i32) -> bool {
    let notification = events.iter().position(|event| {
        event.kind
            == (ReaperTestEventKind::TokioNotificationReceived {
                signal: Signal::SIGCHLD as i32,
            })
    });
    let selected = events.iter().position(|event| {
        event.kind
            == (ReaperTestEventKind::Selected {
                pid,
                managed: false,
            })
    });
    notification
        .zip(selected)
        .is_some_and(|(notification, selected)| {
            notification < selected && reaper_completed_reap(events, pid)
        })
}

#[cfg(target_os = "macos")]
fn assert_darwin_reaper_did_not_wait_managed(
    events: &[ReaperTestEvent],
    managed_pid: i32,
) -> io::Result<()> {
    let waited = events.iter().any(|event| {
        matches!(
            event.kind,
            ReaperTestEventKind::TargetedWaitResult { pid, .. }
                | ReaperTestEventKind::TargetedWaitError { pid, .. }
                | ReaperTestEventKind::Reaped { pid }
                if pid == managed_pid
        )
    });
    if waited {
        return Err(io::Error::other(format!(
            "selective reaper waited on registered Capsule PID {managed_pid}"
        )));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn assert_darwin_event_prefix(
    events: &[ReaperTestEvent],
    expected: impl IntoIterator<Item = ReaperTestEventKind>,
) -> io::Result<()> {
    for (index, expected) in expected.into_iter().enumerate() {
        let observed = events.get(index).map(|event| event.kind);
        if observed != Some(expected) {
            return Err(io::Error::other(format!(
                "Darwin Tokio reaper event {index} was {observed:?}, expected {expected:?}"
            )));
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn assert_darwin_tokio_requeue_events(
    events: &[ReaperTestEvent],
    child_pid: i32,
) -> io::Result<()> {
    let mut expected = Vec::with_capacity(SELECTIVE_EINTR_BUDGET + 10);
    expected.push(ReaperTestEventKind::TokioNotificationReceived {
        signal: Signal::SIGCHLD as i32,
    });
    for ordinal in 1..=SELECTIVE_EINTR_BUDGET + 1 {
        expected.push(ReaperTestEventKind::PeekEintrInjected { ordinal });
    }
    expected.extend([
        ReaperTestEventKind::PassDisposition {
            failure_errno: Some(nix::errno::Errno::EINTR as i32),
            requeue: true,
        },
        ReaperTestEventKind::BackoffStarted,
        ReaperTestEventKind::BackoffCompleted,
    ]);
    assert_darwin_event_prefix(events, expected)?;

    let wake_index = SELECTIVE_EINTR_BUDGET + 5;
    let wake = events.get(wake_index).map(|event| event.kind);
    if !matches!(
        wake,
        Some(ReaperTestEventKind::WakeAttempt {
            process_pid,
            signal,
            succeeded: true,
            errno: None,
        }) if process_pid == std::process::id() as i32
            && signal == Signal::SIGCHLD as i32
    ) {
        return Err(io::Error::other(format!(
            "Darwin retry self-wake was not a successful SIGCHLD from this worker: {wake:?}"
        )));
    }

    let signal_index = wake_index + 1;
    let final_events = [
        ReaperTestEventKind::TokioNotificationReceived {
            signal: Signal::SIGCHLD as i32,
        },
        ReaperTestEventKind::Selected {
            pid: child_pid,
            managed: false,
        },
        ReaperTestEventKind::TargetedWaitResult {
            pid: child_pid,
            status: ReaperTestWaitStatus::Exited(0),
        },
        ReaperTestEventKind::Reaped { pid: child_pid },
        ReaperTestEventKind::PassDisposition {
            failure_errno: None,
            requeue: false,
        },
    ];
    assert_darwin_event_prefix(&events[signal_index..], final_events)?;
    if events.len() != signal_index + final_events.len() {
        return Err(io::Error::other(format!(
            "Darwin EINTR requeue proof observed {} events, expected {}",
            events.len(),
            signal_index + final_events.len()
        )));
    }
    let backoff_started = events[SELECTIVE_EINTR_BUDGET + 3].elapsed;
    let backoff_completed = events[SELECTIVE_EINTR_BUDGET + 4].elapsed;
    if backoff_completed.saturating_sub(backoff_started) < REAPER_RETRY_BACKOFF {
        return Err(io::Error::other(
            "Darwin production retry backoff was shorter than 100 ms",
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn combine_darwin_proof_and_cleanup(
    role: &str,
    proof: io::Result<()>,
    cleanup: io::Result<()>,
) -> io::Result<()> {
    match (proof, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(io::Error::other(format!(
            "{role} passed but owned cleanup failed: {cleanup}"
        ))),
        (Err(primary), Err(cleanup)) => Err(io::Error::other(format!(
            "{role} proof failed: {primary}; bounded owned cleanup also failed: {cleanup}"
        ))),
    }
}

#[cfg(target_os = "macos")]
async fn prove_darwin_coexistence(directory: &Path) -> io::Result<()> {
    start_darwin_reaper_for_test()?;
    let capture = arm_reaper_test_capture(0);
    let mut events = ReaperEventLog::new(capture);
    let mut managed = DarwinManagedTask::spawn(directory, "managed", None)?;
    let proof = async {
        let deadline = FixtureTimer::deadline_after(FIXTURE_PROOF_DEADLINE)?;
        let managed_pid = managed.wait_started(directory, deadline).await?;
        let mut unowned = DarwinFixtureChild::spawn(directory, "unowned")?;
        let ready =
            read_darwin_fixture_record(directory.join("darwin-unowned.ready"), deadline).await?;
        let ready_pids = fixture_pids(&ready)?;
        if ready_pids.len() != 1 || ready_pids[0] != unowned.id() {
            return Err(io::Error::other(format!(
                "unowned Darwin fixture readiness {:?} did not match child {}",
                ready_pids,
                unowned.id()
            )));
        }
        let unowned_pid = unowned.id();
        unowned.release()?;
        let raw_pid = i32::try_from(unowned_pid)
            .map_err(|error| io::Error::other(format!("unowned PID does not fit i32: {error}")))?;
        events.wait_until(deadline, |observed| darwin_tokio_reaped(observed, raw_pid))?;
        assert_targeted_echild(unowned_pid)?;
        assert_darwin_fixture_disposition(directory, "unowned", "released\n", deadline).await?;

        let managed_raw_pid = i32::try_from(managed_pid)
            .map_err(|error| io::Error::other(format!("managed PID does not fit i32: {error}")))?;
        if managed
            .task
            .as_ref()
            .is_none_or(tokio::task::JoinHandle::is_finished)
        {
            return Err(io::Error::other(
                "registered Capsule task finished before its gate was released",
            ));
        }
        if !jackin_process::coordinate_children(|registry| registry.contains(managed_pid)) {
            return Err(io::Error::other(
                "managed Capsule reservation disappeared while its gate was closed",
            ));
        }
        assert_darwin_reaper_did_not_wait_managed(events.events(), managed_raw_pid)?;

        managed.release()?;
        let result = managed.take_result(deadline).await?;
        assert_darwin_exec_success(&result)?;
        assert_darwin_fixture_disposition(directory, "managed", "released\n", deadline).await?;
        ensure_darwin_registry_released(managed_pid)?;
        assert_darwin_reaper_did_not_wait_managed(events.events(), managed_raw_pid)?;
        events.flush()?;
        assert_darwin_reaper_did_not_wait_managed(events.events(), managed_raw_pid)?;
        Ok(())
    }
    .await;
    let cleanup = managed.cleanup().await;
    combine_darwin_proof_and_cleanup("managed/unmanaged Tokio fanout", proof, cleanup)
}

#[cfg(target_os = "macos")]
async fn prove_darwin_eintr_requeue(directory: &Path) -> io::Result<()> {
    start_darwin_reaper_for_test()?;
    let capture = arm_reaper_test_capture(SELECTIVE_EINTR_BUDGET + 1);
    let mut events = ReaperEventLog::new(capture);
    let deadline = FixtureTimer::deadline_after(FIXTURE_PROOF_DEADLINE)?;
    let mut child = DarwinFixtureChild::spawn(directory, "unowned")?;
    let ready =
        read_darwin_fixture_record(directory.join("darwin-unowned.ready"), deadline).await?;
    let ready_pids = fixture_pids(&ready)?;
    if ready_pids.len() != 1 || ready_pids[0] != child.id() {
        return Err(io::Error::other(format!(
            "EINTR fixture readiness {:?} did not match child {}",
            ready_pids,
            child.id()
        )));
    }
    let child_pid = child.id();
    let raw_pid = i32::try_from(child_pid)
        .map_err(|error| io::Error::other(format!("EINTR child PID does not fit i32: {error}")))?;
    child.release()?;
    events.wait_until(deadline, |observed| {
        reaper_completed_reap(observed, raw_pid)
    })?;
    assert_darwin_tokio_requeue_events(events.events(), raw_pid)?;
    assert_targeted_echild(child_pid)?;
    assert_darwin_fixture_disposition(directory, "unowned", "released\n", deadline).await?;
    events.require_quiet(Duration::from_millis(25))?;
    events.flush()?;
    assert_darwin_tokio_requeue_events(events.events(), raw_pid)
}

#[cfg(target_os = "macos")]
async fn prove_darwin_timeout_and_cancel(directory: &Path) -> io::Result<()> {
    start_darwin_reaper_for_test()?;
    let capture = arm_reaper_test_capture(0);
    let mut events = ReaperEventLog::new(capture);

    let mut success_raw_pid = None;
    let mut success = DarwinManagedTask::spawn(directory, "success", None)?;
    let success_proof = async {
        let deadline = FixtureTimer::deadline_after(FIXTURE_PROOF_DEADLINE)?;
        let pid = success.wait_started(directory, deadline).await?;
        success.release()?;
        let result = success.take_result(deadline).await?;
        assert_darwin_exec_success(&result)?;
        assert_darwin_fixture_disposition(directory, "success", "released\n", deadline).await?;
        ensure_darwin_registry_released(pid)?;
        let raw_pid = i32::try_from(pid)
            .map_err(|error| io::Error::other(format!("success PID does not fit i32: {error}")))?;
        events.drain_available()?;
        assert_darwin_reaper_did_not_wait_managed(events.events(), raw_pid)?;
        success_raw_pid = Some(raw_pid);
        Ok(())
    }
    .await;
    combine_darwin_proof_and_cleanup(
        "successful managed wait",
        success_proof,
        success.cleanup().await,
    )?;
    let success_raw_pid = success_raw_pid.ok_or_else(|| {
        io::Error::other("successful managed wait did not retain its observed PID")
    })?;

    let mut timeout_raw_pid = None;
    let mut timed = DarwinManagedTask::spawn(directory, "timeout", Some(Duration::from_secs(1)))?;
    let timed_proof = async {
        let deadline = FixtureTimer::deadline_after(FIXTURE_PROOF_DEADLINE)?;
        let pid = timed.wait_started(directory, deadline).await?;
        let result = timed.take_result(deadline).await?;
        if !result.timed_out || result.success || result.code.is_some() {
            return Err(io::Error::other(format!(
                "bounded Capsule timeout returned an unexpected result: {result:?}"
            )));
        }
        ensure_darwin_registry_released(pid)?;
        let raw_pid = i32::try_from(pid)
            .map_err(|error| io::Error::other(format!("timeout PID does not fit i32: {error}")))?;
        events.drain_available()?;
        assert_darwin_reaper_did_not_wait_managed(events.events(), raw_pid)?;
        timeout_raw_pid = Some(raw_pid);
        Ok(())
    }
    .await;
    combine_darwin_proof_and_cleanup(
        "managed request timeout",
        timed_proof,
        timed.cleanup().await,
    )?;
    let timeout_raw_pid = timeout_raw_pid
        .ok_or_else(|| io::Error::other("managed timeout did not retain its observed PID"))?;

    let mut cancelled = DarwinManagedTask::spawn(directory, "cancel", None)?;
    let cancelled_proof = async {
        let deadline = FixtureTimer::deadline_after(FIXTURE_PROOF_DEADLINE)?;
        let pid = cancelled.wait_started(directory, deadline).await?;
        cancelled.cancel_and_join().await?;
        wait_darwin_pid_absent(pid, deadline).await?;
        assert_darwin_cancel_fixture_has_no_disposition(directory)?;
        Ok(())
    }
    .await;
    combine_darwin_proof_and_cleanup(
        "managed cancellation handoff",
        cancelled_proof,
        cancelled.cleanup().await,
    )?;

    events.flush()?;
    assert_darwin_reaper_did_not_wait_managed(events.events(), success_raw_pid)?;
    assert_darwin_reaper_did_not_wait_managed(events.events(), timeout_raw_pid)
}

#[cfg(target_os = "macos")]
fn prove_darwin_blocked_startup_mask() -> io::Result<()> {
    check_darwin_worker_baseline()?;
    let action_before = jackin_process_directory::current_sigchld_action()?;
    let eligibility_before = jackin_process_directory::tokio_sigchld_eligibility()?;
    let capture = arm_reaper_test_capture(0);
    let mut events = ReaperEventLog::new(capture);
    let mut mask = SigchldMaskGuard::block_for_spawn()?;
    let blocked_result = (|| {
        let Err(error) = prepare_darwin_reaper() else {
            return Err(io::Error::other(
                "blocked startup mask unexpectedly passed preparation",
            ));
        };
        if !format!("{error:#}").contains("initiating thread blocks SIGCHLD") {
            return Err(io::Error::other(format!(
                "blocked-mask preflight failed for another reason: {error:#}"
            )));
        }
        if !SigSet::thread_get_mask()
            .map_err(|error| io::Error::other(format!("read blocked worker mask: {error}")))?
            .contains(Signal::SIGCHLD)
        {
            return Err(io::Error::other(
                "blocked-mask fixture did not retain its SIGCHLD block during preflight",
            ));
        }
        if DARWIN_REAPER_STATE.load(Ordering::Acquire) != DARWIN_REAPER_UNPREPARED
            || jackin_process::has_reaper_wakeup()
        {
            return Err(io::Error::other(
                "blocked-mask preflight changed owner state or published a callback",
            ));
        }
        let action_after = jackin_process_directory::current_sigchld_action()?;
        let eligibility_after = jackin_process_directory::tokio_sigchld_eligibility()?;
        if action_after != action_before || eligibility_after != eligibility_before {
            return Err(io::Error::other(
                "blocked-mask preflight changed SIGCHLD action eligibility",
            ));
        }
        Ok(())
    })();
    let restore = mask.restore();
    let proof =
        combine_darwin_proof_and_cleanup("blocked startup-mask preflight", blocked_result, restore);
    let unblocked = SigSet::thread_get_mask()
        .map_err(|error| io::Error::other(format!("read restored worker mask: {error}")))
        .map(|mask| !mask.contains(Signal::SIGCHLD));
    proof?;
    if !unblocked? {
        return Err(io::Error::other(
            "blocked-mask fixture did not restore the SIGCHLD mask",
        ));
    }
    writeln!(
        io::stderr().lock(),
        "PID1_DARWIN_TOKIO phase=blocked-mask-preflight-rejected worker_pid={} action_unchanged=true callback_absent=true registration_attempted=false mask_restored=true",
        std::process::id()
    )?;
    events.require_quiet(Duration::from_millis(25))?;
    events.flush()?;
    if !events.events().is_empty() {
        return Err(io::Error::other(
            "blocked-mask startup unexpectedly registered a Tokio listener",
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn marker_pid(diagnostics: &str, prefix: &str, field: &str) -> io::Result<u32> {
    let line = diagnostics
        .lines()
        .find(|line| line.contains(prefix))
        .ok_or_else(|| io::Error::other(format!("missing worker marker {prefix:?}")))?;
    let value = line
        .split_whitespace()
        .find_map(|part| part.strip_prefix(&format!("{field}=")))
        .ok_or_else(|| io::Error::other(format!("marker {prefix:?} has no {field} field")))?;
    value
        .parse::<u32>()
        .map_err(|error| io::Error::other(format!("invalid marker PID {value:?}: {error}")))
}

#[cfg(target_os = "macos")]
fn assert_darwin_fatal_markers(diagnostics: &str) -> io::Result<()> {
    let backend = marker_pid(diagnostics, "backend=tokio-unix-signal", "worker_pid")?;
    let registered = marker_pid(diagnostics, "phase=registered", "process_pid")?;
    let polled = marker_pid(diagnostics, "phase=polled", "worker_pid")?;
    let injected = marker_pid(diagnostics, "phase=injected-fatal", "worker_pid")?;
    let fatal = marker_pid(diagnostics, "PID1_DARWIN_TOKIO_FATAL", "pid")?;
    if backend == registered && registered == polled && polled == injected && injected == fatal {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "post-registration fatal markers came from different PIDs: backend={backend}, registered={registered}, polled={polled}, injected={injected}, fatal={fatal}"
        )))
    }
}

#[cfg(target_os = "macos")]
fn assert_darwin_active_markers(diagnostics: &str) -> io::Result<()> {
    let backend = marker_pid(diagnostics, "backend=tokio-unix-signal", "worker_pid")?;
    let registered = marker_pid(diagnostics, "phase=registered", "process_pid")?;
    let polled = marker_pid(diagnostics, "phase=polled", "worker_pid")?;
    let active = marker_pid(diagnostics, "phase=active", "worker_pid")?;
    if backend == registered && registered == polled && polled == active {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Tokio backend/readiness/active markers came from different PIDs: backend={backend}, registered={registered}, polled={polled}, active={active}"
        )))
    }
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_tokio_reaper_coexists_with_registered_child_wait() -> io::Result<()> {
    let diagnostics = run_darwin_ordinary_worker(DARWIN_COEXIST_WORKER, 0)?;
    assert_darwin_active_markers(&diagnostics)?;
    if diagnostics.contains("backend=tokio-unix-signal")
        && diagnostics.contains("TokioNotificationReceived")
        && diagnostics.contains("Selected { pid:")
    {
        Ok(())
    } else {
        Err(io::Error::other(
            "ordinary coexistence worker omitted its Tokio backend or selective-reap evidence",
        ))
    }
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "current_thread")]
async fn worker_darwin_tokio_reaper_coexists_with_registered_child_wait() -> io::Result<()> {
    if !worker_selected(DARWIN_COEXIST_WORKER) {
        return Ok(());
    }
    let directory = fixture_directory()?;
    prove_darwin_coexistence(&directory).await
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_tokio_reaper_requeues_real_eintr_exhaustion() -> io::Result<()> {
    let diagnostics = run_darwin_ordinary_worker(DARWIN_EINTR_WORKER, 0)?;
    assert_darwin_active_markers(&diagnostics)?;
    if diagnostics.contains("TokioNotificationReceived")
        && diagnostics.contains("PeekEintrInjected { ordinal: 17 }")
        && diagnostics.contains("requeue: true")
        && diagnostics.contains("WakeAttempt")
    {
        Ok(())
    } else {
        Err(io::Error::other(
            "ordinary EINTR worker omitted the Tokio requeue/self-wake evidence",
        ))
    }
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "current_thread")]
async fn worker_darwin_tokio_reaper_requeues_real_eintr_exhaustion() -> io::Result<()> {
    if !worker_selected(DARWIN_EINTR_WORKER) {
        return Ok(());
    }
    let directory = fixture_directory()?;
    prove_darwin_eintr_requeue(&directory).await
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_tokio_registered_child_timeout_and_cancel_handoff() -> io::Result<()> {
    let diagnostics = run_darwin_ordinary_worker(DARWIN_HANDOFF_WORKER, 0)?;
    assert_darwin_active_markers(&diagnostics)?;
    if diagnostics.contains("TokioNotificationReceived") {
        Ok(())
    } else {
        Err(io::Error::other(
            "ordinary timeout/cancellation worker omitted a real Tokio notification",
        ))
    }
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "current_thread")]
async fn worker_darwin_tokio_registered_child_timeout_and_cancel_handoff() -> io::Result<()> {
    if !worker_selected(DARWIN_HANDOFF_WORKER) {
        return Ok(());
    }
    let directory = fixture_directory()?;
    prove_darwin_timeout_and_cancel(&directory).await
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_tokio_reaper_rejects_blocked_startup_mask() -> io::Result<()> {
    let diagnostics = run_darwin_ordinary_worker(DARWIN_MASK_WORKER, 0)?;
    let backend = marker_pid(&diagnostics, "backend=tokio-unix-signal", "worker_pid")?;
    let blocked = marker_pid(
        &diagnostics,
        "phase=blocked-mask-preflight-rejected",
        "worker_pid",
    )?;
    if backend == blocked {
        Ok(())
    } else {
        Err(io::Error::other(
            "blocked-mask preflight did not preserve the ordinary worker identity",
        ))
    }
}

#[cfg(target_os = "macos")]
#[test]
fn worker_darwin_tokio_reaper_rejects_blocked_startup_mask() -> io::Result<()> {
    if worker_selected(DARWIN_MASK_WORKER) {
        prove_darwin_blocked_startup_mask()
    } else {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_tokio_reaper_fails_closed_after_registered_worker_panic() -> io::Result<()> {
    let diagnostics = run_darwin_ordinary_worker(DARWIN_FATAL_WORKER, 1)?;
    assert_darwin_fatal_markers(&diagnostics)
}

#[cfg(target_os = "macos")]
#[test]
fn worker_darwin_tokio_reaper_fails_closed_after_registered_worker_panic() -> io::Result<()> {
    if !worker_selected(DARWIN_FATAL_WORKER) {
        return Ok(());
    }
    check_darwin_worker_baseline()?;
    inject_darwin_worker_panic_after_poll();
    prepare_darwin_reaper().map_err(|error| {
        io::Error::other(format!("fatal worker returned an early error: {error:#}"))
    })?;
    Err(io::Error::other(
        "prepare returned despite the post-registration fatal worker injection",
    ))
}

#[cfg(target_os = "macos")]
#[test]
fn worker_darwin_tokio_gated_child() -> io::Result<()> {
    if worker_selected(DARWIN_GATED_CHILD_WORKER) {
        run_darwin_gated_child()
    } else {
        Ok(())
    }
}

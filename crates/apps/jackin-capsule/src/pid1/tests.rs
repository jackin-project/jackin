// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Isolated direct-child tests for PID 1's selective reaper.

use super::*;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const WORKER_ENV: &str = "JACKIN_PID1_TEST_WORKER";
const MAX_WORKER_DIAGNOSTIC_BYTES: u64 = 64 * 1024;

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
fn wait_until_exited(pid: u32) {
    wait_for_child_exit(pid, TestChildSelector::Specific)
        .expect("known child exits before the selector deadline");
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
static TEST_WAKE_SENDER: std::sync::OnceLock<std::sync::mpsc::SyncSender<()>> =
    std::sync::OnceLock::new();

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
    let (wake_tx, wake_rx) = std::sync::mpsc::sync_channel(1);
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
    let (wake_tx, wake_rx) = std::sync::mpsc::sync_channel(1);
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

mod selective_pass_faults;

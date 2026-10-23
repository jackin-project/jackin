// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::decode_child_payload;
use std::io;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const WORKER_ENV: &str = "JACKIN_MACOS_WAIT_TEST_WORKER";
const WORKER_TIMEOUT: Duration = Duration::from_secs(5);

fn run_isolated_worker(name: &str) {
    let executable = std::env::current_exe().expect("test executable path");
    let mut child = Command::new(executable)
        .args(["--exact", name, "--nocapture"])
        .env(WORKER_ENV, name)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn isolated selector worker");
    let deadline = Instant::now() + WORKER_TIMEOUT;
    loop {
        match child.try_wait().expect("poll isolated selector worker") {
            Some(status) => {
                assert!(
                    status.success(),
                    "isolated selector worker failed: {status}"
                );
                break;
            }
            None if Instant::now() < deadline => selector_poll_delay(deadline),
            None => {
                stop_known_child(&mut child);
                panic!("isolated selector worker exceeded its test deadline");
            }
        }
    }
}

fn stop_known_child(child: &mut Child) {
    if child.try_wait().expect("poll known test child").is_none() {
        child.kill().expect("kill known test child handle");
        child.wait().expect("reap known test child handle");
    }
}

fn wait_for_selected_child(child: &Child) -> u32 {
    let expected = child.id();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match super::super::peek_exited_child().expect("peek exited child") {
            Some(pid) if pid == expected => return pid,
            Some(pid) => panic!("selected unexpected child {pid}; expected {expected}"),
            None if Instant::now() < deadline => selector_poll_delay(deadline),
            None => panic!("child {expected} did not become selectable before deadline"),
        }
    }
}

fn wait_for_specific_child(child: &Child) {
    let expected = child.id();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if super::super::peek_specific_exited_child(expected).expect("peek specific child") {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "child {expected} did not become waitable before deadline"
        );
        selector_poll_delay(deadline);
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "bounded selector polls wait for known fixture children outside a runtime"
)]
fn selector_poll_delay(deadline: Instant) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if !remaining.is_zero() {
        std::thread::sleep(Duration::from_millis(5).min(remaining));
    }
}

#[test]
fn isolated_no_child_is_echild() {
    run_isolated_worker("macos_wait::tests::worker_no_child_is_echild");
}

#[test]
fn isolated_live_child_is_not_selected() {
    run_isolated_worker("macos_wait::tests::worker_live_child_is_not_selected");
}

#[test]
fn isolated_exited_child_remains_waitable_until_owner_waits() {
    run_isolated_worker("macos_wait::tests::worker_exited_child_remains_waitable");
}

#[test]
fn isolated_signaled_child_has_terminal_payload() {
    run_isolated_worker("macos_wait::tests::worker_signaled_child_has_terminal_payload");
}

#[test]
fn isolated_targeted_live_child_is_not_selected() {
    run_isolated_worker("macos_wait::tests::worker_targeted_live_child_is_not_selected");
}

#[test]
fn isolated_targeted_exited_child_remains_waitable() {
    run_isolated_worker("macos_wait::tests::worker_targeted_exited_child_remains_waitable");
}

#[test]
fn worker_no_child_is_echild() {
    if std::env::var(WORKER_ENV).ok().as_deref()
        != Some("macos_wait::tests::worker_no_child_is_echild")
    {
        return;
    }
    let error = super::super::peek_exited_child().expect_err("no child must report ECHILD");
    assert_eq!(error.raw_os_error(), Some(libc::ECHILD));
}

#[test]
fn worker_live_child_is_not_selected() {
    if std::env::var(WORKER_ENV).ok().as_deref()
        != Some("macos_wait::tests::worker_live_child_is_not_selected")
    {
        return;
    }
    let mut child = Command::new("/bin/sleep")
        .arg("5")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn known live fixture child");
    assert_eq!(
        super::super::peek_exited_child().expect("peek live child"),
        None
    );
    stop_known_child(&mut child);
}

#[test]
fn worker_exited_child_remains_waitable() {
    if std::env::var(WORKER_ENV).ok().as_deref()
        != Some("macos_wait::tests::worker_exited_child_remains_waitable")
    {
        return;
    }
    let mut child = spawn_exit_fixture("exit-23");
    let pid = wait_for_selected_child(&child);
    assert_eq!(
        super::super::peek_exited_child().expect("repeat peek"),
        Some(pid)
    );
    assert_eq!(
        super::super::peek_exited_child().expect("third peek"),
        Some(pid)
    );
    let status = child.wait().expect("owner consumes selected child");
    assert_eq!(status.code(), Some(23));
    let error =
        super::super::peek_exited_child().expect_err("consumed status is no longer a child");
    assert_eq!(error.raw_os_error(), Some(libc::ECHILD));
}

#[test]
fn worker_signaled_child_has_terminal_payload() {
    if std::env::var(WORKER_ENV).ok().as_deref()
        != Some("macos_wait::tests::worker_signaled_child_has_terminal_payload")
    {
        return;
    }
    let mut child = spawn_exit_fixture("signal-self");
    let pid = wait_for_selected_child(&child);
    assert_eq!(pid, child.id());
    assert!(
        super::super::peek_exited_child()
            .expect("repeat signaled peek")
            .is_some()
    );
    let status = child.wait().expect("owner consumes signaled child");
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(status.signal(), Some(libc::SIGKILL));
}

#[test]
fn worker_targeted_live_child_is_not_selected() {
    if std::env::var(WORKER_ENV).ok().as_deref()
        != Some("macos_wait::tests::worker_targeted_live_child_is_not_selected")
    {
        return;
    }
    let mut child = Command::new("/bin/sleep")
        .arg("5")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn known live fixture child");
    assert!(
        !super::super::peek_specific_exited_child(child.id()).expect("peek live target"),
        "a live specific child must not be selected"
    );
    stop_known_child(&mut child);
}

#[test]
fn worker_targeted_exited_child_remains_waitable() {
    if std::env::var(WORKER_ENV).ok().as_deref()
        != Some("macos_wait::tests::worker_targeted_exited_child_remains_waitable")
    {
        return;
    }
    let mut child = spawn_exit_fixture("exit-23");
    wait_for_specific_child(&child);
    assert!(super::super::peek_specific_exited_child(child.id()).expect("repeat targeted peek"));
    let status = child.wait().expect("owner consumes targeted child status");
    assert_eq!(status.code(), Some(23));
    let error = super::super::peek_specific_exited_child(child.id())
        .expect_err("consumed status is no longer a direct child");
    assert_eq!(error.raw_os_error(), Some(libc::ECHILD));
}

#[test]
fn child_fixture() {
    if let Ok("signal-self") = std::env::var("JACKIN_MACOS_WAIT_CHILD_FIXTURE").as_deref() {
        let _kill_result =
            nix::sys::signal::kill(nix::unistd::Pid::this(), nix::sys::signal::Signal::SIGKILL);
    }
}

fn spawn_exit_fixture(kind: &str) -> Child {
    if kind == "exit-23" {
        return Command::new("/bin/sh")
            .args(["-c", "exit 23"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn known exit-23 child");
    }
    let executable = std::env::current_exe().expect("test executable path");
    Command::new(executable)
        .args(["--exact", "macos_wait::tests::child_fixture", "--nocapture"])
        .env("JACKIN_MACOS_WAIT_CHILD_FIXTURE", kind)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn known signaled child fixture")
}

#[test]
fn payload_rejects_negative_pid() {
    let error = decode_child_payload(-1, libc::SIGCHLD, libc::CLD_EXITED)
        .expect_err("negative child PID is invalid");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn payload_rejects_unexpected_signal_and_code() {
    assert_eq!(
        decode_child_payload(1, libc::SIGTERM, libc::CLD_EXITED)
            .expect_err("unexpected signal")
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        decode_child_payload(1, libc::SIGCHLD, libc::CLD_STOPPED)
            .expect_err("non-terminal code")
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn payload_maps_zero_pid_to_no_result() {
    assert_eq!(decode_child_payload(0, 0, 0).expect("zero sentinel"), None);
}

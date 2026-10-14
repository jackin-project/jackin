// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::process::{Command, Stdio};

#[test]
fn reap_zombies_returns_when_no_children() {
    reap_zombies();
}

#[test]
fn waitpid_wnohang_returns_exit_status_after_synchronous_wait() {
    let mut child = Command::new("true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn /bin/true");
    let pid = Pid::from_raw(i32::try_from(child.id()).unwrap_or(i32::MAX));
    let status = child.wait().expect("wait /bin/true");
    assert!(status.success());
    let probe = waitpid(pid, Some(WaitPidFlag::WNOHANG));
    probe.expect_err("expected ECHILD");
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[test]
fn reap_zombies_does_not_steal_registered_session_child() {
    let mut child = Command::new("true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn /bin/true");
    let pid = Pid::from_raw(i32::try_from(child.id()).unwrap_or(i32::MAX));
    register_managed_child(child.id());
    waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT)
        .expect("child should exit but remain waitable");

    reap_zombies();

    let status = child
        .wait()
        .expect("session owner should still be able to reap child");
    unregister_managed_child(child.id());
    assert!(status.success());
}

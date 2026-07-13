//! Tests for `pid1`.
use super::*;
use std::process::{Command, Stdio};

#[test]
fn reap_zombies_returns_when_no_children() {
    // No children, no zombie queue — reap_zombies must return
    // quickly. If it spins or blocks, this test hangs and the
    // CI runner kills it. Regression guard against a refactor
    // that drops the WNOHANG flag from the loop.
    reap_zombies();
}

#[test]
fn waitpid_wnohang_returns_exit_status_after_synchronous_wait() {
    // Spawn /bin/true, wait synchronously, then re-`waitpid` with
    // WNOHANG. The child is reaped by `Child::wait`, so WNOHANG
    // returns ECHILD ("no such process"). This pins the kernel
    // contract the reaper loop relies on: after a reap, WNOHANG
    // sees no zombie and the inner `match` short-circuits.
    let mut child = Command::new("true")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn /bin/true");
    let pid = Pid::from_raw(child.id() as i32);
    let status = child.wait().expect("wait /bin/true");
    assert!(status.success());
    let probe = waitpid(pid, Some(WaitPidFlag::WNOHANG));
    // ECHILD is the kernel's "no zombie for this pid" response —
    // identical to the `Err(_)` arm the reaper short-circuits on.
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
    let pid = Pid::from_raw(child.id() as i32);
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

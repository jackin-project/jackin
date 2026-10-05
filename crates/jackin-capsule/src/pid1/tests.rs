// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `pid1`.
use super::*;
#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
use nix::sys::wait::{Id, waitid};
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
    let (mut child, registration) = child_ownership::coordinate(|registry| {
        let child = Command::new("true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn /bin/true");
        let registration = registry.register(child.id());
        (child, registration)
    });
    let pid = Pid::from_raw(i32::try_from(child.id()).unwrap_or(i32::MAX));
    let status = child.wait().expect("wait /bin/true");
    drop(registration);
    assert!(status.success());
    let probe = waitpid(pid, Some(WaitPidFlag::WNOHANG));
    // ECHILD is the kernel's "no zombie for this pid" response —
    // identical to the `Err(_)` arm the reaper short-circuits on.
    probe.expect_err("expected ECHILD");
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[test]
fn reap_zombies_does_not_steal_registered_session_child() {
    let (mut child, registration) = child_ownership::coordinate(|registry| {
        let child = Command::new("true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn /bin/true");
        let registration = registry.register(child.id());
        (child, registration)
    });
    let pid = Pid::from_raw(i32::try_from(child.id()).unwrap_or(i32::MAX));
    waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT)
        .expect("child should exit but remain waitable");

    reap_zombies();

    let status = child
        .wait()
        .expect("session owner should still be able to reap child");
    drop(registration);
    assert!(status.success());
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[tokio::test]
async fn reaper_preserves_exec_leader_while_descendant_holds_capture_pipe() {
    let fixture = tempfile::tempdir().expect("fixture directory");
    let pid_path = fixture.path().join("leader");
    let release_path = fixture.path().join("release");
    let request = jackin_process::ExecRequest::new(
        "sh",
        [
            "-c",
            "(while [ ! -e \"$2\" ]; do sleep 0.01; done) & printf '%s' $$ > \"$1\"; exit 7",
            "fixture",
            pid_path.to_str().expect("fixture path"),
            release_path.to_str().expect("release path"),
        ],
    );
    let execution = tokio::spawn(async move { jackin_process::exec_async(&request).await });
    let pid = fixture_pid(&pid_path).await;
    waitid(
        Id::Pid(Pid::from_raw(i32::try_from(pid).expect("child PID"))),
        WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT,
    )
    .expect("leader exited with descendant retaining capture pipe");
    assert!(child_ownership::coordinate(
        |registry| registry.contains(pid)
    ));
    reap_zombies();
    std::fs::write(release_path, []).expect("release descendant capture pipe");
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), execution)
        .await
        .expect("capture completes")
        .expect("execution task")
        .expect("owner retains exit status");
    assert_eq!(result.code, Some(7));
    assert!(!child_ownership::coordinate(
        |registry| registry.contains(pid)
    ));
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
async fn fixture_pid(path: &std::path::Path) -> u32 {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(path)
                && let Ok(pid) = text.parse()
            {
                return pid;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("fixture reports its PID")
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[test]
fn spawn_registration_excludes_reaper_at_exited_child_boundary() {
    let (attempted_tx, attempted_rx) = std::sync::mpsc::channel();
    let (mut child, registration, reaper) = child_ownership::coordinate(|registry| {
        let child = Command::new("true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn fixture");
        let pid = Pid::from_raw(i32::try_from(child.id()).expect("fixture PID"));
        waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT)
            .expect("child already exited before registration");
        let reaper = std::thread::spawn(move || {
            attempted_tx.send(()).expect("announce reaper attempt");
            reap_zombies();
        });
        attempted_rx
            .recv()
            .expect("reaper started during spawn gap");
        let registration = registry.register(child.id());
        (child, registration, reaper)
    });
    reaper
        .join()
        .expect("reaper finishes after spawn lock releases");
    assert!(child.wait().expect("owner retains zombie status").success());
    drop(registration);
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[tokio::test]
async fn cancelled_exec_kills_descendant_before_releasing_reaper_reservation() {
    let fixture = tempfile::tempdir().expect("fixture directory");
    let leader_path = fixture.path().join("leader");
    let descendant_path = fixture.path().join("descendant");
    let request = jackin_process::ExecRequest::new(
        "sh",
        [
            "-c",
            "sleep 30 & printf '%s' $$ > \"$1\"; printf '%s' $! > \"$2\"; exit 0",
            "fixture",
            leader_path.to_str().expect("leader path"),
            descendant_path.to_str().expect("descendant path"),
        ],
    );
    let execution = tokio::spawn(async move { jackin_process::exec_async(&request).await });
    let leader = fixture_pid(&leader_path).await;
    let descendant = fixture_pid(&descendant_path).await;
    waitid(
        Id::Pid(Pid::from_raw(i32::try_from(leader).expect("leader PID"))),
        WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT,
    )
    .expect("leader exited while descendant retains capture pipe");
    assert!(child_ownership::coordinate(
        |registry| registry.contains(leader)
    ));
    reap_zombies();
    execution.abort();
    assert!(
        execution
            .await
            .expect_err("execution cancelled")
            .is_cancelled()
    );
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while child_ownership::coordinate(|registry| registry.contains(leader)) {
            reap_zombies();
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("canonical cancellation waiter releases reaped leader");
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            // A zombie has terminated and awaits the host init's orphan reap.
            let terminated =
                std::fs::read_to_string(format!("/proc/{descendant}/stat")).map_or(true, |stat| {
                    stat.rsplit_once(')')
                        .is_some_and(|(_, state)| state.trim_start().starts_with('Z'))
                });
            if terminated {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("cancellation terminates descendant holding capture pipe");
    reap_zombies();
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[test]
fn reaper_preserves_shared_sync_spawn_status() {
    let request = jackin_process::ExecRequest::new("true", None::<&str>);
    let mut child = jackin_process::spawn_sync(&request).expect("registered spawn");
    let pid = Pid::from_raw(i32::try_from(child.id()).expect("child PID"));
    waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT)
        .expect("exited child remains waitable");
    reap_zombies();
    assert!(child.wait().expect("owner retains status").success());
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[tokio::test]
async fn reaper_preserves_shared_async_spawn_status() {
    let request = jackin_process::ExecRequest::new("true", None::<&str>);
    let mut child = jackin_process::spawn_async(&request).expect("registered spawn");
    let pid = Pid::from_raw(i32::try_from(child.id().expect("child PID")).expect("child PID"));
    waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT)
        .expect("exited child remains waitable");
    reap_zombies();
    assert!(child.wait().await.expect("owner retains status").success());
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[test]
fn registered_zombie_cannot_hide_unowned_sibling_from_reaper() {
    let (mut owned, registration, mut orphan) = child_ownership::coordinate(|registry| {
        let owned = Command::new("true").spawn().expect("owned child");
        let registration = registry.register(owned.id());
        let orphan = Command::new("true")
            .spawn()
            .expect("unowned orphan fixture");
        (owned, registration, orphan)
    });
    for child in [&owned, &orphan] {
        let pid = Pid::from_raw(i32::try_from(child.id()).expect("child PID"));
        waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT)
            .expect("child exited without reaping");
    }
    reap_zombies();
    assert!(
        owned
            .wait()
            .expect("managed owner retains status")
            .success()
    );
    assert_eq!(
        orphan
            .wait()
            .expect_err("unowned zombie reaped")
            .raw_os_error(),
        Some(libc::ECHILD)
    );
    drop(registration);
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[test]
fn registered_async_drop_reaps_without_a_live_tokio_runtime() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("fixture runtime");
    let child = runtime.block_on(async {
        jackin_process::spawn_async(&jackin_process::ExecRequest::new("sleep", ["30"]))
            .expect("registered async child")
    });
    let pid = child.id().expect("child PID");
    drop(runtime);
    drop(child);
    let started = std::time::Instant::now();
    while child_ownership::coordinate(|registry| registry.contains(pid)) {
        reap_zombies();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "native cleanup owner did not reap"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        nix::sys::wait::waitpid(
            Pid::from_raw(i32::try_from(pid).expect("PID")),
            Some(WaitPidFlag::WNOHANG)
        ),
        Err(nix::errno::Errno::ECHILD),
    );
}

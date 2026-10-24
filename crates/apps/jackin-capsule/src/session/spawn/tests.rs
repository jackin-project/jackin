// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(any(
    all(target_os = "linux", not(target_env = "uclibc")),
    target_os = "macos"
))]

use super::{SpawnedChild, spawn_child_with_registration};
use nix::sys::wait::{WaitPidFlag, waitpid};
use portable_pty::{CommandBuilder, MasterPty, PtySize, SlavePty, native_pty_system};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const WORKER_ENV: &str = "JACKIN_PTY_OWNERSHIP_WORKER";
const MAX_WORKER_DIAGNOSTIC_BYTES: u64 = 64 * 1024;

fn run_worker(name: &str) {
    let executable = std::env::current_exe().expect("test executable path");
    let mut stdout_capture = tempfile::tempfile().expect("create PTY worker stdout capture");
    let mut stderr_capture = tempfile::tempfile().expect("create PTY worker stderr capture");
    let mut child = Command::new(executable)
        .args(["--exact", name, "--nocapture"])
        .env(WORKER_ENV, name)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            stdout_capture
                .try_clone()
                .expect("clone PTY worker stdout capture"),
        ))
        .stderr(Stdio::from(
            stderr_capture
                .try_clone()
                .expect("clone PTY worker stderr capture"),
        ))
        .spawn()
        .expect("spawn isolated PTY ownership worker");
    match crate::util::poll_owned_child_until_timeout(
        &mut child,
        Duration::from_secs(10),
        Duration::from_millis(5),
    )
    .expect("poll isolated PTY worker")
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
                "isolated PTY worker failed: {status}\n{diagnostics}"
            );
        }
        crate::util::ChildPollOutcome::TimedOut => {
            stop_known_child(&mut child);
            let diagnostics = worker_failure_diagnostics(
                name,
                "timed out after 10 seconds",
                &mut stdout_capture,
                &mut stderr_capture,
            );
            panic!("isolated PTY ownership worker exceeded its deadline\n{diagnostics}");
        }
    }
}

fn worker_failure_diagnostics(
    name: &str,
    disposition: &str,
    stdout_capture: &mut File,
    stderr_capture: &mut File,
) -> String {
    let mut diagnostics = format!("isolated PTY ownership worker {name} {disposition}");
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
    if child.try_wait().expect("poll known test child").is_none() {
        child.kill().expect("kill known test child handle");
        child.wait().expect("reap known test child handle");
    }
}

fn pty() -> (Box<dyn MasterPty + Send>, Box<dyn SlavePty + Send>) {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("open native PTY");
    (pair.master, pair.slave)
}

fn exit_command() -> CommandBuilder {
    let mut command = CommandBuilder::new("/bin/sh");
    command.arg("-c");
    command.arg("exit 0");
    command
}

fn live_command() -> CommandBuilder {
    let mut command = CommandBuilder::new("/bin/sleep");
    command.arg("30");
    command
}

fn wait_until_exited(pid: u32) {
    crate::pid1::wait_for_child_exit(pid, crate::pid1::TestChildSelector::Any)
        .expect("known PTY child exits before the selector deadline");
}

fn wait_for_registry_contention(previous: usize) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while jackin_process::registry_contention_count() == previous {
        assert!(
            Instant::now() < deadline,
            "the fixture thread did not positively observe registry lock contention"
        );
        std::hint::spin_loop();
    }
}

fn await_spawn_release(
    receiver: &mpsc::Receiver<()>,
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
) -> anyhow::Result<Box<dyn portable_pty::Child + Send + Sync>> {
    if receiver.recv_timeout(Duration::from_secs(3)).is_ok() {
        return Ok(child);
    }
    drop(child.kill());
    drop(child.wait());
    anyhow::bail!("controller did not release PTY spawn seam");
}

#[test]
fn pty_spawn_registration_precedes_reaper_selection() {
    run_worker("session::spawn::tests::worker_registered_spawn_race");
}

#[test]
fn worker_registered_spawn_race() {
    if std::env::var(WORKER_ENV).ok().as_deref()
        != Some("session::spawn::tests::worker_registered_spawn_race")
    {
        return;
    }
    let (master, slave) = pty();
    let (pid_tx, pid_rx) = mpsc::sync_channel(0);
    let (continue_tx, continue_rx) = mpsc::sync_channel(0);
    let spawn_thread = std::thread::spawn(move || {
        spawn_child_with_registration(|| {
            let child = slave.spawn_command(exit_command())?;
            let pid = child.process_id();
            let child = if let Some(pid) = pid {
                pid_tx.send(pid).map_err(|error| anyhow::anyhow!(error))?;
                await_spawn_release(&continue_rx, child)?
            } else {
                child
            };
            Ok((child, pid))
        })
    });

    let pid = pid_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("actual PTY child spawned under shared coordinate");
    wait_until_exited(pid);
    let contention_before = jackin_process::registry_contention_count();
    let (attempt_tx, attempt_rx) = mpsc::sync_channel(0);
    let reaper = std::thread::spawn(move || {
        attempt_tx.send(()).expect("announce reaper entry attempt");
        crate::pid1::reap_zombies();
    });
    attempt_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("reaper reaches production helper while spawn coordinate is held");
    wait_for_registry_contention(contention_before);
    continue_tx
        .send(())
        .expect("complete PTY registration seam");

    let spawned = spawn_thread
        .join()
        .expect("coordinated PTY spawn thread")
        .expect("coordinated PTY spawn");
    reaper.join().expect("production PID 1 reaper thread");
    let (mut child, registration) = match spawned {
        SpawnedChild::Registered {
            child,
            pid,
            registration,
        } => {
            assert_eq!(
                pid,
                child.process_id().expect("PTY child PID remains available")
            );
            (child, registration)
        }
        SpawnedChild::MissingPid(mut child) => {
            drop(child.kill());
            drop(child.wait());
            panic!("native PTY backend must expose a child PID");
        }
    };
    wait_until_exited(registration.pid());
    let status = child.wait().expect("owner can wait after reaper pass");
    assert!(status.success());
    drop(registration);
    drop(master);
}

#[test]
fn old_spawn_before_coordinate_variant_loses_wait_status() {
    run_worker("session::spawn::tests::worker_old_order_mutant");
}

#[test]
fn live_registered_pty_does_not_defer_unowned_child_reap() {
    run_worker("session::spawn::tests::worker_live_pty_with_unowned_child");
}

#[test]
fn worker_live_pty_with_unowned_child() {
    if std::env::var(WORKER_ENV).ok().as_deref()
        != Some("session::spawn::tests::worker_live_pty_with_unowned_child")
    {
        return;
    }
    let (master, slave) = pty();
    let spawned = spawn_child_with_registration(|| {
        let child = slave.spawn_command(live_command())?;
        let pid = child.process_id();
        Ok((child, pid))
    });
    let (mut child, registration) = match spawned {
        Ok(SpawnedChild::Registered {
            child,
            registration,
            ..
        }) => (child, registration),
        Ok(SpawnedChild::MissingPid(mut child)) => {
            drop(child.kill());
            drop(child.wait());
            panic!("native PTY backend must expose a child PID");
        }
        Err(error) => panic!("spawn registered live PTY child: {error}"),
    };

    let mut unowned = Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn known unowned child");
    let unowned_pid =
        nix::unistd::Pid::from_raw(i32::try_from(unowned.id()).expect("unowned PID fits i32"));
    wait_until_exited(unowned.id());

    crate::pid1::reap_zombies();
    assert_eq!(
        waitpid(unowned_pid, Some(WaitPidFlag::WNOHANG))
            .expect_err("actual PID 1 reaper consumes the unowned child"),
        nix::errno::Errno::ECHILD
    );
    assert_eq!(
        unowned
            .wait()
            .expect_err("reaper-owned Child handle reports ECHILD")
            .raw_os_error(),
        Some(nix::errno::Errno::ECHILD as i32)
    );

    drop(child.kill());
    assert!(
        child.wait().is_ok(),
        "registered PTY owner retains its wait status"
    );
    drop(registration);
    drop(master);
}

#[test]
fn worker_old_order_mutant() {
    if std::env::var(WORKER_ENV).ok().as_deref()
        != Some("session::spawn::tests::worker_old_order_mutant")
    {
        return;
    }
    let (master, slave) = pty();
    let mut child = slave
        .spawn_command(exit_command())
        .expect("old-order fixture spawn before coordinate");
    let pid = child.process_id().expect("native PTY child PID");
    wait_until_exited(pid);

    let (attempt_tx, attempt_rx) = mpsc::sync_channel(0);
    let reaper = std::thread::spawn(move || {
        attempt_tx
            .send(())
            .expect("announce old-order reaper attempt");
        crate::pid1::reap_zombies();
    });
    attempt_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("old-order reaper enters production helper");
    reaper.join().expect("old-order production reaper pass");

    let owner_wait = child
        .wait()
        .expect_err("old-order reaper consumes the unowned PTY child");
    assert_eq!(
        owner_wait.raw_os_error(),
        Some(nix::errno::Errno::ECHILD as i32),
        "old-order spawn-before-register variant must lose the wait status"
    );
    let registration = jackin_process::coordinate_children(|registry| {
        registry.register(pid).expect("late mutant reservation")
    });
    registration.release_to_reaper();
    drop(master);
}

#[test]
fn selection_check_wait_excludes_concurrent_pty_spawn() {
    run_worker("session::spawn::tests::worker_selection_wait_exclusion");
}

#[test]
fn worker_selection_wait_exclusion() {
    if std::env::var(WORKER_ENV).ok().as_deref()
        != Some("session::spawn::tests::worker_selection_wait_exclusion")
    {
        return;
    }
    let mut orphan = Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn known unowned direct child");
    let orphan_pid =
        nix::unistd::Pid::from_raw(i32::try_from(orphan.id()).expect("orphan PID fits i32"));
    wait_until_exited(orphan.id());

    let (selected_tx, selected_rx) = mpsc::sync_channel(0);
    let (continue_select_tx, continue_select_rx) = mpsc::sync_channel(0);
    let (reaped_tx, reaped_rx) = mpsc::sync_channel(0);
    let (continue_wait_tx, continue_wait_rx) = mpsc::sync_channel(0);
    struct Observer {
        selected_tx: mpsc::SyncSender<nix::unistd::Pid>,
        continue_select_rx: mpsc::Receiver<()>,
        reaped_tx: mpsc::SyncSender<nix::unistd::Pid>,
        continue_wait_rx: mpsc::Receiver<()>,
    }
    impl crate::pid1::ReaperObserver for Observer {
        fn selected(&mut self, pid: nix::unistd::Pid, managed: bool) {
            if !managed {
                let _sent = self.selected_tx.send(pid);
                self.continue_select_rx
                    .recv_timeout(Duration::from_secs(3))
                    .expect("controller releases the selection gate");
            }
        }

        fn reaped(&mut self, pid: nix::unistd::Pid) {
            let _sent = self.reaped_tx.send(pid);
            self.continue_wait_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("controller releases the selected-wait gate");
        }
    }
    let mut observer = Observer {
        selected_tx,
        continue_select_rx,
        reaped_tx,
        continue_wait_rx,
    };
    let reaper = std::thread::spawn(move || {
        crate::pid1::reap_zombies_selective_with_observer(&mut observer)
    });
    assert_eq!(
        selected_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("production selector exposes selected unowned child"),
        orphan_pid
    );

    let (master, slave) = pty();
    let (attempted_tx, attempted_rx) = mpsc::sync_channel(0);
    let (spawned_tx, spawned_rx) = mpsc::sync_channel(1);
    let contention_before = jackin_process::registry_contention_count();
    let spawn_thread = std::thread::spawn(move || {
        attempted_tx
            .send(())
            .expect("announce coordinated spawn attempt");
        spawn_child_with_registration(|| {
            let child = slave.spawn_command(exit_command())?;
            let pid = child.process_id();
            if let Some(pid) = pid {
                let _sent = spawned_tx.send(pid);
            }
            Ok((child, pid))
        })
    });
    attempted_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("spawner reaches the shared coordinate call");
    wait_for_registry_contention(contention_before);

    continue_select_tx
        .send(())
        .expect("allow actual ownership check");
    assert_eq!(
        reaped_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("production reaper completes selected-PID wait"),
        orphan_pid
    );
    // The observer remains inside the shared critical section until this
    // gate opens. The PTY closure cannot run before selected-PID wait.
    assert!(matches!(
        spawned_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    continue_wait_tx
        .send(())
        .expect("release reaper after selected wait");

    let selected_failure = reaper.join().expect("selected-PID reaper thread");
    assert!(selected_failure.is_none());
    assert_eq!(
        waitpid(orphan_pid, Some(WaitPidFlag::WNOHANG))
            .expect_err("selected-PID wait consumed the unowned child"),
        nix::errno::Errno::ECHILD
    );
    assert_eq!(
        orphan
            .wait()
            .expect_err("reaper-owned Child handle reports ECHILD")
            .raw_os_error(),
        Some(nix::errno::Errno::ECHILD as i32)
    );
    let spawned = spawn_thread
        .join()
        .expect("coordinated PTY spawn thread")
        .expect("coordinated PTY spawn");
    let spawned_pid = spawned_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("PTY spawn begins only after reaper lock is released");
    let (mut child, registration) = match spawned {
        SpawnedChild::Registered {
            child,
            pid,
            registration,
        } => {
            assert_eq!(pid, spawned_pid);
            (child, registration)
        }
        SpawnedChild::MissingPid(mut child) => {
            drop(child.kill());
            drop(child.wait());
            panic!("native PTY backend must expose a child PID");
        }
    };
    wait_until_exited(spawned_pid);
    assert!(
        child
            .wait()
            .expect("owner waits spawned PTY child")
            .success()
    );
    drop(registration);
    drop(master);
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(any(target_os = "linux", target_os = "macos"))]

use super::{NativeSpawnGuard, native_spawn_guard, notice_pipe_after_create};
use std::ffi::OsString;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::TryLockError;
use std::sync::mpsc::{self, TryRecvError};
use std::time::Duration;

const FD_SCAN_HELPER: &str = r#"
import os,sys
expected_identities={tuple(int(part) for part in value.split(":", 1)) for value in sys.argv[1:]}
root="/proc/self/fd" if os.path.isdir("/proc/self/fd") else "/dev/fd"
inherited_identities=[]
for name in os.listdir(root):
    try:
        fd=int(name)
        identity=os.fstat(fd)
    except OSError as error:
        if error.errno != 9:
            raise
        continue
    if (identity.st_dev,identity.st_ino) in expected_identities:
        inherited_identities.append((fd,identity.st_dev,identity.st_ino))
print("INHERITED_IDENTITIES="+repr(sorted(inherited_identities)),flush=True)
raise SystemExit(1 if inherited_identities else 0)
"#;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct PipeIdentity {
    device: u64,
    inode: u64,
}

fn pipe_identity(fd: &OwnedFd) -> io::Result<PipeIdentity> {
    let stat = rustix::fs::fstat(fd)?;
    #[cfg(target_os = "linux")]
    let device = stat.st_dev;
    #[cfg(target_os = "macos")]
    let device = u64::try_from(stat.st_dev)
        .map_err(|_| io::Error::other("pipe device identity is negative"))?;
    let inode = stat.st_ino;
    Ok(PipeIdentity { device, inode })
}

impl PipeIdentity {
    const fn is_nonzero(self) -> bool {
        self.device != 0 || self.inode != 0
    }
}

fn final_descriptor_is_safe(fd: &OwnedFd) -> bool {
    fd.as_raw_fd() >= 3
        && rustix::io::fcntl_getfd(fd)
            .is_ok_and(|flags| flags.contains(rustix::io::FdFlags::CLOEXEC))
        && rustix::fs::fcntl_getfl(fd)
            .is_ok_and(|flags| flags.contains(rustix::fs::OFlags::NONBLOCK))
}

fn spawn_fd_scan(identities: (PipeIdentity, PipeIdentity)) -> io::Result<Child> {
    let arguments = [
        OsString::from("-c"),
        OsString::from(FD_SCAN_HELPER),
        OsString::from(format!("{}:{}", identities.0.device, identities.0.inode)),
        OsString::from(format!("{}:{}", identities.1.device, identities.1.inode)),
    ];
    Command::new("python3")
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "direct native Command spawn is deliberately held under NativeSpawnGuard"
)]
fn native_spawn_notice_pipe_closes_inheritable_window_before_fork() {
    let (attempted_tx, attempted_rx) = mpsc::channel();
    let (acquired_tx, acquired_rx) = mpsc::channel();
    let (scan_tx, scan_rx) = mpsc::channel::<Option<(PipeIdentity, PipeIdentity)>>();
    let mut prepared: Option<(OwnedFd, OwnedFd)> = None;
    let mut original_identities = None;
    let mut identity_error = None;
    let mut pipe_error = None;
    let mut callback_blocked = false;
    let mut blocked_after_prepare = false;
    let mut final_descriptors_safe_before_release = false;
    let mut acquired_after_release = false;
    let mut contender_joined = false;
    let mut helper_status = None;
    let mut helper_stdout = None;
    let mut helper_stderr = None;
    let mut helper_error = None;

    let mut spawn = Some(native_spawn_guard());
    std::thread::scope(|scope| {
        let contender = scope.spawn(move || -> io::Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
            if !matches!(
                super::NATIVE_SPAWN.try_lock(),
                Err(TryLockError::WouldBlock)
            ) {
                return Err(io::Error::other(
                    "native spawn contender was not blocked by the owner",
                ));
            }
            attempted_tx
                .send(())
                .map_err(|_| io::Error::other("native spawn attempt observer dropped"))?;
            let guard = native_spawn_guard();
            acquired_tx
                .send(())
                .map_err(|_| io::Error::other("native spawn acquisition observer dropped"))?;
            let identities = scan_rx
                .recv_timeout(Duration::from_secs(1))
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "scan input not delivered"))?
                .ok_or_else(|| io::Error::other("notice-pipe identity unavailable"))?;
            let child = spawn_fd_scan(identities);
            drop(guard);
            let output = child?.wait_with_output()?;
            Ok((output.status, output.stdout, output.stderr))
        });
        {
            let spawn_guard: &NativeSpawnGuard = spawn
                .as_ref()
                .unwrap_or_else(|| panic!("native spawn guard disappeared"));
            match notice_pipe_after_create(spawn_guard, |read, write| {
                match pipe_identity(read).and_then(|read_identity| {
                    pipe_identity(write).map(|write_identity| (read_identity, write_identity))
                }) {
                    Ok(identities) if identities.0.is_nonzero() && identities.1.is_nonzero() => {
                        original_identities = Some(identities);
                    }
                    Ok(_) => {
                        identity_error = Some("pipe identity was all zero".to_owned());
                    }
                    Err(error) => identity_error = Some(error.to_string()),
                }
                let announced = attempted_rx.recv_timeout(Duration::from_secs(1)).is_ok();
                callback_blocked =
                    announced && matches!(acquired_rx.try_recv(), Err(TryRecvError::Empty));
            }) {
                Ok(pipes) => {
                    blocked_after_prepare =
                        matches!(acquired_rx.try_recv(), Err(TryRecvError::Empty));
                    final_descriptors_safe_before_release =
                        final_descriptor_is_safe(&pipes.0) && final_descriptor_is_safe(&pipes.1);
                    prepared = Some(pipes);
                }
                Err(error) => pipe_error = Some(error.to_string()),
            }
        }
        if scan_tx.send(original_identities).is_err() {
            helper_error = Some("native spawn scan input receiver dropped".to_owned());
        }
        drop(spawn.take());
        acquired_after_release = acquired_rx.recv_timeout(Duration::from_secs(1)).is_ok();
        match contender.join() {
            Ok(Ok((status, stdout, stderr))) => {
                contender_joined = true;
                helper_status = Some(status);
                helper_stdout = Some(stdout);
                helper_stderr = Some(stderr);
            }
            Ok(Err(error)) => {
                contender_joined = true;
                helper_error = Some(error.to_string());
            }
            Err(_) => {
                helper_error = Some("native spawn contender panicked".to_owned());
            }
        }
    });

    assert!(
        pipe_error.is_none(),
        "notice-pipe setup failed: {pipe_error:?}"
    );
    assert!(
        identity_error.is_none(),
        "notice-pipe identity failed: {identity_error:?}"
    );
    assert!(
        callback_blocked,
        "native spawn contender was not blocked in raw FD window"
    );
    assert!(
        blocked_after_prepare,
        "native spawn contender acquired before prepared descriptors were closed"
    );
    assert!(
        final_descriptors_safe_before_release,
        "final notice descriptors were not safe before guard release"
    );
    assert!(
        acquired_after_release,
        "native spawn contender did not acquire after guard release"
    );
    assert!(contender_joined, "native spawn contender panicked");

    assert!(
        helper_error.is_none(),
        "FD-scan helper failed: {helper_error:?}"
    );
    let (read, write) = prepared.unwrap_or_else(|| panic!("notice-pipe descriptors missing"));
    assert!(
        final_descriptor_is_safe(&read) && final_descriptor_is_safe(&write),
        "final notice descriptors must be >=3, CLOEXEC, and NONBLOCK"
    );

    let status = helper_status.unwrap_or_else(|| panic!("FD-scan helper status missing"));
    let stdout = String::from_utf8_lossy(
        &helper_stdout.unwrap_or_else(|| panic!("FD-scan helper output missing")),
    )
    .into_owned();
    let stderr = String::from_utf8_lossy(
        &helper_stderr.unwrap_or_else(|| panic!("FD-scan helper stderr missing")),
    )
    .into_owned();
    assert!(
        status.success(),
        "FD-scan helper inherited descriptors or failed: stdout={stdout}, stderr={stderr}"
    );
    assert!(
        stdout.contains("INHERITED_IDENTITIES=[]"),
        "notice-pipe identities leaked into child: {stdout}"
    );
}

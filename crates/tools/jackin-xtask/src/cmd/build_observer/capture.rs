// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use nix::fcntl::{FcntlArg, OFlag, fcntl};
use std::{
    io::{ErrorKind, Read},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const CLEANUP: Duration = Duration::from_millis(50);

pub(super) fn ps(args: &[&str], cap: usize, end: Instant) -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    let program = "/bin/ps";
    #[cfg(target_os = "linux")]
    let program = "/usr/bin/ps";
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (args, cap, end);
        return None;
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        command(program, args, cap, end)
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "bounded OS observations run in synchronous host-side xtask work"
)]
fn drain(child: &mut Child, cap: usize, end: Instant) -> Option<Vec<u8>> {
    let mut stdout = child.stdout.take()?;
    let flags = OFlag::from_bits_truncate(fcntl(&stdout, FcntlArg::F_GETFL).ok()?);
    fcntl(&stdout, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).ok()?;
    let mut bytes = Vec::new();
    let mut eof = false;
    let mut status = None;
    let mut block = [0; 4096];
    while Instant::now() + CLEANUP < end {
        if !eof {
            match stdout.read(&mut block) {
                Ok(0) => eof = true,
                Ok(size) => {
                    if bytes.len().checked_add(size)? > cap {
                        return None;
                    }
                    bytes.extend_from_slice(block.get(..size)?);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(_) => return None,
            }
        }
        if status.is_none() {
            status = child.try_wait().ok()?;
        }
        if eof && let Some(done) = status {
            return done.success().then_some(bytes);
        }
        thread::sleep(Duration::from_millis(1));
    }
    None
}

#[expect(
    clippy::disallowed_methods,
    reason = "bounded owned-child cleanup runs in synchronous host-side xtask work"
)]
pub(super) fn stop(child: &mut Child, end: Instant) {
    if child.try_wait().ok().flatten().is_none() {
        drop(child.kill());
    }
    while Instant::now() < end {
        match child.try_wait() {
            Ok(None) => thread::sleep(Duration::from_millis(1)),
            Ok(Some(_)) | Err(_) => return,
        }
    }
}

fn command(program: &str, args: &[&str], cap: usize, end: Instant) -> Option<Vec<u8>> {
    if Instant::now() + CLEANUP >= end {
        return None;
    }
    let mut child = Command::new(program)
        .args(args)
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let output = drain(&mut child, cap, end);
    if output.is_none() {
        stop(&mut child, end);
    }
    output
}

// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Synchronous process capture for the standalone pre-commit owner.

use std::{
    fs::File,
    io::{self, Read},
    path::Path,
    process::{Command, Output, Stdio},
    thread,
};

use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};

/// Capture a command from the synchronous hook-owner CLI.
///
/// This executable runs outside Jackin's async runtime and render threads. The
/// caller configures the command's exact environment and working directory.
pub(super) fn capture(command: &mut Command) -> io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()
}

/// Capture a command while keeping each output stream within its own budget.
///
/// The readers continue to drain after a budget is exceeded. This lets the
/// child exit without blocking on a full pipe. The caller receives an error,
/// never a truncated `Output` value.
pub(super) fn capture_limited(
    command: &mut Command,
    stdout_limit: usize,
    stderr_limit: usize,
) -> io::Result<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("captured command has no stdout pipe"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("captured command has no stderr pipe"))?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout, stdout_limit));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, stderr_limit));
    let status = child.wait();
    let stdout = stdout_reader
        .join()
        .map_err(|_| io::Error::other("stdout capture thread panicked"))
        .and_then(|result| result);
    let stderr = stderr_reader
        .join()
        .map_err(|_| io::Error::other("stderr capture thread panicked"))
        .and_then(|result| result);
    let status = status?;
    let stdout = stdout?;
    let stderr = stderr?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_bounded(mut reader: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(limit.min(16 * 1024));
    let mut buffer = [0; 16 * 1024];
    let mut exceeded = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(output.len());
        let accepted = count.min(remaining);
        output.extend_from_slice(&buffer[..accepted]);
        exceeded |= accepted != count;
    }
    if exceeded {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("captured output exceeded {limit} byte limit"),
        ));
    }
    Ok(output)
}

/// Open a read handle for synchronous snapshot identity verification.
pub(super) fn open_readonly(path: &Path) -> io::Result<File> {
    let descriptor =
        open(path, OFlag::O_RDONLY | OFlag::O_CLOEXEC, Mode::empty()).map_err(io::Error::from)?;
    Ok(File::from(descriptor))
}

/// Create a private file without following or replacing an existing path.
pub(super) fn create_private_new(path: &Path) -> io::Result<File> {
    let descriptor = open(
        path,
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(io::Error::from)?;
    Ok(File::from(descriptor))
}

#[cfg(test)]
mod tests;

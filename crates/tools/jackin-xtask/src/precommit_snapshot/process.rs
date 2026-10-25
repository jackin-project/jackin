// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Synchronous process capture for the standalone pre-commit owner.

use std::{
    fs::File,
    io,
    path::Path,
    process::{Command, Output, Stdio},
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

// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Synchronous process capture for the standalone pre-commit owner.

use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
    process::{Command, Output},
};

/// Capture a command from the synchronous hook-owner CLI.
///
/// This executable runs outside Jackin's async runtime and render threads. The
/// caller configures the command's exact environment and working directory.
#[expect(
    clippy::disallowed_methods,
    reason = "the standalone pre-commit owner performs synchronous CLI work"
)]
pub(super) fn capture(command: &mut Command) -> io::Result<Output> {
    command.output()
}

/// Open a read handle for synchronous snapshot identity verification.
#[expect(
    clippy::disallowed_methods,
    reason = "the standalone hook owner verifies a retained file descriptor synchronously"
)]
pub(super) fn open_readonly(path: &Path) -> io::Result<File> {
    File::open(path)
}

/// Open a configured private file from the synchronous hook owner.
#[expect(
    clippy::disallowed_methods,
    reason = "the standalone hook owner creates private custody files synchronously"
)]
pub(super) fn open_configured(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    options.open(path)
}

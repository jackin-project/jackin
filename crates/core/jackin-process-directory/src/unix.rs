// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::sync::Arc;

#[expect(
    unsafe_code,
    reason = "sole audited Unix pre_exec boundary; callback invokes only async-signal-safe fchdir"
)]
pub fn current_dir(command: &mut Command, directory: Arc<File>) -> io::Result<()> {
    if command.get_current_dir().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "pathname cwd and descriptor cwd are mutually exclusive",
        ));
    }
    if !directory.metadata()?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "pinned working directory descriptor is not a directory",
        ));
    }
    let descriptor = rustix::io::fcntl_dupfd_cloexec(directory.as_ref(), 3)?;

    // SAFETY: The callback only calls fchdir (async-signal-safe) and converts
    // its errno to io::Error without allocation. No locks, allocation, cwd
    // pathname lookup, Arc operations, or destructor run in the callback.
    // Capturing OwnedFd keeps it valid until spawn completes. Duplication above
    // fd 2 prevents the child's standard-stream setup from replacing it.
    unsafe {
        command.pre_exec(move || rustix::process::fchdir(&descriptor).map_err(io::Error::from));
    }
    Ok(())
}

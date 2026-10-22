// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use anyhow::Result;
use std::fs;

#[test]
fn source_reader_rejects_symlinked_files_and_ancestors() -> Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    fs::create_dir(root.path().join("sources"))?;
    fs::write(outside.path().join("test.swift"), "outside")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.path().join("test.swift"),
        root.path().join("sources/test.swift"),
    )?;
    #[cfg(not(unix))]
    fs::write(root.path().join("sources/test.swift"), "inside")?;
    let reader = RepositoryDirectory::open(root.path())?;
    #[cfg(unix)]
    reader
        .read_file(Path::new("sources/test.swift"), 1024)
        .unwrap_err();
    #[cfg(not(unix))]
    assert_eq!(
        reader.read_file(Path::new("sources/test.swift"), 1024)?,
        b"inside"
    );
    reader
        .read_file(Path::new("../test.swift"), 1024)
        .unwrap_err();
    Ok(())
}

#[cfg(unix)]
#[test]
fn bounded_reader_rejects_fifo_without_blocking() -> Result<()> {
    use std::{sync::mpsc, thread, time::Duration};
    let root = tempfile::tempdir()?;
    nix::unistd::mkfifo(
        &root.path().join("report.xml"),
        nix::sys::stat::Mode::from_bits_truncate(0o600),
    )?;
    let reader = RepositoryDirectory::open(root.path())?;
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _send_result = sender.send(reader.read_file(Path::new("report.xml"), 1024).is_err());
    });
    assert!(receiver.recv_timeout(Duration::from_secs(2))?);
    Ok(())
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded, owner-only atomic persistence for monitor state.

use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use jackin_protocol::usage_monitor::{MonitorIssue, MonitorIssueCode};
use nix::fcntl::{OFlag, open, openat, renameat};
use nix::sys::stat::{Mode, fchmod, mkdirat};
use nix::unistd::{UnlinkatFlags, fsync, geteuid, unlinkat};

use super::StoreState;

const STORE_FILE: &str = "state.json";
const STORE_DIR: &str = "monitor";
const STORE_MODE: u32 = 0o600;
const STORE_DIR_MODE: u32 = 0o700;
pub(super) const MAX_STORE_BYTES: usize = 32 * 1024 * 1024;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Open or create the private monitor directory below the host broker state.
pub(super) fn open_store_dir(data_dir: &Path) -> Result<File, MonitorIssue> {
    crate::secure_run_directory(data_dir).map_err(|_| unavailable())?;
    let broker_path = data_dir.join(crate::BROKER_DIR);
    let broker_fd = open(
        &broker_path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| unavailable())?;
    let broker_dir = File::from(broker_fd);
    validate_owned_dir(&broker_dir, STORE_DIR_MODE)?;

    match mkdirat(
        &broker_dir,
        STORE_DIR,
        Mode::from_bits_truncate(STORE_DIR_MODE as u16),
    ) {
        Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
        Err(_) => return Err(unavailable()),
    }
    let store_fd = openat(
        &broker_dir,
        STORE_DIR,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| unavailable())?;
    let store_dir = File::from(store_fd);
    fchmod(&store_dir, Mode::from_bits_truncate(STORE_DIR_MODE as u16))
        .map_err(|_| unavailable())?;
    validate_owned_dir(&store_dir, STORE_DIR_MODE)?;
    Ok(store_dir)
}

/// Load one validated store. Missing state is returned as `None`.
pub(super) fn load(dir: &File) -> Result<Option<StoreState>, MonitorIssue> {
    let fd = match openat(
        dir,
        STORE_FILE,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(nix::errno::Errno::ENOENT) => return Ok(None),
        Err(_) => return Err(unavailable()),
    };
    let file = File::from(fd);
    validate_owned_file(&file)?;
    let size = usize::try_from(file.metadata().map_err(|_| unavailable())?.len())
        .map_err(|_| unavailable())?;
    if size > MAX_STORE_BYTES {
        return Err(unavailable());
    }
    let mut bytes = Vec::with_capacity(size);
    file.take(u64::try_from(MAX_STORE_BYTES + 1).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() > MAX_STORE_BYTES {
        return Err(unavailable());
    }
    let state = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    Ok(Some(state))
}

/// Atomically publish one bounded state snapshot with mode `0600`.
pub(super) fn save(dir: &File, state: &StoreState) -> Result<(), MonitorIssue> {
    let bytes = serde_json::to_vec(state).map_err(|_| unavailable())?;
    if bytes.len() > MAX_STORE_BYTES {
        return Err(unavailable());
    }

    // Refuse a symlink or foreign-mode existing target before replacing it.
    match openat(
        dir,
        STORE_FILE,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    ) {
        Ok(fd) => validate_owned_file(&File::from(fd))?,
        Err(nix::errno::Errno::ENOENT) => {}
        Err(_) => return Err(unavailable()),
    }

    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = format!(".state.{}.{}.tmp", std::process::id(), counter);
    let fd = openat(
        dir,
        temporary.as_str(),
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW,
        Mode::from_bits_truncate(STORE_MODE as u16),
    )
    .map_err(|_| unavailable())?;
    let mut file = File::from(fd);
    let result = (|| {
        fchmod(&file, Mode::from_bits_truncate(STORE_MODE as u16)).map_err(|_| unavailable())?;
        validate_owned_file(&file)?;
        file.write_all(&bytes).map_err(|_| unavailable())?;
        file.sync_all().map_err(|_| unavailable())?;
        renameat(dir, temporary.as_str(), dir, STORE_FILE).map_err(|_| unavailable())?;
        fsync(dir).map_err(|_| unavailable())
    })();
    if result.is_err() {
        let _ignored = unlinkat(dir, temporary.as_str(), UnlinkatFlags::NoRemoveDir);
    }
    result
}

fn validate_owned_dir(dir: &File, expected_mode: u32) -> Result<(), MonitorIssue> {
    let metadata = dir.metadata().map_err(|_| unavailable())?;
    if !metadata.is_dir()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != expected_mode
    {
        return Err(unavailable());
    }
    Ok(())
}

fn validate_owned_file(file: &File) -> Result<(), MonitorIssue> {
    let metadata = file.metadata().map_err(|_| unavailable())?;
    if !metadata.is_file()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != STORE_MODE
        || metadata.nlink() != 1
    {
        return Err(unavailable());
    }
    Ok(())
}

fn unavailable() -> MonitorIssue {
    MonitorIssue {
        code: MonitorIssueCode::MonitorStoreUnavailable,
        message: "monitor state is unavailable or invalid".to_owned(),
        retry_at_epoch: None,
    }
}

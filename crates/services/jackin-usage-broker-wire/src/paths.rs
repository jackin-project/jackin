// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker run-directory hardening.

use std::fs::{self, File};

use std::os::unix::fs::MetadataExt as _;

use std::path::{Path, PathBuf};

use jackin_protocol::usage_broker::UsageCoordinationError;
use nix::fcntl::{OFlag, open, openat};

use super::{BROKER_DIR, BROKER_RUN_DIR, unavailable};
use nix::sys::stat::{Mode, fchmod, mkdirat};
use nix::unistd::geteuid;

pub fn secure_run_directory(data_dir: &Path) -> Result<PathBuf, UsageCoordinationError> {
    fs::create_dir_all(data_dir).map_err(|_| unavailable())?;
    let data_fd = open(
        data_dir,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| unavailable())?;
    let data = File::from(data_fd);
    validate_owned_base_directory(&data)?;
    let broker = private_child_directory(&data, BROKER_DIR)?;
    let run = private_child_directory(&broker, BROKER_RUN_DIR)?;
    drop(run);
    Ok(data_dir.join(BROKER_DIR).join(BROKER_RUN_DIR))
}

pub fn private_child_directory(parent: &File, name: &str) -> Result<File, UsageCoordinationError> {
    match mkdirat(parent, name, Mode::from_bits_truncate(0o700)) {
        Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
        Err(_) => return Err(unavailable()),
    }
    let fd = openat(
        parent,
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| unavailable())?;
    let directory = File::from(fd);
    fchmod(&directory, Mode::from_bits_truncate(0o700)).map_err(|_| unavailable())?;
    validate_owned_directory(&directory)?;
    Ok(directory)
}

pub fn validate_owned_directory(directory: &File) -> Result<(), UsageCoordinationError> {
    let metadata = directory.metadata().map_err(|_| unavailable())?;
    if !metadata.is_dir()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err(unavailable());
    }
    Ok(())
}

pub fn validate_owned_base_directory(directory: &File) -> Result<(), UsageCoordinationError> {
    let metadata = directory.metadata().map_err(|_| unavailable())?;
    if !metadata.is_dir() || metadata.uid() != geteuid().as_raw() || metadata.mode() & 0o022 != 0 {
        return Err(unavailable());
    }
    Ok(())
}

pub fn validate_owned_mode(path: &Path, mode: u32) -> Result<(), UsageCoordinationError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| unavailable())?;
    if metadata.file_type().is_symlink()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != mode
    {
        return Err(unavailable());
    }
    Ok(())
}

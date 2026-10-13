// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Credential mount presence and locking.

#[cfg(any(test, feature = "test-support"))]
use super::path_key;
use super::{
    AuthMountLease, ensure_same_source_identity, entry_file_at, entry_stat, file_target_lock,
    nix_error, open_directory_at, open_parent, validate_owned_stat,
};

use nix::errno::Errno;

use nix::sys::stat::{SFlag, fstat};

use std::path::Path;

pub fn mount_file_present(path: &Path) -> anyhow::Result<bool> {
    let (parent, target, normalized) = match open_parent(path, false) {
        Ok(value) => value,
        Err(error)
            if error
                .chain()
                .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
        {
            return Ok(false);
        }
        Err(error) => return Err(error),
    };
    Ok(entry_file_at(&parent, &target, &normalized, "credential mount file")?.is_some())
}

pub fn mount_directory_present(path: &Path) -> anyhow::Result<bool> {
    let (parent, target, normalized) = match open_parent(path, false) {
        Ok(value) => value,
        Err(error)
            if error
                .chain()
                .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
        {
            return Ok(false);
        }
        Err(error) => return Err(error),
    };
    let Some(expected) = entry_stat(&parent, &target)? else {
        return Ok(false);
    };
    validate_owned_stat(&expected, "credential mount directory", SFlag::S_IFDIR)?;
    let directory = open_directory_at(
        &parent,
        &target,
        &format!(
            "opening credential mount directory {}",
            normalized.display()
        ),
    )?;
    let actual = fstat(&directory)
        .map_err(|error| nix_error(error, "statting credential mount directory"))?;
    ensure_same_source_identity(&expected, &actual, "credential mount directory")?;
    Ok(true)
}

pub fn lock_mount_file(path: &Path) -> anyhow::Result<Option<AuthMountLease>> {
    let (parent, target, normalized, lease) = match file_target_lock(path) {
        Ok(value) => value,
        Err(error)
            if error
                .chain()
                .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    if entry_file_at(&parent, &target, &normalized, "credential mount file")?.is_some() {
        Ok(Some(lease))
    } else {
        Ok(None)
    }
}

pub fn lock_mount_directory(path: &Path) -> anyhow::Result<Option<AuthMountLease>> {
    let (parent, target, normalized, lease) = match file_target_lock(path) {
        Ok(value) => value,
        Err(error)
            if error
                .chain()
                .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let Some(expected) = entry_stat(&parent, &target)? else {
        return Ok(None);
    };
    validate_owned_stat(&expected, "credential mount directory", SFlag::S_IFDIR)?;
    let directory = open_directory_at(
        &parent,
        &target,
        &format!(
            "opening credential mount directory {}",
            normalized.display()
        ),
    )?;
    let actual = fstat(&directory)
        .map_err(|error| nix_error(error, "statting credential mount directory"))?;
    ensure_same_source_identity(&expected, &actual, "credential mount directory")?;
    Ok(Some(lease))
}

#[cfg(any(test, feature = "test-support"))]
pub fn target_lock_key_for_test(path: &Path) -> anyhow::Result<String> {
    path_key(path)
}

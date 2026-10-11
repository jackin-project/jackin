// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Private file replace/create/remove/repair.

use super::{
    ensure_same_source_identity, entry_stat, file_target_lock, fsync_directory,
    new_private_file_temporary, nix_error, owned_fd, unlink_entry, validate_owned_stat,
    write_private_file_at,
};

use anyhow::Context;

use nix::errno::Errno;
use nix::fcntl::{OFlag, openat, renameat};
use nix::sys::stat::{Mode, SFlag, fchmod, fstat};
use nix::unistd::{UnlinkatFlags, geteuid, unlinkat};

use std::io::{Read, Write};

use std::path::Path;

use crate::{PermissionRepairFailure, maybe_inject_permission_repair_failure};

pub fn replace_private_file(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let (parent, target, normalized, _lease) = file_target_lock(path)?;
    if let Some(expected) = entry_stat(&parent, &target)? {
        let kind = SFlag::from_bits_truncate(expected.st_mode);
        anyhow::ensure!(
            kind.contains(SFlag::S_IFREG),
            "refusing to replace non-regular auth file at {}",
            normalized.display()
        );
        anyhow::ensure!(
            expected.st_uid == geteuid().as_raw(),
            "auth file at {} is not owned by the current user",
            normalized.display()
        );
        let existing = owned_fd(
            openat(
                &parent,
                target.as_c_str(),
                OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|error| nix_error(error, "opening existing private auth file"))?,
        );
        let actual = fstat(&existing)
            .map_err(|error| nix_error(error, "statting existing private auth file"))?;
        validate_owned_stat(&actual, "existing private auth file", SFlag::S_IFREG)?;
        ensure_same_source_identity(&expected, &actual, "existing private auth file")?;
        let mut existing_bytes = Vec::new();
        Read::by_ref(&mut &existing)
            .read_to_end(&mut existing_bytes)
            .context("reading existing private auth file")?;
        if existing_bytes == bytes {
            fchmod(&existing, Mode::from_bits_truncate(0o600))
                .map_err(|error| nix_error(error, "restricting private auth file"))?;
            existing.sync_all().context("syncing private auth file")?;
            return Ok(());
        }
    }
    let temporary = new_private_file_temporary(&parent)?;
    let result = (|| {
        write_private_file_at(&parent, &temporary, bytes, "private auth file")?;
        renameat(&parent, temporary.as_c_str(), &parent, target.as_c_str())
            .map_err(|error| nix_error(error, "publishing private auth file"))?;
        fsync_directory(&parent)
    })();
    if result.is_err() {
        let _ignored_cleanup =
            unlink_entry(&parent, &temporary, "removing failed private auth file");
    }
    result
}

pub fn create_private_file_if_absent(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let (parent, target, normalized, _lease) = file_target_lock(path)?;
    let fd = match openat(
        &parent,
        target.as_c_str(),
        OFlag::O_WRONLY
            | OFlag::O_CREAT
            | OFlag::O_EXCL
            | OFlag::O_NOFOLLOW
            | OFlag::O_CLOEXEC
            | OFlag::O_NONBLOCK,
        Mode::from_bits_truncate(0o600),
    ) {
        Ok(fd) => fd,
        Err(Errno::EEXIST) => {
            let stat = entry_stat(&parent, &target)?.ok_or_else(|| {
                anyhow::anyhow!(
                    "private auth file {} disappeared after create collision",
                    normalized.display()
                )
            })?;
            validate_owned_stat(&stat, "existing private auth file", SFlag::S_IFREG)?;
            let file = owned_fd(
                openat(
                    &parent,
                    target.as_c_str(),
                    OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|error| nix_error(error, "opening existing private auth file"))?,
            );
            let actual = fstat(&file)
                .map_err(|error| nix_error(error, "statting existing private auth file"))?;
            validate_owned_stat(&actual, "existing private auth file", SFlag::S_IFREG)?;
            ensure_same_source_identity(&stat, &actual, "existing private auth file")?;
            return Ok(());
        }
        Err(error) => return Err(nix_error(error, "creating private auth file")),
    };
    let file = owned_fd(fd);
    let stat = fstat(&file).map_err(|error| nix_error(error, "statting private auth file"))?;
    validate_owned_stat(&stat, "private auth file", SFlag::S_IFREG)?;
    fchmod(&file, Mode::from_bits_truncate(0o600))
        .map_err(|error| nix_error(error, "restricting private auth file"))?;
    let mut file = file;
    file.write_all(bytes)
        .with_context(|| format!("writing private skeleton at {}", normalized.display()))?;
    file.sync_all()
        .with_context(|| format!("syncing private skeleton at {}", normalized.display()))
}

pub fn remove_file(path: &Path) -> anyhow::Result<()> {
    let (parent, target, _, _lease) = match file_target_lock(path) {
        Ok(value) => value,
        Err(error)
            if error
                .chain()
                .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
        {
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    match unlinkat(&parent, target.as_c_str(), UnlinkatFlags::NoRemoveDir) {
        Ok(()) | Err(Errno::ENOENT) => Ok(()),
        Err(error) => Err(nix_error(error, "removing private auth file")),
    }
}

pub fn repair_file_permissions(path: &Path) -> anyhow::Result<()> {
    let (parent, target, normalized, _lease) = match file_target_lock(path) {
        Ok(value) => value,
        Err(error)
            if error
                .chain()
                .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
        {
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    maybe_inject_permission_repair_failure(PermissionRepairFailure::Stat)?;
    let Some(expected) = entry_stat(&parent, &target)? else {
        return Ok(());
    };
    validate_owned_stat(&expected, "credential file", SFlag::S_IFREG)?;
    anyhow::ensure!(
        expected.st_uid == geteuid().as_raw(),
        "credential file at {} is not owned by the current user",
        normalized.display()
    );
    maybe_inject_permission_repair_failure(PermissionRepairFailure::Chmod)?;
    let file = owned_fd(
        openat(
            &parent,
            target.as_c_str(),
            OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| nix_error(error, "opening credential file for permission repair"))?,
    );
    let actual = fstat(&file)
        .map_err(|error| nix_error(error, "statting credential file for permission repair"))?;
    validate_owned_stat(&actual, "credential file", SFlag::S_IFREG)?;
    ensure_same_source_identity(
        &expected,
        &actual,
        &format!("credential file {}", normalized.display()),
    )?;
    fchmod(&file, Mode::from_bits_truncate(0o600))
        .map_err(|error| nix_error(error, "chmod 0o600 on credential file"))?;
    maybe_inject_permission_repair_failure(PermissionRepairFailure::Verify)?;
    let verified =
        fstat(&file).map_err(|error| nix_error(error, "verifying credential file permissions"))?;
    ensure_same_source_identity(
        &expected,
        &verified,
        &format!("credential file {}", normalized.display()),
    )?;
    anyhow::ensure!(
        verified.st_mode & 0o7777 == 0o600,
        "credential file at {} is not exactly mode 0600 after repair",
        normalized.display()
    );
    Ok(())
}

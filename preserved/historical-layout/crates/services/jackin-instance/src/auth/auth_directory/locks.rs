// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Source locks and transaction staging names.

use super::{
    AUTH_DIRECTORY_SWAP_COUNTER, AuthMountLease, LockedSource, SOURCE_LOCK_POLL,
    SOURCE_LOCK_TIMEOUT, TargetLock, ensure_same_source_identity, entry_stat, nix_error,
    open_directory_at, open_parent, open_private_file, owned_fd, path_key, recover,
    validate_directory, validate_owned_stat,
};

use anyhow::Context;
use fs4::{FileExt, TryLockError};

use nix::errno::Errno;
use nix::fcntl::{OFlag, openat};
use nix::sys::stat::{Mode, SFlag, fchmod, fstat, mkdirat};
use nix::unistd::geteuid;

use std::ffi::{CStr, CString};
use std::fs::File;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub(crate) fn lock_source_file(root: &File, timeout: Duration) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        match FileExt::try_lock(root) {
            Ok(()) => return Ok(()),
            Err(TryLockError::WouldBlock) => {
                let now = Instant::now();
                if now >= deadline {
                    anyhow::bail!(
                        "timed out waiting {timeout:?} for the source auth directory lock"
                    );
                }
                #[expect(
                    clippy::disallowed_methods,
                    reason = "source lock polling runs in blocking launch/prewarm/console validation workers or provisioning OS threads"
                )]
                std::thread::sleep(SOURCE_LOCK_POLL.min(deadline.saturating_duration_since(now)));
            }
            Err(TryLockError::Error(error)) => {
                return Err(error).context("locking source auth directory");
            }
        }
    }
}

pub(crate) fn lock_source_dir_with_timeout(
    path: &Path,
    timeout: Duration,
) -> anyhow::Result<Option<LockedSource>> {
    let (parent, name, normalized) = match open_parent(path, false) {
        Ok(value) => value,
        Err(error) if error.downcast_ref::<Errno>() == Some(&Errno::ENOENT) => return Ok(None),
        Err(error) => return Err(error),
    };
    let Some(expected) = entry_stat(&parent, &name)? else {
        return Ok(None);
    };
    validate_owned_stat(
        &expected,
        &format!("source auth directory {}", normalized.display()),
        SFlag::S_IFDIR,
    )?;
    let fd = match openat(
        &parent,
        name.as_c_str(),
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(Errno::ENOENT) => return Ok(None),
        Err(error) => {
            return Err(nix_error(
                error,
                &format!("opening source auth directory {}", normalized.display()),
            ));
        }
    };
    let root = owned_fd(fd);
    validate_directory(&root, "source auth directory", false)?;
    let stat = fstat(&root).map_err(|error| nix_error(error, "statting source auth directory"))?;
    anyhow::ensure!(
        stat.st_uid == geteuid().as_raw(),
        "source auth directory is not owned by the current user"
    );
    ensure_same_source_identity(
        &expected,
        &stat,
        &format!("source auth directory {}", normalized.display()),
    )?;
    lock_source_file(&root, timeout)?;
    Ok(Some(LockedSource { root }))
}

pub(crate) fn lock_source_dir(path: &Path) -> anyhow::Result<Option<LockedSource>> {
    lock_source_dir_with_timeout(path, SOURCE_LOCK_TIMEOUT)
}

#[cfg(test)]
pub(crate) fn lock_source_dir_for_test(
    path: &Path,
    timeout: Duration,
) -> anyhow::Result<Option<LockedSource>> {
    lock_source_dir_with_timeout(path, timeout)
}

pub(crate) fn new_stage(parent: &File, key: &str) -> anyhow::Result<(CString, File)> {
    for _ in 0..128 {
        let sequence = AUTH_DIRECTORY_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = CString::new(format!(
            ".jackin-auth-stage-{key}-{}-{sequence}",
            std::process::id()
        ))?;
        match mkdirat(parent, name.as_c_str(), Mode::from_bits_truncate(0o700)) {
            Ok(()) => {
                let directory = open_directory_at(parent, &name, "new auth stage")?;
                validate_directory(&directory, "new auth stage", true)?;
                return Ok((name, directory));
            }
            Err(Errno::EEXIST) => {}
            Err(error) => return Err(nix_error(error, "creating auth stage")),
        }
    }
    anyhow::bail!("could not allocate a unique auth stage")
}

pub(crate) fn new_previous(parent: &File, key: &str) -> anyhow::Result<CString> {
    for _ in 0..128 {
        let sequence = AUTH_DIRECTORY_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = CString::new(format!(
            ".jackin-auth-previous-{key}-{}-{sequence}",
            std::process::id()
        ))?;
        if entry_stat(parent, &name)?.is_none() {
            return Ok(name);
        }
    }
    anyhow::bail!("could not allocate a unique auth previous directory")
}

pub(crate) fn new_journal_temporary(parent: &File, key: &str) -> anyhow::Result<CString> {
    for _ in 0..128 {
        let sequence = AUTH_DIRECTORY_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = CString::new(format!(
            ".jackin-auth-journal-{key}-tmp-{}-{sequence}",
            std::process::id()
        ))?;
        if entry_stat(parent, &name)?.is_none() {
            return Ok(name);
        }
    }
    anyhow::bail!("could not allocate a unique temporary auth journal")
}

pub(crate) fn open_lock(parent: &File, key: &str) -> anyhow::Result<(CString, Arc<File>)> {
    let name = CString::new(format!(".jackin-auth-lock-{key}"))?;
    let mut file = None;
    for _ in 0..128 {
        match open_private_file(
            parent,
            &name,
            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NONBLOCK,
            Mode::from_bits_truncate(0o600),
            "opening auth target lock",
        ) {
            Ok(candidate) => {
                file = Some(candidate);
                break;
            }
            Err(error)
                if error
                    .chain()
                    .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
            {
                std::thread::yield_now();
            }
            Err(error) => return Err(error),
        }
    }
    let file =
        file.ok_or_else(|| anyhow::anyhow!("auth target lock parent disappeared during creation"))?;
    fchmod(&file, Mode::from_bits_truncate(0o600))
        .map_err(|error| nix_error(error, "restricting auth target lock"))?;
    FileExt::lock(&file).with_context(|| "locking auth target")?;
    Ok((name, Arc::new(file)))
}

pub(crate) fn target_lock(path: &Path, create_parent: bool) -> anyhow::Result<TargetLock> {
    let (parent, target, normalized) = open_parent(path, create_parent)?;
    let key = path_key(&normalized)?;
    let (_lock_name, lock) = open_lock(&parent, &key)?;
    let journal = CString::new(format!(".jackin-auth-journal-{key}"))?;
    let target_lock = TargetLock {
        parent,
        target,
        key,
        journal,
        _lock: lock,
    };
    recover(&target_lock)?;
    Ok(target_lock)
}

pub(crate) fn new_private_file_temporary(parent: &File) -> anyhow::Result<CString> {
    for _ in 0..128 {
        let sequence = AUTH_DIRECTORY_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = CString::new(format!(
            ".jackin-auth-file-{}-{sequence}",
            std::process::id()
        ))?;
        if entry_stat(parent, &name)?.is_none() {
            return Ok(name);
        }
    }
    anyhow::bail!("could not allocate a unique private auth file")
}

pub(crate) fn file_target_lock(
    path: &Path,
) -> anyhow::Result<(File, CString, PathBuf, AuthMountLease)> {
    let (parent, target, normalized) = open_parent(path, false)?;
    let key = path_key(&normalized)?;
    let (_lock_name, lock) = open_lock(&parent, &key)?;
    Ok((parent, target, normalized, AuthMountLease { _lock: lock }))
}

pub(crate) fn entry_file_at(
    parent: &File,
    target: &CStr,
    normalized: &Path,
    label: &str,
) -> anyhow::Result<Option<File>> {
    let Some(expected) = entry_stat(parent, target)? else {
        return Ok(None);
    };
    validate_owned_stat(&expected, label, SFlag::S_IFREG)?;
    let file = owned_fd(
        openat(
            parent,
            target,
            OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| nix_error(error, label))?,
    );
    let actual = fstat(&file).map_err(|error| nix_error(error, label))?;
    validate_owned_stat(&actual, label, SFlag::S_IFREG)?;
    ensure_same_source_identity(
        &expected,
        &actual,
        &format!("{label} {}", normalized.display()),
    )?;
    Ok(Some(file))
}

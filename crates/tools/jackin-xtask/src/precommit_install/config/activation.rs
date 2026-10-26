// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Durable replacement of the selected local Git config.

use std::{
    fmt,
    fs::File,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use jackin_process_directory::CheckedSourceRoot;
use nix::{
    errno::Errno,
    fcntl::{AtFlags, OFlag, open, openat, renameat},
    sys::stat::{Mode, fstat, fstatat},
    unistd::fsync,
};

#[derive(Debug)]
pub(super) struct ActivationAfterRenameError(String);

impl fmt::Display for ActivationAfterRenameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Git config rename completed, but post-rename validation or directory sync failed: {}",
            self.0
        )
    }
}

impl std::error::Error for ActivationAfterRenameError {}

pub(super) fn activation_failed_after_rename(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ActivationAfterRenameError>().is_some()
}

struct ConfigActivationEntry<'a> {
    parent: &'a File,
    lock_name: &'a std::ffi::OsStr,
    config_name: &'a std::ffi::OsStr,
    lock: &'a File,
    destination: Option<&'a File>,
}

pub(super) fn validate_hook_config_custody(common_path: &Path, selected_path: &Path) -> Result<()> {
    validate_hook_config_source_custody(common_path, selected_path)?;
    validate_config_leaf(&append_lock_suffix(selected_path)?, true)
}

pub(super) fn validate_hook_config_source_custody(
    common_path: &Path,
    selected_path: &Path,
) -> Result<()> {
    ensure!(
        common_path.file_name().is_some_and(|name| name == "config"),
        "common Git config path has an unexpected leaf"
    );
    ensure!(
        selected_path == common_path
            || selected_path
                .file_name()
                .is_some_and(|name| name == "config.worktree"),
        "selected Git config path has an unexpected leaf"
    );
    validate_config_leaf(common_path, false)?;
    if selected_path != common_path {
        validate_config_leaf(selected_path, false)?;
    }
    Ok(())
}

pub(super) fn activate_hook_config(common_path: &Path, selected_path: &Path) -> Result<()> {
    validate_hook_config_custody(common_path, selected_path)?;
    let config = selected_path
        .file_name()
        .context("selected Git config path has no leaf")?;
    let lock_path = append_lock_suffix(selected_path)?;
    let lock_name = lock_path
        .file_name()
        .context("Git config lock path has no leaf")?;
    let parent_path = selected_path
        .parent()
        .context("selected Git config path has no parent")?;
    let checked_parent = CheckedSourceRoot::from_absolute_directory(parent_path)
        .context("validating Git config parent before activation")?;
    let parent = open_config_parent(parent_path)?;
    let checked_lock = checked_parent
        .open_file(Path::new(lock_name))
        .context("holding checked Git config lock before activation")?;
    let held_lock = open_config_file(&parent, lock_name)?;
    ensure_same_file(&held_lock, checked_lock.file())?;
    checked_lock
        .recheck()
        .context("rechecking Git config lock before activation")?;

    let checked_config = match checked_parent.open_file(Path::new(config)) {
        Ok(checked_config) => {
            let held_config = open_config_file(&parent, config)?;
            ensure_same_file(&held_config, checked_config.file())?;
            checked_config
                .recheck()
                .context("rechecking existing Git config before activation")?;
            Some((checked_config, held_config))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).context("validating existing Git config before activation");
        }
    };

    let activation = ConfigActivationEntry {
        parent: &parent,
        lock_name,
        config_name: config,
        lock: &held_lock,
        destination: checked_config.as_ref().map(|(_, file)| file),
    };
    activate_config_entry(
        &activation,
        || {
            checked_parent.recheck()?;
            checked_lock.recheck()?;
            if let Some((checked_config, _)) = &checked_config {
                checked_config.recheck()?;
            }
            Ok(())
        },
        |file| {
            file.sync_all()?;
            Ok(())
        },
        |parent, lock, config| {
            renameat(parent, lock, parent, config)?;
            Ok(())
        },
        |parent| {
            fsync(parent)?;
            Ok(())
        },
    )?;
    Ok(())
}

fn activate_config_entry(
    entry: &ConfigActivationEntry<'_>,
    check_before_rename: impl FnOnce() -> Result<()>,
    sync_lock: impl FnOnce(&File) -> Result<()>,
    rename_entry: impl FnOnce(&File, &std::ffi::OsStr, &std::ffi::OsStr) -> Result<()>,
    sync_parent: impl FnOnce(&File) -> Result<()>,
) -> Result<()> {
    ensure_named_identity(entry.parent, entry.lock_name, entry.lock)?;
    sync_lock(entry.lock).context("syncing completed Git config lock before activation")?;
    ensure_named_identity(entry.parent, entry.lock_name, entry.lock)?;
    check_before_rename().context("rechecking Git config custody before activation rename")?;
    // The Git lock serializes compliant writers; direct writers can still race the final check and rename.
    match entry.destination {
        Some(destination) => ensure_named_identity(entry.parent, entry.config_name, destination)?,
        None => ensure_name_absent(entry.parent, entry.config_name)?,
    }
    rename_entry(entry.parent, entry.lock_name, entry.config_name)
        .context("atomically activating checked Git config lock")?;
    if let Err(error) = ensure_named_identity(entry.parent, entry.config_name, entry.lock) {
        return Err(anyhow::Error::new(ActivationAfterRenameError(format!(
            "checking activated Git config identity: {error:#}"
        ))));
    }
    if let Err(error) = sync_parent(entry.parent) {
        return Err(anyhow::Error::new(ActivationAfterRenameError(format!(
            "syncing parent directory: {error:#}"
        ))));
    }
    Ok(())
}

fn ensure_name_absent(parent: &File, name: &std::ffi::OsStr) -> Result<()> {
    match fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Err(Errno::ENOENT) => Ok(()),
        Ok(_) => anyhow::bail!("Git config destination appeared before activation rename"),
        Err(error) => Err(error).context("checking absent Git config destination"),
    }
}

fn open_config_parent(path: &Path) -> Result<File> {
    let descriptor = open(
        path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;
    Ok(File::from(descriptor))
}

fn open_config_file(parent: &File, name: &std::ffi::OsStr) -> Result<File> {
    let descriptor = openat(
        parent,
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    )?;
    let file = File::from(descriptor);
    ensure_named_identity(parent, name, &file)?;
    Ok(file)
}

fn ensure_same_file(actual: &File, expected: &File) -> Result<()> {
    let actual = actual.metadata()?;
    let expected = expected.metadata()?;
    ensure!(
        actual.dev() == expected.dev()
            && actual.ino() == expected.ino()
            && actual.uid() == expected.uid()
            && actual.nlink() == expected.nlink()
            && actual.mode() == expected.mode()
            && actual.len() == expected.len(),
        "Git config entry changed while acquiring its held descriptor"
    );
    Ok(())
}

fn ensure_named_identity(parent: &File, name: &std::ffi::OsStr, file: &File) -> Result<()> {
    let named = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)?;
    let opened = fstat(file)?;
    ensure!(
        named.st_dev == opened.st_dev
            && named.st_ino == opened.st_ino
            && named.st_uid == opened.st_uid
            && named.st_nlink == opened.st_nlink,
        "Git config entry name no longer identifies its held file"
    );
    Ok(())
}

fn append_lock_suffix(path: &Path) -> Result<PathBuf> {
    let mut name = path
        .file_name()
        .context("Git config path has no leaf name")?
        .to_os_string();
    name.push(".lock");
    Ok(path.with_file_name(name))
}

fn validate_config_leaf(path: &Path, required: bool) -> Result<()> {
    let parent_path = path
        .parent()
        .context("Git config leaf has no parent directory")?;
    let parent = CheckedSourceRoot::from_absolute_directory(parent_path)
        .context("validating Git config parent ownership, mode, and ACL")?;
    let name = path
        .file_name()
        .context("Git config path has no leaf name")?;
    match parent.open_file(Path::new(name)) {
        Ok(config) => config
            .recheck()
            .context("rechecking Git config leaf custody")?,
        Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(());
        }
        Err(error) => return Err(error).context("validating existing Git config leaf custody"),
    }
    Ok(())
}

#[cfg(test)]
mod tests;

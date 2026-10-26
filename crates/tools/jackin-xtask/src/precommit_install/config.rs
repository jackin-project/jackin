// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

mod activation;
mod signals;

use std::{
    ffi::OsStr,
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use jackin_process_directory::CheckedSourceRoot;
use nix::{
    fcntl::{AtFlags, OFlag, open, openat},
    sys::stat::{Mode, fchmod, fstat, fstatat},
    unistd::{UnlinkatFlags, fsync, unlinkat},
};

const CONFIG_LIMIT: u64 = 16 * 1024 * 1024;

pub(super) fn activation_failed_after_rename(error: &anyhow::Error) -> bool {
    activation::activation_failed_after_rename(error)
}

pub(super) fn after_rename_error(error: anyhow::Error) -> anyhow::Error {
    if activation_failed_after_rename(&error) {
        return error;
    }
    let marker = activation::ActivationAfterRenameError::from(&error);
    error.context(marker)
}

pub(super) fn activate(
    common_config: &Path,
    selected_config: &Path,
    owner: &super::files::OwnerInstallation,
) -> Result<()> {
    owner.recheck()?;
    ensure!(
        selected_config
            .file_name()
            .is_some_and(|name| name == "config.worktree"),
        "worktree-specific Git config is required"
    );
    let parent = selected_config
        .parent()
        .context("selected worktree config has no parent")?;
    let lock_path = lock_path(selected_config)?;
    ensure!(
        !lock_path.exists(),
        "Git worktree config is locked by another process"
    );
    let checked_parent = CheckedSourceRoot::from_absolute_directory(parent)
        .context("validating Git worktree config directory")?;
    checked_parent.recheck()?;

    activation::validate_hook_config_source_custody(common_config, selected_config)
        .context("validating Git config and lock custody before preparing candidate")?;
    let (source_config, original) = read_selected_config(&checked_parent, selected_config)?;
    signals::with_install_signals(|signals| {
        let mut candidate = tempfile::NamedTempFile::new_in(parent)
            .context("creating private Git config candidate")?;
        fchmod(candidate.as_file(), Mode::from_bits_truncate(0o600))
            .context("setting newly created Git config candidate permissions")?;
        if let Some(bytes) = original.as_deref() {
            candidate.write_all(bytes)?;
            candidate.as_file().sync_all()?;
        }
        let candidate_path = candidate.path().to_path_buf();
        update_candidate(&candidate_path, owner.owner_path())?;
        validate_candidate(&candidate_path)?;
        if let Some(config) = source_config.as_ref() {
            config
                .recheck()
                .context("rechecking source Git config after candidate preparation")?;
        }
        checked_parent
            .recheck()
            .context("rechecking Git config directory after candidate preparation")?;
        let bytes = read_candidate(&candidate_path)?;
        signals.enter_critical_phase()?;

        let mut lock = ConfigLock::create(
            lock_path.clone(),
            open(
                &lock_path,
                OFlag::O_RDWR
                    | OFlag::O_CREAT
                    | OFlag::O_EXCL
                    | OFlag::O_NOFOLLOW
                    | OFlag::O_CLOEXEC,
                Mode::from_bits_truncate(0o600),
            )
            .context("creating exclusive Git worktree config lock")?,
        );
        fchmod(lock.file(), Mode::from_bits_truncate(0o600))
            .context("setting newly created Git worktree config lock permissions")?;
        signals.check()?;
        checked_parent
            .recheck()
            .context("rechecking Git config directory after acquiring lock")?;
        let (current_config, current_bytes) =
            read_selected_config(&checked_parent, selected_config)?;
        ensure_config_unchanged(
            source_config.as_ref(),
            original.as_deref(),
            current_config.as_ref(),
            current_bytes.as_deref(),
        )?;
        checked_parent
            .recheck()
            .context("rechecking Git config directory before activation")?;
        signals.check()?;

        let result = (|| {
            write_held_lock(lock.file(), &lock_path, &bytes)?;
            activation::activate_hook_config(common_config, selected_config, owner)
                .context("activating native worktree hook configuration")?;
            Ok(())
        })();

        if let Err(error) = result {
            if activation::activation_failed_after_rename(&error) {
                lock.disarm();
                return Err(error)
                    .context("hook config was renamed, but final durability checks failed");
            }
            return match lock.remove_if_owned() {
                Ok(()) => Err(error),
                Err(cleanup) => {
                    Err(error.context(format!("Git config lock cleanup also failed: {cleanup:#}")))
                }
            };
        }
        lock.disarm();
        Ok(())
    })
}

struct ConfigLock {
    path: PathBuf,
    file: File,
    armed: bool,
}

impl ConfigLock {
    fn create(path: PathBuf, file: impl Into<File>) -> Self {
        Self {
            path,
            file: file.into(),
            armed: true,
        }
    }

    fn file(&self) -> &File {
        &self.file
    }

    fn remove_if_owned(&mut self) -> Result<()> {
        if self.armed {
            remove_lock_if_same(&self.path, &self.file)?;
            self.armed = false;
        }
        Ok(())
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ConfigLock {
    fn drop(&mut self) {
        if self.armed {
            drop(remove_lock_if_same(&self.path, &self.file));
        }
    }
}

fn ensure_config_unchanged(
    expected_file: Option<&jackin_process_directory::CheckedSourceFile>,
    expected_bytes: Option<&[u8]>,
    current_file: Option<&jackin_process_directory::CheckedSourceFile>,
    current_bytes: Option<&[u8]>,
) -> Result<()> {
    match (expected_file, expected_bytes, current_file, current_bytes) {
        (None, None, None, None) => Ok(()),
        (Some(expected_file), Some(expected_bytes), Some(current_file), Some(current_bytes)) => {
            let expected = expected_file.file().metadata()?;
            let current = current_file.file().metadata()?;
            ensure!(
                expected.dev() == current.dev()
                    && expected.ino() == current.ino()
                    && expected.uid() == current.uid()
                    && expected.nlink() == current.nlink()
                    && expected.mode() == current.mode()
                    && expected_bytes == current_bytes,
                "Git worktree config changed while hook configuration was prepared"
            );
            Ok(())
        }
        _ => anyhow::bail!(
            "Git worktree config presence changed while hook configuration was prepared"
        ),
    }
}

fn update_candidate(candidate: &Path, owner: &Path) -> Result<()> {
    let mut owner_command = super::git::shell_quote_path(owner)?;
    owner_command.push(" run-snapshot");
    for key in ["hook.hk-pre-commit.command", "hook.hk-pre-commit.event"] {
        super::git::config_unset_all(candidate, key)?;
    }
    for (key, value) in [
        ("hook.hk-pre-commit.enabled", OsStr::new("false")),
        ("hook.jackin-pre-commit.command", owner_command.as_os_str()),
        ("hook.jackin-pre-commit.event", OsStr::new("pre-commit")),
        ("hook.jackin-pre-commit.enabled", OsStr::new("true")),
        ("hook.hk-pre-push.enabled", OsStr::new("false")),
    ] {
        super::git::config_set(candidate, key, value)?;
    }
    for key in ["hook.hk-pre-push.command", "hook.hk-pre-push.event"] {
        super::git::config_unset_all(candidate, key)?;
    }
    for (key, value) in [
        (
            "hook.jackin-pre-push.command",
            OsStr::new("sh scripts/pre-push-security"),
        ),
        ("hook.jackin-pre-push.event", OsStr::new("pre-push")),
        ("hook.jackin-pre-push.enabled", OsStr::new("true")),
    ] {
        super::git::config_set(candidate, key, value)?;
    }
    for (key, expected) in [
        ("hook.hk-pre-commit.enabled", OsStr::new("false")),
        ("hook.jackin-pre-commit.command", owner_command.as_os_str()),
        ("hook.jackin-pre-commit.event", OsStr::new("pre-commit")),
        ("hook.jackin-pre-commit.enabled", OsStr::new("true")),
        ("hook.hk-pre-push.enabled", OsStr::new("false")),
        (
            "hook.jackin-pre-push.command",
            OsStr::new("sh scripts/pre-push-security"),
        ),
        ("hook.jackin-pre-push.event", OsStr::new("pre-push")),
        ("hook.jackin-pre-push.enabled", OsStr::new("true")),
    ] {
        ensure!(
            super::git::config_get(candidate, key)?.as_os_str() == expected,
            "staged worktree config has an invalid {key}"
        );
    }
    Ok(())
}

fn validate_candidate(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == nix::unistd::geteuid().as_raw()
            && metadata.nlink() == 1
            && metadata.len() <= CONFIG_LIMIT
            && metadata.permissions().mode() & 0o777 == 0o600,
        "Git config candidate has unsafe custody or exceeds its bound"
    );
    Ok(())
}

fn read_candidate(path: &Path) -> Result<Vec<u8>> {
    validate_candidate(path)?;
    let mut file = File::from(open(
        path,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    )?);
    let opened = file.metadata()?;
    ensure!(
        opened.is_file(),
        "Git config candidate is not a regular file"
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(CONFIG_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    let named = fs::symlink_metadata(path)?;
    ensure!(
        bytes.len() as u64 <= CONFIG_LIMIT
            && opened.dev() == named.dev()
            && opened.ino() == named.ino()
            && opened.len() == named.len()
            && opened.len() == bytes.len() as u64,
        "Git config candidate changed while reading"
    );
    Ok(bytes)
}

fn read_selected_config(
    parent: &CheckedSourceRoot,
    path: &Path,
) -> Result<(
    Option<jackin_process_directory::CheckedSourceFile>,
    Option<Vec<u8>>,
)> {
    let name = path
        .file_name()
        .context("selected Git config has no leaf")?;
    let checked = match parent.open_file(Path::new(name)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((None, None)),
        Err(error) => {
            return Err(error).context("opening selected Git config without following links");
        }
    };
    let metadata = checked.file().metadata()?;
    ensure!(
        metadata.len() <= CONFIG_LIMIT,
        "Git worktree config exceeds its bound"
    );
    let mut bytes = Vec::new();
    checked
        .file()
        .take(CONFIG_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == metadata.len(),
        "Git worktree config changed while reading"
    );
    checked
        .recheck()
        .context("rechecking selected Git config after reading")?;
    Ok((Some(checked), Some(bytes)))
}

fn write_held_lock(lock: &File, path: &Path, bytes: &[u8]) -> Result<()> {
    let opened = lock.metadata()?;
    ensure!(opened.len() == 0, "Git worktree config lock is not empty");
    let named = fs::symlink_metadata(path)?;
    ensure!(
        named.is_file()
            && !named.file_type().is_symlink()
            && named.uid() == opened.uid()
            && named.dev() == opened.dev()
            && named.ino() == opened.ino()
            && named.nlink() == 1
            && opened.permissions().mode() & 0o777 == 0o600,
        "Git worktree config lock changed after creation"
    );
    let mut lock = lock;
    lock.write_all(bytes)?;
    lock.sync_all()
        .context("syncing completed Git worktree config lock")?;
    ensure!(
        lock.metadata()?.len() == bytes.len() as u64,
        "Git config lock write was incomplete"
    );
    Ok(())
}

fn remove_lock_if_same(path: &Path, lock: &File) -> Result<()> {
    let opened = lock.metadata()?;
    let parent_path = path.parent().context("Git config lock has no parent")?;
    let leaf = path.file_name().context("Git config lock has no name")?;
    let checked_parent = CheckedSourceRoot::from_absolute_directory(parent_path)
        .context("checking Git config parent before lock cleanup")?;
    let parent = File::from(open(
        parent_path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?);
    checked_parent.recheck()?;
    let cleanup = match openat(
        &parent,
        leaf,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    ) {
        Ok(descriptor) => File::from(descriptor),
        Err(nix::errno::Errno::ENOENT) => return Ok(()),
        Err(error) => return Err(error).context("opening Git config lock for cleanup"),
    };
    if cleanup.metadata()?.dev() == opened.dev() && cleanup.metadata()?.ino() == opened.ino() {
        let cleanup_stat = fstat(&cleanup)?;
        let named = fstatat(&parent, leaf, AtFlags::AT_SYMLINK_NOFOLLOW)?;
        ensure!(
            named.st_dev == cleanup_stat.st_dev && named.st_ino == cleanup_stat.st_ino,
            "Git config lock name changed during cleanup"
        );
        unlinkat(&parent, leaf, UnlinkatFlags::NoRemoveDir)
            .context("removing unactivated Git config lock")?;
        fsync(&parent).context("syncing Git config lock cleanup")?;
    }
    Ok(())
}

fn lock_path(config: &Path) -> Result<PathBuf> {
    let name = config.file_name().context("Git config path has no leaf")?;
    let mut lock_name = name.to_os_string();
    lock_name.push(".lock");
    Ok(config.with_file_name(lock_name))
}

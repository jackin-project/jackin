// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Descriptor-relative operations for the private native hook installation.

use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
};

use anyhow::{Context, Result, ensure};
use jackin_process_directory::{CheckedInstallRoot, CheckedSourceRoot};
use nix::{
    errno::Errno,
    fcntl::{AtFlags, OFlag, open, openat, renameat},
    sys::stat::{Mode, fchmod, fstatat, mkdirat},
    unistd::{UnlinkatFlags, fsync, unlinkat},
};

pub(super) struct InstallDirectories {
    pub(super) root: File,
    pub(super) generations: File,
    pub(super) pending: File,
}

impl InstallDirectories {
    pub(super) fn open(common_git_dir: &Path) -> Result<Self> {
        let common = open_common_git_dir(common_git_dir)?;
        let root = ensure_directory(&common, "jackin-precommit-owner", 0o700)?;
        let generations = ensure_directory(&root, "generations", 0o700)?;
        let pending = ensure_directory(&root, "pending", 0o700)?;
        validate_acl_directory(&root, "owner root")?;
        validate_acl_directory(&generations, "generations directory")?;
        validate_acl_directory(&pending, "pending directory")?;
        Ok(Self {
            root,
            generations,
            pending,
        })
    }
}

pub(super) fn open_directory(parent: &File, name: &str, mode: u32) -> Result<File> {
    validate_component(name)?;
    let descriptor = openat(
        parent,
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;
    let directory = File::from(descriptor);
    validate_directory(&directory, mode)?;
    validate_named_identity(parent, name, &directory)?;
    match mode {
        0o700 => validate_acl_directory(&directory, "private hook directory")?,
        0o500 => validate_acl_sealed_directory(&directory)?,
        _ => {}
    }
    Ok(directory)
}

fn validate_acl_directory(directory: &File, label: &str) -> Result<()> {
    CheckedInstallRoot::from_pinned_directory(directory.try_clone()?)
        .with_context(|| format!("validating native ACLs on {label}"))?;
    Ok(())
}

pub(super) fn validate_acl_sealed_directory(directory: &File) -> Result<()> {
    CheckedInstallRoot::validate_pinned_sealed_directory(directory)
        .context("validating native ACLs on sealed generation directory")?;
    Ok(())
}

pub(super) fn validate_hook_config_custody(common_path: &Path, selected_path: &Path) -> Result<()> {
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
    validate_config_leaf(&append_lock_suffix(selected_path)?, true)
}

fn append_lock_suffix(path: &Path) -> Result<std::path::PathBuf> {
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

pub(super) fn ensure_directory(parent: &File, name: &str, mode: u32) -> Result<File> {
    validate_component(name)?;
    let created = match mkdirat(parent, name, native_mode(mode)?) {
        Ok(()) => true,
        Err(Errno::EEXIST) => false,
        Err(error) => return Err(error).context("creating private hook directory"),
    };
    let directory = open_directory(parent, name, mode)?;
    if created {
        fsync(parent)?;
    }
    Ok(directory)
}

pub(super) fn create_directory(parent: &File, name: &str, mode: u32) -> Result<File> {
    validate_component(name)?;
    mkdirat(parent, name, native_mode(mode)?)?;
    let directory = open_directory(parent, name, mode)?;
    fsync(parent)?;
    Ok(directory)
}

pub(super) fn open_lock(root: &File) -> Result<File> {
    let flags = OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let descriptor = openat(
        root,
        "provision.lock",
        flags,
        Mode::from_bits_truncate(0o600),
    )?;
    let file = File::from(descriptor);
    validate_regular(&file, 0o600, 0)?;
    validate_named_identity(root, "provision.lock", &file)?;
    Ok(file)
}

pub(super) fn create_file(directory: &File, name: &str, bytes: &[u8], mode: u32) -> Result<()> {
    validate_component(name)?;
    let flags =
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let descriptor = openat(directory, name, flags, native_mode(mode)?)?;
    let mut file = File::from(descriptor);
    let result = (|| {
        fchmod(&file, native_mode(mode)?)?;
        use std::io::Write;
        file.write_all(bytes)?;
        file.sync_all()?;
        validate_regular(&file, mode, bytes.len() as u64)?;
        validate_named_identity(directory, name, &file)?;
        Ok(())
    })();
    if let Err(error) = result {
        return match unlinkat(directory, name, UnlinkatFlags::NoRemoveDir) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(error.context(format!(
                "removing incomplete native hook file {name} also failed: {cleanup}"
            ))),
        };
    }
    Ok(())
}

pub(super) fn read_file(directory: &File, name: &str, limit: u64, mode: u32) -> Result<Vec<u8>> {
    validate_component(name)?;
    let descriptor = openat(
        directory,
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    )?;
    let mut file = File::from(descriptor);
    validate_regular(&file, mode, limit)?;
    let before = file.metadata()?;
    validate_named_identity(directory, name, &file)?;
    let mut bytes = Vec::new();
    file.by_ref().take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "native hook record exceeds its bound"
    );
    let after = file.metadata()?;
    ensure!(
        before.dev() == after.dev()
            && before.ino() == after.ino()
            && before.len() == after.len()
            && after.uid() == nix::unistd::geteuid().as_raw()
            && after.nlink() == 1
            && after.permissions().mode() & 0o777 == mode,
        "native hook record changed while reading"
    );
    validate_named_identity(directory, name, &file)?;
    Ok(bytes)
}

pub(super) fn rename(directory: &File, old_name: &str, new_name: &str) -> Result<()> {
    validate_component(old_name)?;
    validate_component(new_name)?;
    renameat(directory, old_name, directory, new_name)?;
    Ok(())
}

pub(super) fn remove_file(directory: &File, name: &str) -> Result<()> {
    validate_component(name)?;
    unlinkat(directory, name, UnlinkatFlags::NoRemoveDir)?;
    Ok(())
}

pub(super) fn remove_directory(directory: &File, name: &str) -> Result<()> {
    validate_component(name)?;
    unlinkat(directory, name, UnlinkatFlags::RemoveDir)?;
    Ok(())
}

pub(super) fn sync(directory: &File) -> Result<()> {
    fsync(directory)?;
    Ok(())
}

pub(super) fn exists(directory: &File, name: &str) -> Result<bool> {
    validate_component(name)?;
    match fstatat(directory, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(_) => Ok(true),
        Err(Errno::ENOENT) => Ok(false),
        Err(error) => Err(error).context("checking native hook directory entry"),
    }
}

fn open_common_git_dir(path: &Path) -> Result<File> {
    let canonical = fs::canonicalize(path).context("resolving Git common directory")?;
    ensure!(canonical == path, "Git common directory is not physical");
    let before = fs::symlink_metadata(path)?;
    ensure!(
        before.is_dir() && !before.file_type().is_symlink(),
        "Git common path is not a directory"
    );
    let directory = open_path(path)?;
    let opened = directory.metadata()?;
    let checked = CheckedSourceRoot::from_absolute_directory(path)
        .context("validating Git common directory ancestors and ACLs")?;
    checked.recheck()?;
    let checked_metadata = fs::metadata(&canonical)?;
    checked.recheck()?;
    ensure!(
        opened.dev() == before.dev()
            && opened.ino() == before.ino()
            && checked_metadata.dev() == opened.dev()
            && checked_metadata.ino() == opened.ino()
            && opened.uid() == nix::unistd::geteuid().as_raw()
            && opened.permissions().mode() & 0o022 == 0,
        "Git common directory has unsafe custody"
    );
    Ok(directory)
}

pub(super) fn open_path(path: &Path) -> Result<File> {
    let descriptor = open(
        path,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;
    Ok(File::from(descriptor))
}

fn validate_directory(directory: &File, mode: u32) -> Result<()> {
    let metadata = directory.metadata()?;
    ensure!(
        metadata.is_dir()
            && metadata.uid() == nix::unistd::geteuid().as_raw()
            && metadata.permissions().mode() & 0o777 == mode,
        "native hook directory has unsafe owner, type, or mode"
    );
    Ok(())
}

fn validate_regular(file: &File, mode: u32, limit: u64) -> Result<()> {
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.uid() == nix::unistd::geteuid().as_raw()
            && metadata.nlink() == 1
            && metadata.len() <= limit
            && metadata.permissions().mode() & 0o777 == mode,
        "native hook file has unsafe owner, type, mode, or link count"
    );
    Ok(())
}

fn validate_named_identity(parent: &File, name: &str, file: &File) -> Result<()> {
    let named = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)?;
    let opened = file.metadata()?;
    ensure!(
        u64::try_from(named.st_dev)? == opened.dev()
            && named.st_ino == opened.ino()
            && named.st_uid == nix::unistd::geteuid().as_raw(),
        "native hook file or directory name changed"
    );
    Ok(())
}

fn native_mode(mode: u32) -> Result<Mode> {
    Ok(Mode::from_bits_truncate(
        mode.try_into()
            .context("mode is outside the platform range")?,
    ))
}

fn validate_component(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains('/')
            && !name.contains('\0'),
        "native hook path is not one component"
    );
    Ok(())
}

#[cfg(test)]
mod tests;

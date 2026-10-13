// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Locked source reads and bounded tree copy.

use super::{
    AUTH_DIRECTORY_SWAP_COUNTER, SnapshotDirectory, entry_stat, fsync_directory, ignore_eexist,
    lock_source_dir, nix_error, open_directory_at, open_parent, open_private_file,
    open_source_directory_at, open_source_directory_at_with_hook, open_source_file,
    open_source_file_with_hook, validate_directory, validate_owned_stat,
};

use crate::auth::{
    MAX_AUTH_SOURCE_FILE_BYTES, MAX_AUTH_SOURCE_TREE_BYTES, MAX_AUTH_SOURCE_TREE_ENTRIES,
};
use anyhow::Context;

use nix::dir::Dir;
use nix::errno::Errno;
use nix::fcntl::{OFlag, renameat};
use nix::sys::stat::{FileStat, Mode, SFlag, fchmod, mkdirat};

use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::{Read, Write};

use std::path::Path;

use std::sync::atomic::Ordering;

pub(crate) fn source_entry_kind(
    source: &File,
    name: &CStr,
    label: &str,
) -> anyhow::Result<Option<FileStat>> {
    let Some(stat) = entry_stat(source, name)? else {
        return Ok(None);
    };
    let kind = SFlag::from_bits_truncate(stat.st_mode);
    if kind.contains(SFlag::S_IFLNK) {
        anyhow::bail!("{label} is a symlink; refusing to follow source auth state");
    }
    Ok(Some(stat))
}

pub(crate) fn read_source_file(
    source: &File,
    name: &CStr,
    stat: &FileStat,
    label: &str,
) -> anyhow::Result<Vec<u8>> {
    validate_owned_stat(stat, label, SFlag::S_IFREG)?;
    let file = open_source_file(source, name, stat, label)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut &file)
        .take((MAX_AUTH_SOURCE_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .with_context(|| format!("reading {label}"))?;
    anyhow::ensure!(
        bytes.len() <= MAX_AUTH_SOURCE_FILE_BYTES,
        "{label} exceeds the credential source size limit"
    );
    Ok(bytes)
}

pub(crate) fn read_locked_source_file(
    source: &File,
    components: &[&str],
    label: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    let Some((file_name_text, directory_names)) = components.split_last() else {
        anyhow::bail!("{label} has no source path components");
    };
    let mut directory = source.try_clone()?;
    for directory_name_text in directory_names {
        let directory_name = source_name(directory_name_text)?;
        let Some(stat) = source_entry_kind(&directory, &directory_name, label)? else {
            return Ok(None);
        };
        validate_owned_stat(&stat, label, SFlag::S_IFDIR)?;
        directory =
            open_source_directory_at_with_hook(&directory, &directory_name, &stat, label, false)?;
    }
    let file_name = source_name(file_name_text)?;
    let Some(stat) = source_entry_kind(&directory, &file_name, label)? else {
        return Ok(None);
    };
    validate_owned_stat(&stat, label, SFlag::S_IFREG)?;
    let file = open_source_file_with_hook(&directory, &file_name, &stat, label, true)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut &file)
        .take((MAX_AUTH_SOURCE_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .with_context(|| format!("reading {label}"))?;
    anyhow::ensure!(
        bytes.len() <= MAX_AUTH_SOURCE_FILE_BYTES,
        "{label} exceeds the credential source size limit"
    );
    Ok(Some(bytes))
}

pub(crate) fn validate_locked_source_directory(
    source: &File,
    components: &[&str],
    label: &str,
) -> anyhow::Result<bool> {
    let mut directory = source.try_clone()?;
    for component_text in components {
        let component = source_name(component_text)?;
        let Some(stat) = source_entry_kind(&directory, &component, label)? else {
            return Ok(false);
        };
        validate_owned_stat(&stat, label, SFlag::S_IFDIR)?;
        directory =
            open_source_directory_at_with_hook(&directory, &component, &stat, label, false)?;
    }
    Ok(true)
}

pub(crate) fn read_source_path(path: &Path, label: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("source path has no parent: {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("source path has no valid file name: {}", path.display()))?;
    let Some(source) = lock_source_dir(parent)? else {
        return Ok(None);
    };
    read_locked_source_file(&source.root, &[name], label)
}

pub(crate) fn write_private_file_at(
    directory: &File,
    name: &CStr,
    bytes: &[u8],
    label: &str,
) -> anyhow::Result<()> {
    let file = open_private_file(
        directory,
        name,
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL,
        Mode::from_bits_truncate(0o600),
        label,
    )?;
    fchmod(&file, Mode::from_bits_truncate(0o600))
        .map_err(|error| nix_error(error, "restricting staged auth file"))?;
    let mut file = file;
    file.write_all(bytes)
        .with_context(|| format!("writing {label}"))?;
    file.sync_all()
        .with_context(|| format!("syncing {label}"))?;
    Ok(())
}

pub(crate) fn source_name(name: &str) -> anyhow::Result<CString> {
    CString::new(name).context("auth source name contains NUL")
}

pub(crate) fn copy_optional_source_file(
    source: &File,
    source_name_text: &str,
    destination: &File,
    destination_name_text: &str,
    label: &str,
) -> anyhow::Result<()> {
    let source_entry_name = source_name(source_name_text)?;
    let Some(stat) = source_entry_kind(source, &source_entry_name, label)? else {
        return Ok(());
    };
    validate_owned_stat(&stat, label, SFlag::S_IFREG)?;
    let bytes = read_source_file(source, &source_entry_name, &stat, label)?;
    let destination_name = source_name(destination_name_text)?;
    write_private_file_at(destination, &destination_name, &bytes, label)
}

pub(crate) struct CopyBudget {
    bytes: usize,
    entries: usize,
}

pub(crate) fn copy_tree(source: &File, destination: &File, label: &str) -> anyhow::Result<()> {
    let mut budget = CopyBudget {
        bytes: 0,
        entries: 0,
    };
    copy_tree_with_budget(source, destination, label, &mut budget)
}

pub(crate) fn copy_tree_with_budget(
    source: &File,
    destination: &File,
    label: &str,
    budget: &mut CopyBudget,
) -> anyhow::Result<()> {
    validate_directory(source, label, false)?;
    fchmod(destination, Mode::from_bits_truncate(0o700))
        .map_err(|error| nix_error(error, "restricting staged auth directory"))?;
    validate_directory(destination, "staged auth directory", true)?;
    let source_clone = source.try_clone()?;
    let mut entries = Dir::from_fd(source_clone.into())
        .map_err(|error| nix_error(error, "opening source auth directory entries"))?;
    let mut names = Vec::new();
    for entry in entries.iter() {
        let entry = entry.map_err(|error| nix_error(error, "reading source auth directory"))?;
        let name = entry.file_name();
        if name.to_bytes() != b"." && name.to_bytes() != b".." {
            names.push(name.to_owned());
        }
    }

    for name in names {
        budget.entries = budget.entries.saturating_add(1);
        anyhow::ensure!(
            budget.entries <= MAX_AUTH_SOURCE_TREE_ENTRIES,
            "{label} contains too many entries"
        );
        let entry_label = format!("{label}/{}", name.to_string_lossy());
        let stat = entry_stat(source, &name)?.ok_or_else(|| {
            anyhow::anyhow!("{entry_label} disappeared during secure source snapshot")
        })?;
        let kind = SFlag::from_bits_truncate(stat.st_mode);
        if kind.contains(SFlag::S_IFLNK) {
            anyhow::bail!("{entry_label} is a symlink; refusing to sync source auth state");
        }
        if kind.contains(SFlag::S_IFDIR) {
            validate_owned_stat(&stat, &entry_label, SFlag::S_IFDIR)?;
            mkdirat(
                destination,
                name.as_c_str(),
                Mode::from_bits_truncate(0o700),
            )
            .or_else(ignore_eexist)
            .map_err(|error| nix_error(error, "creating staged auth subdirectory"))?;
            let child = open_directory_at(destination, &name, &entry_label)?;
            validate_directory(&child, &entry_label, true)?;
            let source_child = open_source_directory_at(source, &name, &stat, &entry_label)?;
            copy_tree_with_budget(&source_child, &child, &entry_label, budget)?;
            fsync_directory(&child)?;
        } else if kind.contains(SFlag::S_IFREG) {
            validate_owned_stat(&stat, &entry_label, SFlag::S_IFREG)?;
            let bytes = read_source_file(source, &name, &stat, &entry_label)?;
            budget.bytes = budget.bytes.saturating_add(bytes.len());
            anyhow::ensure!(
                budget.bytes <= MAX_AUTH_SOURCE_TREE_BYTES,
                "{label} exceeds the credential source size limit"
            );
            write_private_file_at(destination, &name, &bytes, &entry_label)?;
        } else {
            anyhow::bail!("{entry_label} is a special file; refusing to sync it");
        }
    }
    fsync_directory(destination)
}

pub(crate) fn copy_optional_source_tree(
    source: &File,
    source_name_text: &str,
    destination: &File,
    destination_name_text: &str,
    label: &str,
) -> anyhow::Result<()> {
    let source_entry_name = source_name(source_name_text)?;
    let Some(stat) = source_entry_kind(source, &source_entry_name, label)? else {
        return Ok(());
    };
    validate_owned_stat(&stat, label, SFlag::S_IFDIR)?;
    mkdirat(
        destination,
        source_entry_name.as_c_str(),
        Mode::from_bits_truncate(0o700),
    )
    .map_err(|error| nix_error(error, "creating staged auth tree"))?;
    let destination_name = source_name(destination_name_text)?;
    if destination_name != source_entry_name {
        renameat(
            destination,
            source_entry_name.as_c_str(),
            destination,
            destination_name.as_c_str(),
        )
        .map_err(|error| nix_error(error, "naming staged auth tree"))?;
    }
    let staged = open_directory_at(destination, &destination_name, label)?;
    let source_dir = open_source_directory_at(source, &source_entry_name, &stat, label)?;
    copy_tree(&source_dir, &staged, label)
}

pub(crate) fn open_directory_path(path: &Path) -> anyhow::Result<File> {
    let (parent, name, normalized) = open_parent(path, false)?;
    open_directory_at(
        &parent,
        name.as_c_str(),
        &format!("opening auth directory {}", normalized.display()),
    )
}

/// Allocate a source snapshot beneath a host-private parent. Missing
/// parent components are created descriptor-relatively with `mkdirat`;
/// parent and child creation both use no-follow checks. The returned
/// directory guard owns descriptor-relative cleanup of the unique child
/// path after workers release their snapshot clones.
pub(crate) fn create_snapshot_directory(parent_path: &Path) -> anyhow::Result<SnapshotDirectory> {
    let (parent, target, normalized) = open_parent(parent_path, true)?;
    let parent_directory = match open_directory_at(
        &parent,
        target.as_c_str(),
        &format!("opening auth snapshot parent {}", normalized.display()),
    ) {
        Ok(directory) => directory,
        Err(error)
            if error
                .chain()
                .any(|cause| cause.downcast_ref::<Errno>() == Some(&Errno::ENOENT)) =>
        {
            mkdirat(&parent, target.as_c_str(), Mode::from_bits_truncate(0o700))
                .or_else(ignore_eexist)
                .map_err(|error| nix_error(error, "creating auth snapshot parent"))?;
            open_directory_at(
                &parent,
                target.as_c_str(),
                &format!("opening auth snapshot parent {}", normalized.display()),
            )?
        }
        Err(error) => return Err(error),
    };
    validate_directory(&parent_directory, "auth snapshot parent", true)?;
    for _ in 0..128 {
        let sequence = AUTH_DIRECTORY_SWAP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name_text = format!(".jackin-auth-source-{}-{sequence}", std::process::id());
        let name = CString::new(name_text.as_str())?;
        match mkdirat(
            &parent_directory,
            name.as_c_str(),
            Mode::from_bits_truncate(0o700),
        ) {
            Ok(()) => {
                let directory =
                    open_directory_at(&parent_directory, &name, "auth source snapshot")?;
                validate_directory(&directory, "auth source snapshot", true)?;
                drop(directory);
                return Ok(SnapshotDirectory {
                    path: normalized.join(name_text),
                    parent: parent_directory,
                    name,
                });
            }
            Err(Errno::EEXIST) => {}
            Err(error) => return Err(nix_error(error, "creating auth source snapshot")),
        }
    }
    anyhow::bail!("could not allocate a unique auth source snapshot")
}

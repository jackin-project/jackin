// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Interrupted-swap recovery and directory publish.

use super::{
    JOURNAL_SCHEMA_VERSION, SwapPhase, TargetLock, TreeEntryKind, classify_tree_entry_for_removal,
    entry_stat, fsync_directory, nix_error, open_directory_at, read_journal, target_present,
    unlink_entry, validate_owned_stat,
};

use nix::dir::Dir;

use nix::fcntl::renameat;
use nix::sys::stat::SFlag;
use nix::unistd::{UnlinkatFlags, geteuid, unlinkat};

use std::ffi::{CStr, CString};
use std::fs::File;

pub(crate) fn remove_tree(parent: &File, name: &CStr, label: &str) -> anyhow::Result<()> {
    let Some(stat) = entry_stat(parent, name)? else {
        return Ok(());
    };
    validate_owned_stat(&stat, label, SFlag::S_IFDIR)?;
    let directory = open_directory_at(parent, name, label)?;
    let clone = directory.try_clone()?;
    let mut entries = Dir::from_fd(clone.into())
        .map_err(|error| nix_error(error, "opening auth cleanup directory"))?;
    let mut names = Vec::new();
    for entry in entries.iter() {
        let entry = entry.map_err(|error| nix_error(error, "reading auth cleanup directory"))?;
        let entry_name = entry.file_name();
        if entry_name.to_bytes() != b"." && entry_name.to_bytes() != b".." {
            names.push(entry_name.to_owned());
        }
    }
    for entry_name in names {
        let Some(entry_stat) = entry_stat(&directory, &entry_name)? else {
            continue;
        };
        match classify_tree_entry_for_removal(entry_stat.st_mode) {
            TreeEntryKind::Directory => {
                remove_tree(&directory, &entry_name, label)?;
            }
            TreeEntryKind::Regular => {
                validate_owned_stat(&entry_stat, label, SFlag::S_IFREG)?;
                unlink_entry(&directory, &entry_name, "removing auth file")?;
            }
            TreeEntryKind::Symlink => {
                // `lstat` mode bits on a symlink carry no access meaning
                // (Linux always reports 0777, macOS 0755), so only the
                // link's ownership is checked. `unlinkat` never follows
                // the link, so removing it cannot touch its target.
                anyhow::ensure!(
                    entry_stat.st_uid == geteuid().as_raw(),
                    "{label} is not owned by the current user"
                );
                unlink_entry(&directory, &entry_name, "removing auth symlink")?;
            }
            TreeEntryKind::Special => {
                anyhow::bail!("{label} contains a special file entry")
            }
        }
    }
    fsync_directory(&directory)?;
    unlinkat(parent, name, UnlinkatFlags::RemoveDir)
        .map_err(|error| nix_error(error, "removing auth directory"))?;
    fsync_directory(parent)
}

pub(crate) fn valid_transaction_name(name: &str, key: &str, kind: &str) -> bool {
    name.starts_with(&format!(".jackin-auth-{kind}-{key}-"))
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
}

pub(crate) fn previous_name(previous: Option<&CString>) -> anyhow::Result<&CStr> {
    previous
        .map(CString::as_c_str)
        .ok_or_else(|| anyhow::anyhow!("auth swap journal refers to a missing previous name"))
}

pub(crate) fn recover(target: &TargetLock) -> anyhow::Result<()> {
    let Some(journal) = read_journal(target)? else {
        cleanup_orphans(target)?;
        return Ok(());
    };
    anyhow::ensure!(
        journal.schema_version == JOURNAL_SCHEMA_VERSION,
        "unsupported auth swap journal schema"
    );
    anyhow::ensure!(
        journal.target == hex::encode(target.target.as_bytes()),
        "auth swap journal targets a different directory"
    );
    anyhow::ensure!(
        valid_transaction_name(&journal.stage, &target.key, "stage"),
        "auth swap journal contains an invalid stage name"
    );
    if let Some(previous) = &journal.previous {
        anyhow::ensure!(
            valid_transaction_name(previous, &target.key, "previous"),
            "auth swap journal contains an invalid previous name"
        );
    }
    let stage = CString::new(journal.stage.as_str())?;
    let previous = journal.previous.as_deref().map(CString::new).transpose()?;
    let target_exists = target_present(target)?;
    let previous_exists = previous
        .as_ref()
        .map(|name| entry_stat(&target.parent, name).map(|stat| stat.is_some()))
        .transpose()?
        .unwrap_or(false);

    match journal.phase {
        SwapPhase::Prepared => {
            if !target_exists && previous_exists {
                renameat(
                    &target.parent,
                    previous_name(previous.as_ref())?,
                    &target.parent,
                    target.target.as_c_str(),
                )
                .map_err(|error| nix_error(error, "restoring prepared auth swap"))?;
                fsync_directory(&target.parent)?;
            } else if target_exists && previous_exists {
                remove_tree(
                    &target.parent,
                    previous_name(previous.as_ref())?,
                    "stale auth previous directory",
                )?;
            }
        }
        SwapPhase::BackedUp => {
            if !target_exists && previous_exists {
                renameat(
                    &target.parent,
                    previous_name(previous.as_ref())?,
                    &target.parent,
                    target.target.as_c_str(),
                )
                .map_err(|error| nix_error(error, "restoring backed-up auth swap"))?;
                fsync_directory(&target.parent)?;
            } else if target_exists && previous_exists {
                remove_tree(
                    &target.parent,
                    previous_name(previous.as_ref())?,
                    "completed auth previous directory",
                )?;
            } else if !target_exists {
                anyhow::bail!("auth swap journal has neither destination nor previous tree");
            }
        }
        SwapPhase::Installed => {
            if !target_exists && previous_exists {
                renameat(
                    &target.parent,
                    previous_name(previous.as_ref())?,
                    &target.parent,
                    target.target.as_c_str(),
                )
                .map_err(|error| nix_error(error, "restoring lost installed auth swap"))?;
                fsync_directory(&target.parent)?;
            } else if previous_exists {
                remove_tree(
                    &target.parent,
                    previous_name(previous.as_ref())?,
                    "installed auth previous directory",
                )?;
            }
        }
    }
    remove_tree(&target.parent, &stage, "orphaned auth stage")?;
    unlink_entry(
        &target.parent,
        &target.journal,
        "removing auth swap journal",
    )?;
    fsync_directory(&target.parent)?;
    cleanup_orphans(target)
}

pub(crate) fn cleanup_orphans(target: &TargetLock) -> anyhow::Result<()> {
    let clone = target.parent.try_clone()?;
    let mut entries = Dir::from_fd(clone.into())
        .map_err(|error| nix_error(error, "opening auth parent for orphan cleanup"))?;
    // Transaction names created by this implementation carry the target
    // identity. Pre-846f984 names do not, so there is no safe way to
    // attribute those legacy trees to this target; leave them untouched.
    let new_stage_prefix = format!(".jackin-auth-stage-{}-", target.key);
    let new_previous_prefix = format!(".jackin-auth-previous-{}-", target.key);
    let new_journal_temporary_prefix = format!(".jackin-auth-journal-{}-tmp-", target.key);
    let mut directories = Vec::new();
    let mut journal_temporaries = Vec::new();
    for entry in entries.iter() {
        let entry = entry.map_err(|error| nix_error(error, "reading auth parent"))?;
        let name = entry.file_name();
        let text = name.to_string_lossy();
        if text.starts_with(&new_stage_prefix) || text.starts_with(&new_previous_prefix) {
            directories.push(name.to_owned());
        } else if text.starts_with(&new_journal_temporary_prefix) {
            journal_temporaries.push(name.to_owned());
        }
    }
    for name in directories {
        remove_tree(&target.parent, &name, "orphaned auth swap directory")?;
    }
    for name in journal_temporaries {
        let stat = entry_stat(&target.parent, &name)?.ok_or_else(|| {
            anyhow::anyhow!("temporary auth swap journal disappeared during cleanup")
        })?;
        validate_owned_stat(&stat, "temporary auth swap journal", SFlag::S_IFREG)?;
        unlink_entry(
            &target.parent,
            &name,
            "removing orphaned temporary auth swap journal",
        )?;
    }
    Ok(())
}

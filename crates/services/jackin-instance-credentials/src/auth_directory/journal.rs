// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Swap journal read/write and removal classification.

use super::{
    FailurePoint, MAX_JOURNAL_BYTES, SwapJournal, TargetLock, entry_stat, fsync_directory,
    maybe_fail, new_journal_temporary, nix_error, open_private_file, validate_owned_stat,
};

use anyhow::Context;

use nix::errno::Errno;
use nix::fcntl::{OFlag, renameat};
use nix::sys::stat::{Mode, SFlag, mode_t};
use nix::unistd::{UnlinkatFlags, unlinkat};

use std::ffi::CStr;
use std::fs::File;
use std::io::{Read, Write};

pub fn target_present(target: &TargetLock) -> anyhow::Result<bool> {
    let Some(stat) = entry_stat(&target.parent, &target.target)? else {
        return Ok(false);
    };
    validate_owned_stat(&stat, "existing auth destination", SFlag::S_IFDIR)?;
    Ok(true)
}

pub fn write_journal(target: &TargetLock, journal: &SwapJournal) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(journal).context("serializing auth swap journal")?;
    let temporary = new_journal_temporary(&target.parent, &target.key)?;
    let result = (|| {
        let file = open_private_file(
            &target.parent,
            &temporary,
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NONBLOCK,
            Mode::from_bits_truncate(0o600),
            "opening temporary auth swap journal",
        )?;
        let mut file = file;
        file.write_all(&bytes)
            .context("writing temporary auth swap journal")?;
        file.sync_all()
            .context("syncing temporary auth swap journal")?;

        let replacing = entry_stat(&target.parent, &target.journal)?.is_some();
        if replacing {
            let stat = entry_stat(&target.parent, &target.journal)?.ok_or_else(|| {
                anyhow::anyhow!("auth swap journal disappeared during atomic rewrite")
            })?;
            validate_owned_stat(&stat, "auth swap journal", SFlag::S_IFREG)?;
            maybe_fail(FailurePoint::JournalRewrite)?;
        }
        renameat(
            &target.parent,
            temporary.as_c_str(),
            &target.parent,
            target.journal.as_c_str(),
        )
        .map_err(|error| nix_error(error, "publishing auth swap journal"))?;
        fsync_directory(&target.parent)
    })();
    if result.is_err() {
        let _ignored_cleanup = unlink_entry(
            &target.parent,
            &temporary,
            "removing failed temporary auth swap journal",
        );
    }
    result
}

pub fn read_journal(target: &TargetLock) -> anyhow::Result<Option<SwapJournal>> {
    let Some(stat) = entry_stat(&target.parent, &target.journal)? else {
        return Ok(None);
    };
    validate_owned_stat(&stat, "auth swap journal", SFlag::S_IFREG)?;
    let file = open_private_file(
        &target.parent,
        &target.journal,
        OFlag::O_RDONLY | OFlag::O_NONBLOCK,
        Mode::empty(),
        "opening auth swap journal",
    )?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut &file)
        .take((MAX_JOURNAL_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .context("reading auth swap journal")?;
    anyhow::ensure!(
        bytes.len() <= MAX_JOURNAL_BYTES,
        "auth swap journal is oversized"
    );
    let journal = serde_json::from_slice(&bytes).context("parsing auth swap journal")?;
    Ok(Some(journal))
}

pub fn unlink_entry(parent: &File, name: &CStr, label: &str) -> anyhow::Result<()> {
    match unlinkat(parent, name, UnlinkatFlags::NoRemoveDir) {
        Ok(()) => Ok(()),
        Err(Errno::ENOENT) => Ok(()),
        Err(error) => Err(nix_error(error, label)),
    }
}

/// How `remove_tree` must treat one directory entry, matched on exact
/// `S_IFMT` bits. `SFlag::contains` on whole file-type flags over-matches
/// (`S_IFLNK` contains the `S_IFREG` bit), which previously routed symlinks
/// into the regular-file writability check — and `lstat` mode bits on a
/// symlink are meaningless (Linux always reports `0777`, macOS `0755`), so
/// legitimate Linux trees were rejected as group-writable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeEntryKind {
    Directory,
    Regular,
    Symlink,
    Special,
}

pub fn classify_tree_entry_for_removal(mode: mode_t) -> TreeEntryKind {
    let file_type = mode & SFlag::S_IFMT.bits();
    if file_type == SFlag::S_IFDIR.bits() {
        TreeEntryKind::Directory
    } else if file_type == SFlag::S_IFREG.bits() {
        TreeEntryKind::Regular
    } else if file_type == SFlag::S_IFLNK.bits() {
        TreeEntryKind::Symlink
    } else {
        TreeEntryKind::Special
    }
}

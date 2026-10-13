// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Auth directory staging, wipe, and source snapshot.

use super::{
    FailurePoint, JOURNAL_SCHEMA_VERSION, LockedSource, SwapJournal, SwapPhase, TargetLock,
    copy_tree, fsync_directory, maybe_fail, new_previous, new_stage, nix_error, remove_tree,
    target_lock, target_present, unlink_entry, write_journal,
};
use crate::AuthProvisionOutcome;

use nix::errno::Errno;
use nix::fcntl::renameat;

use std::ffi::CString;
use std::fs::File;

use std::path::Path;

pub(crate) fn publish(target: &TargetLock, stage: CString) -> anyhow::Result<()> {
    let has_target = target_present(target)?;
    let previous = has_target
        .then(|| new_previous(&target.parent, &target.key))
        .transpose()?;
    let journal = SwapJournal {
        schema_version: JOURNAL_SCHEMA_VERSION,
        target: hex::encode(target.target.as_bytes()),
        stage: stage.to_string_lossy().into_owned(),
        previous: previous
            .as_ref()
            .map(|name| name.to_string_lossy().into_owned()),
        phase: SwapPhase::Prepared,
    };
    write_journal(target, &journal)?;
    maybe_fail(FailurePoint::Prepared)?;

    if let Some(previous) = &previous {
        renameat(
            &target.parent,
            target.target.as_c_str(),
            &target.parent,
            previous.as_c_str(),
        )
        .map_err(|error| nix_error(error, "moving previous auth directory"))?;
        fsync_directory(&target.parent)?;
        write_journal(
            target,
            &SwapJournal {
                phase: SwapPhase::BackedUp,
                ..journal.clone()
            },
        )?;
        maybe_fail(FailurePoint::Backup)?;
    }

    renameat(
        &target.parent,
        stage.as_c_str(),
        &target.parent,
        target.target.as_c_str(),
    )
    .map_err(|error| nix_error(error, "publishing staged auth directory"))?;
    fsync_directory(&target.parent)?;
    let installed = SwapJournal {
        phase: SwapPhase::Installed,
        ..journal
    };
    write_journal(target, &installed)?;
    maybe_fail(FailurePoint::Installed)?;

    if let Some(previous) = &previous {
        remove_tree(&target.parent, previous, "previous auth directory")?;
    }
    unlink_entry(
        &target.parent,
        &target.journal,
        "removing auth swap journal",
    )?;
    fsync_directory(&target.parent)
}

pub fn stage_auth_directory_with_locked_source<F>(
    target_dir: &Path,
    host_dir: &Path,
    source: Option<LockedSource>,
    populate: F,
) -> anyhow::Result<AuthProvisionOutcome>
where
    F: FnOnce(&Path, &File, &File) -> anyhow::Result<()>,
{
    let target = target_lock(target_dir, true).map_err(|error| {
        anyhow::anyhow!("opening auth target {}: {error:#}", target_dir.display())
    })?;
    let outcome = if let Some(source) = &source {
        let (stage, directory) = new_stage(&target.parent, &target.key)?;
        if let Err(error) = populate(host_dir, &source.root, &directory) {
            let _ignored_cleanup = remove_tree(&target.parent, &stage, "failed auth stage");
            return Err(error);
        }
        fsync_directory(&directory)?;
        drop(directory);
        publish(&target, stage)?;
        AuthProvisionOutcome::Synced
    } else {
        let (stage, directory) = new_stage(&target.parent, &target.key)?;
        fsync_directory(&directory)?;
        drop(directory);
        publish(&target, stage)?;
        AuthProvisionOutcome::HostMissing
    };
    Ok(outcome)
}

pub fn wipe_auth_directory(target_dir: &Path) -> anyhow::Result<()> {
    let target = match target_lock(target_dir, false) {
        Ok(target) => target,
        Err(error) if error.downcast_ref::<Errno>() == Some(&Errno::ENOENT) => return Ok(()),
        Err(error) => return Err(error),
    };
    if target_present(&target)? {
        remove_tree(&target.parent, &target.target, "auth destination")?;
    }
    Ok(())
}

pub fn snapshot_source(source: &File, snapshot: &File) -> anyhow::Result<()> {
    copy_tree(source, snapshot, "Hermes source snapshot")
}

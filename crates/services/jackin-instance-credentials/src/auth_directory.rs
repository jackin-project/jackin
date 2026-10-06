// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// Descriptor-relative auth directory transactions.
///
/// The destination parent and source root are opened once, every component is
/// traversed with `O_NOFOLLOW`, and all mutations use the pinned descriptors.
/// The journal is written and synced before each rename boundary so a retry can
/// complete or roll back an interrupted swap without retaining an old tree.
#[cfg(unix)]
use std::sync::atomic::AtomicU64;

#[cfg(unix)]
static AUTH_DIRECTORY_SWAP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(unix)]
mod files;
#[cfg(unix)]
mod journal;
#[cfg(unix)]
mod locks;
#[cfg(unix)]
mod mount_locks;
#[cfg(unix)]
mod open;
#[cfg(unix)]
mod source;
#[cfg(unix)]
mod stage;
#[cfg(unix)]
mod transactions;
#[cfg(unix)]
mod types;

#[cfg(unix)]
pub use types::AuthMountLease;

#[cfg(unix)]
pub use files::{
    create_private_file_if_absent, remove_file, repair_file_permissions, replace_private_file,
};
#[cfg(unix)]
pub use journal::{
    TreeEntryKind, classify_tree_entry_for_removal, read_journal, target_present, unlink_entry,
    write_journal,
};
#[cfg(any(test, feature = "test-support"))]
#[cfg(unix)]
pub use locks::lock_source_dir_for_test;
#[cfg(unix)]
pub use locks::{
    entry_file_at, file_target_lock, lock_source_dir, new_journal_temporary, new_previous,
    new_private_file_temporary, new_stage, target_lock,
};
#[cfg(any(test, feature = "test-support"))]
#[cfg(unix)]
pub use mount_locks::target_lock_key_for_test;
#[cfg(unix)]
pub use mount_locks::{
    lock_mount_directory, lock_mount_file, mount_directory_present, mount_file_present,
};
#[cfg(unix)]
pub use open::{
    ensure_same_source_identity, entry_stat, fsync_directory, ignore_eexist, nix_error,
    open_directory_at, open_parent, open_private_file, open_source_directory_at,
    open_source_directory_at_with_hook, open_source_file, open_source_file_with_hook, owned_fd,
    path_key, validate_directory, validate_owned_stat,
};
#[cfg(unix)]
pub use source::{
    copy_optional_source_file, copy_optional_source_tree, copy_tree, create_snapshot_directory,
    open_directory_path, read_locked_source_file, read_source_path,
    validate_locked_source_directory, write_private_file_at,
};
#[cfg(unix)]
pub use stage::{snapshot_source, stage_auth_directory_with_locked_source, wipe_auth_directory};
#[cfg(unix)]
pub use transactions::{recover, remove_tree};
#[cfg(any(test, feature = "test-support"))]
#[cfg(unix)]
pub use types::{FailureGuard, inject_failure, set_hermes_snapshot_hook, set_source_open_hook};
#[cfg(unix)]
pub use types::{
    FailurePoint, JOURNAL_SCHEMA_VERSION, LockedSource, MAX_JOURNAL_BYTES, SOURCE_LOCK_POLL,
    SOURCE_LOCK_TIMEOUT, SnapshotDirectory, SwapJournal, SwapPhase, TargetLock, maybe_fail,
    run_hermes_snapshot_hook, run_source_open_hook,
};

#[cfg(not(unix))]
use super::read_bounded_local_file;
#[cfg(not(unix))]
use crate::AuthProvisionOutcome;
#[cfg(not(unix))]
use std::fs::File;
#[cfg(not(unix))]
use std::path::Path;

#[derive(Clone, Debug)]
#[cfg(not(unix))]
pub struct AuthMountLease;

#[cfg(not(unix))]
pub fn mount_file_present(path: &Path) -> anyhow::Result<bool> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(metadata.is_file(), "credential mount is not a regular file");
    Ok(true)
}

#[cfg(not(unix))]
pub fn mount_directory_present(path: &Path) -> anyhow::Result<bool> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(metadata.is_dir(), "credential mount is not a directory");
    Ok(true)
}

#[cfg(not(unix))]
pub fn lock_mount_file(path: &Path) -> anyhow::Result<Option<AuthMountLease>> {
    Ok(mount_file_present(path)?.then_some(AuthMountLease))
}

#[cfg(not(unix))]
pub fn lock_mount_directory(path: &Path) -> anyhow::Result<Option<AuthMountLease>> {
    Ok(mount_directory_present(path)?.then_some(AuthMountLease))
}

#[cfg(not(unix))]
pub fn read_source_path(path: &Path, label: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        !metadata.file_type().is_symlink() && metadata.is_file(),
        "{label} is not a regular source file"
    );
    Ok(Some(super::read_bounded_local_file(path)?))
}

#[cfg(not(unix))]
pub fn stage_auth_directory<F>(
    target_dir: &Path,
    host_dir: &Path,
    populate: F,
) -> anyhow::Result<AuthProvisionOutcome>
where
    F: FnOnce(&Path, &File, &File) -> anyhow::Result<()>,
{
    let parent = target_dir.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let staged = tempfile::Builder::new()
        .prefix(".jackin-auth-stage-")
        .tempdir_in(parent)?;
    let staged_file = File::open(staged.path())?;
    let outcome = if host_dir.is_dir() {
        let source = File::open(host_dir)?;
        populate(host_dir, &source, &staged_file)?;
        AuthProvisionOutcome::Synced
    } else {
        AuthProvisionOutcome::HostMissing
    };
    std::fs::rename(staged.path(), target_dir)?;
    Ok(outcome)
}

#[cfg(not(unix))]
pub fn wipe_auth_directory(target_dir: &Path) -> anyhow::Result<()> {
    if target_dir.exists() {
        std::fs::remove_dir_all(target_dir)?;
    }
    Ok(())
}

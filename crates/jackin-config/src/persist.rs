// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Atomic file writes and workspace filename validation.
//!
//! Uses a per-process counter mixed with the PID so concurrent migrations
//! cannot clobber each other's staged files. Not responsible for config
//! deserialization, migration logic, or mount resolution.

#![expect(
    clippy::disallowed_methods,
    reason = "synchronous config persistence and advisory locking run only on caller-governed blocking paths"
)]

use anyhow::Context;
use fs4::TryLockError;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read as _, Seek as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// Per-process counter mixed with the PID into the staged-write filename.
// Combined with the PID it produces unique suffixes across concurrent
// migrations, so two writers cannot clobber each other's staged file before
// rename, and a leftover staged file cannot truncate an operator-created
// `<name>.tmp` workspace file.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const LOCK_POLL: Duration = Duration::from_millis(25);

#[derive(Clone, Copy)]
enum LockMode {
    Shared,
    Exclusive,
}

/// Held shared advisory lock for one complete config-tree snapshot.
///
/// The OS lock, not the persistent lock-file contents, is authoritative.
#[derive(Debug)]
pub struct ConfigReadGuard {
    _file: Option<File>,
}

#[derive(Debug)]
pub(crate) struct ConfigWriteGuard {
    _file: File,
}

/// A fully written and synced sibling file awaiting its atomic rename.
#[derive(Debug)]
pub(crate) struct StagedWrite {
    target: PathBuf,
    tmp: PathBuf,
    original: TargetState,
    expected_sha256: [u8; 32],
    recovery_owned: bool,
    committed: bool,
}

#[derive(Debug)]
enum TargetState {
    Missing,
    File(Vec<u8>),
    Other,
}

/// A staged deletion that can be restored if a later config mutation fails.
#[derive(Debug)]
pub(crate) struct StagedDelete {
    target: PathBuf,
    original: Vec<u8>,
    committed: bool,
}

/// Reject workspace file stems that are not valid [`WorkspaceName`](jackin_core::WorkspaceName)s.
pub fn validate_workspace_file_stem(name: &str) -> crate::ConfigResult<()> {
    jackin_core::WorkspaceName::parse(name)
        .map(drop)
        .map_err(Into::into)
}

/// Acquire a shared advisory lock covering a complete config-tree read.
///
/// Readers may coexist, but an editor excludes them until it saves or is dropped.
/// If no writer has created the sibling lock file yet, this returns an empty guard
/// instead of creating host state. Snapshot readers must still verify their input
/// bytes after parsing to cover the race with a first writer.
pub fn acquire_config_read_lock(config_file: &Path) -> crate::ConfigResult<ConfigReadGuard> {
    let lock_path = config_file.with_file_name("config.lock");
    let file = match File::open(&lock_path) {
        Ok(file) => Some(acquire_open_lock(
            file,
            &lock_path,
            LockMode::Shared,
            LOCK_TIMEOUT,
            LOCK_POLL,
        )?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    Ok(ConfigReadGuard { _file: file })
}

/// Acquire a shared advisory lock for a security-sensitive config snapshot.
///
/// Unlike [`acquire_config_read_lock`], this rejects a missing lock file. The
/// ordinary read path permits lock-free first-run reads; admission and launch
/// paths must fail closed because a lock-free snapshot cannot be held stable
/// against a writer that follows the config writer protocol.
pub fn acquire_config_read_lock_required(
    config_file: &Path,
) -> crate::ConfigResult<ConfigReadGuard> {
    let lock_path = config_file.with_file_name("config.lock");
    let file = File::open(&lock_path).map_err(|error| {
        anyhow::Error::new(error).context(format!(
            "opening required config lock {}",
            lock_path.display()
        ))
    })?;
    Ok(ConfigReadGuard {
        _file: Some(acquire_open_lock(
            file,
            &lock_path,
            LockMode::Shared,
            LOCK_TIMEOUT,
            LOCK_POLL,
        )?),
    })
}

pub(crate) fn acquire_config_write_lock(
    config_file: &Path,
) -> crate::ConfigResult<ConfigWriteGuard> {
    let mut file = acquire_lock(config_file, LockMode::Exclusive, LOCK_TIMEOUT, LOCK_POLL)?;
    // Every writer enters through this lock, so a stale publication journal
    // (previous owner died mid-commit) is rolled forward here, before any
    // read or write observes the tree. The lock is exclusive, so no
    // concurrent committer can own the journal we are completing.
    // Orphaned staged files are collected on the same path.
    recover_pending_publication(config_file)?;
    file.set_len(0)?;
    file.rewind()?;
    writeln!(file, "{}", std::process::id())?;
    file.sync_all()?;
    Ok(ConfigWriteGuard { _file: file })
}

/// Resolve the global config file that owns a split workspace file's writer
/// lock. Standalone migration callers use a sibling `config.toml`; normal
/// split files use `<config-dir>/config.toml`.
pub(crate) fn config_file_for_workspace_path(path: &Path) -> PathBuf {
    let Some(parent) = path.parent() else {
        return PathBuf::from("config.toml");
    };
    if parent.file_name() == Some(OsStr::new("workspaces")) {
        parent.parent().map_or_else(
            || parent.join("config.toml"),
            |root| root.join("config.toml"),
        )
    } else {
        parent.join("config.toml")
    }
}

fn acquire_lock(
    config_file: &Path,
    mode: LockMode,
    timeout: Duration,
    poll: Duration,
) -> crate::ConfigResult<File> {
    let lock_path = config_file.with_file_name("config.lock");
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating config directory {}", parent.display()))?;
    }
    let file = open_private(&lock_path)?;
    acquire_open_lock(file, &lock_path, mode, timeout, poll)
}

fn acquire_open_lock(
    file: File,
    lock_path: &Path,
    mode: LockMode,
    timeout: Duration,
    poll: Duration,
) -> crate::ConfigResult<File> {
    let started = Instant::now();
    acquire_open_lock_with_timing(
        file,
        lock_path,
        mode,
        timeout,
        poll,
        || started.elapsed(),
        std::thread::sleep,
    )
}

fn acquire_open_lock_with_timing<N, W>(
    file: File,
    lock_path: &Path,
    mode: LockMode,
    timeout: Duration,
    poll: Duration,
    mut elapsed: N,
    mut wait: W,
) -> crate::ConfigResult<File>
where
    N: FnMut() -> Duration,
    W: FnMut(Duration),
{
    loop {
        let acquired = match mode {
            LockMode::Shared => fs4::FileExt::try_lock_shared(&file),
            LockMode::Exclusive => fs4::FileExt::try_lock(&file),
        };
        let elapsed_now = elapsed();
        match acquired {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if elapsed_now < timeout => {
                wait(poll.min(timeout.saturating_sub(elapsed_now)));
            }
            Err(TryLockError::WouldBlock) => {
                return Err(crate::ConfigError::ConfigLockTimeout {
                    holder: recorded_holder(lock_path),
                });
            }
            Err(TryLockError::Error(err)) => return Err(err.into()),
        }
    }
}

/// Bounded read for synchronous config and credential discovery callers.
/// Callers choose a limit and interpret truncation for their format.
pub(crate) fn read_bounded_file(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
fn acquire_lock_with_timing<N, W>(
    config_file: &Path,
    mode: LockMode,
    timeout: Duration,
    poll: Duration,
    elapsed: N,
    wait: W,
) -> crate::ConfigResult<File>
where
    N: FnMut() -> Duration,
    W: FnMut(Duration),
{
    let lock_path = config_file.with_file_name("config.lock");
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = open_private(&lock_path)?;
    acquire_open_lock_with_timing(file, &lock_path, mode, timeout, poll, elapsed, wait)
}

fn recorded_holder(lock_path: &Path) -> String {
    let mut raw = String::new();
    let Ok(mut file) = File::open(lock_path) else {
        return String::new();
    };
    if file.read_to_string(&mut raw).is_ok() && raw.trim().parse::<u32>().is_ok() {
        format!(" (holder PID {})", raw.trim())
    } else {
        String::new()
    }
}

/// Write `contents` to `path` via a unique staged file then rename.
pub fn atomic_write(path: &Path, contents: &str) -> crate::ConfigResult<()> {
    let mut staged = stage_atomic_write(path, contents)?;
    staged.commit()
}

pub(crate) fn stage_atomic_write(path: &Path, contents: &str) -> crate::ConfigResult<StagedWrite> {
    stage_atomic_write_bytes(path, contents.as_bytes())
}

pub(crate) fn stage_atomic_write_bytes(
    path: &Path,
    contents: &[u8],
) -> crate::ConfigResult<StagedWrite> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directory {}", parent.display()))?;
    }
    let original = target_state(path);
    // Place the `.tmp` marker mid-filename rather than as the extension so
    // `load_workspace_files`'s `extension == "toml"` filter ignores leftover
    // staged files. PID + counter make the suffix unique across processes
    // and concurrent in-process writers.
    let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut staged_name = path
        .file_name()
        .map(OsStr::to_os_string)
        .unwrap_or_default();
    staged_name.push(format!(".tmp.{}.{counter}", std::process::id()));
    let tmp = path.with_file_name(staged_name);

    stage_write(&tmp, contents)?;
    Ok(StagedWrite {
        target: path.to_path_buf(),
        tmp,
        original,
        expected_sha256: Sha256::digest(contents).into(),
        recovery_owned: false,
        committed: false,
    })
}

fn stage_write(tmp: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let mut file = open_staged_private(tmp)?;
    if let Err(err) = file.write_all(contents).and_then(|()| file.sync_all()) {
        drop(file);
        drop(std::fs::remove_file(tmp));
        return Err(err.into());
    }
    Ok(())
}

fn target_state(path: &Path) -> TargetState {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => match std::fs::read(path) {
            Ok(contents) => TargetState::File(contents),
            Err(_) => TargetState::Other,
        },
        Ok(_) => TargetState::Other,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => TargetState::Missing,
        Err(_) => TargetState::Other,
    }
}

/// Reject a target that cannot be atomically replaced by a regular file.
pub(crate) fn ensure_replaceable_target(path: &Path) -> crate::ConfigResult<()> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(crate::ConfigError::msg(format_args!(
            "config target {} is not a regular file",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Stage removal of one regular config file. Missing files are already gone.
pub(crate) fn stage_delete(path: &Path) -> crate::ConfigResult<Option<StagedDelete>> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(StagedDelete {
            target: path.to_path_buf(),
            original: std::fs::read(path)?,
            committed: false,
        })),
        Ok(_) => Err(crate::ConfigError::msg(format_args!(
            "config target {} is not a regular file",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Sibling of `config.toml` recording a multi-file publication in progress.
///
/// Every [`commit_staged_config`] writes this journal (durably, `0600`) before
/// its first rename and removes it after its last mutation. A crash in the
/// rename window leaves the journal behind; the next
/// [`acquire_config_write_lock`] rolls it forward before any read or write
/// observes the tree, converging to either all-new (crash during commit) or
/// all-old (crash during abort).
///
/// The name stays visible (like the `config.lock` sibling) so a stale journal
/// is obvious in directory listings instead of hiding as a dotfile.
pub(crate) fn publication_journal_path(config_file: &Path) -> PathBuf {
    config_file.with_file_name("config.publish.journal")
}

/// Only publication-journal schema this binary forward-rolls.
const PUBLICATION_JOURNAL_VERSION: u32 = 2;

/// Durable record of one multi-file publication, fsync'd before the first rename.
///
/// The same ordered op list journals both directions: a commit lists the
/// staged writes then deletes, an abort lists the staged restores. Recovery
/// always rolls the listed ops forward, so it converges regardless of which
/// phase died. Unknown fields are rejected so a newer writer's journal fails
/// loud instead of half-rolling under an older reader.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicationJournal {
    version: u32,
    ops: Vec<PublicationOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase", deny_unknown_fields)]
enum PublicationOp {
    Write {
        target: PathBuf,
        tmp: PathBuf,
        expected_sha256: [u8; 32],
    },
    Delete {
        target: PathBuf,
    },
}

fn publication_ops(writes: &[StagedWrite], deletes: &[StagedDelete]) -> Vec<PublicationOp> {
    writes
        .iter()
        .map(|write| PublicationOp::Write {
            target: write.target.clone(),
            tmp: write.tmp.clone(),
            expected_sha256: write.expected_sha256,
        })
        .chain(deletes.iter().map(|delete| PublicationOp::Delete {
            target: delete.target.clone(),
        }))
        .collect()
}

/// Durably record a pending multi-file publication before the first rename.
///
/// Written via staged tmp + fsync + rename + parent fsync at `0600`, so the
/// journal itself is all-or-nothing: recovery either sees the full rename set
/// or no journal at all.
#[cfg(test)]
pub(crate) fn write_publication_journal(
    journal_path: &Path,
    writes: &[StagedWrite],
    deletes: &[StagedDelete],
) -> crate::ConfigResult<()> {
    write_publication_ops(journal_path, &publication_ops(writes, deletes))
}

fn write_publication_ops(journal_path: &Path, ops: &[PublicationOp]) -> crate::ConfigResult<()> {
    let journal = PublicationJournal {
        version: PUBLICATION_JOURNAL_VERSION,
        ops: ops.to_vec(),
    };
    let contents = serde_json::to_string_pretty(&journal).map_err(|error| {
        crate::ConfigError::msg(format_args!("serializing publication journal: {error}"))
    })?;
    atomic_write(journal_path, &contents)
}

fn remove_publication_journal(journal_path: &Path) -> crate::ConfigResult<()> {
    match std::fs::remove_file(journal_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(crate::ConfigError::msg(format_args!(
                "config committed but removing publication journal {} failed: {error}; \
                 re-run any config write to converge",
                journal_path.display()
            )));
        }
    }
    sync_parent(journal_path)
}

/// Forward-roll a publication journal left by a crash between renames.
///
/// Runs under the config tree's already-held exclusive lock (see
/// [`acquire_config_write_lock`]); it performs no locking itself. No journal
/// still garbage-collects orphaned staged files and returns `Ok(())`. Each op
/// is idempotent: a write whose target matches the recorded generation is
/// complete; already-applied deletes are skipped. A corrupt journal, a version mismatch,
/// or a write with missing or mismatched generation bytes fails closed with the
/// journal left for forensics — the operator hand-verifies the tree and
/// removes the journal to proceed. Orphaned `*.tmp.<pid>.<ctr>` staged files
/// are garbage-collected best-effort while the write lock is held.
pub(crate) fn recover_pending_publication(config_file: &Path) -> crate::ConfigResult<()> {
    let journal_path = publication_journal_path(config_file);
    let raw = match std::fs::read(&journal_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            collect_staged_garbage(config_file, &HashSet::new());
            return Ok(());
        }
        Err(error) => {
            return Err(crate::ConfigError::msg(format_args!(
                "reading publication journal {} failed: {error}",
                journal_path.display()
            )));
        }
    };
    let journal: PublicationJournal = serde_json::from_slice(&raw).map_err(|parse_error| {
        crate::ConfigError::msg(format_args!(
            "publication journal {} is corrupt (malformed JSON: {parse_error}); hand-verify the \
             config tree, then remove the journal to proceed",
            journal_path.display()
        ))
    })?;
    if journal.version != PUBLICATION_JOURNAL_VERSION {
        return Err(crate::ConfigError::msg(format_args!(
            "publication journal {} has unsupported version {} (expected \
             {PUBLICATION_JOURNAL_VERSION}); hand-verify the config tree, then remove the journal \
             to proceed",
            journal_path.display(),
            journal.version
        )));
    }
    for op in &journal.ops {
        apply_publication_op(op)?;
    }
    let listed: HashSet<PathBuf> = journal
        .ops
        .iter()
        .filter_map(|op| match op {
            PublicationOp::Write { tmp, .. } => Some(tmp.clone()),
            PublicationOp::Delete { .. } => None,
        })
        .collect();
    collect_staged_garbage(config_file, &listed);
    remove_publication_journal(&journal_path)
}

/// Best-effort removal of orphaned staged files while the write lock is held.
///
/// Only `*.tmp.<pid>.<ctr>` names the stager produces are eligible, minus
/// `listed` tmps still named by a live journal; operator `*.toml` files never
/// match. Errors are ignored: leftovers are inert (workspace scans filter by
/// the `.toml` extension) and retried on the next write-locked open.
fn collect_staged_garbage(config_file: &Path, listed: &HashSet<PathBuf>) {
    let Some(config_dir) = config_file.parent() else {
        return;
    };
    let workspaces_dir = config_dir.join("workspaces");
    for dir in [config_dir, workspaces_dir.as_path()] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_staged_garbage(&entry.file_name()) || listed.contains(&path) {
                continue;
            }
            drop(std::fs::remove_file(&path));
        }
    }
}

/// `true` when `file_name` matches the stager's `<name>.tmp.<pid>.<ctr>` shape.
fn is_staged_garbage(file_name: &OsStr) -> bool {
    let Some(name) = file_name.to_str() else {
        return false;
    };
    let Some((_, suffix)) = name.rsplit_once(".tmp.") else {
        return false;
    };
    let Some((pid, counter)) = suffix.split_once('.') else {
        return false;
    };
    !pid.is_empty()
        && !counter.is_empty()
        && pid.bytes().all(|byte| byte.is_ascii_digit())
        && counter.bytes().all(|byte| byte.is_ascii_digit())
}

/// Commit every staged config mutation, restoring committed targets if a
/// later rename, delete, or directory sync fails.
///
/// The commit point is the journal write, not the last rename: once the
/// journal is durable, any crash converges (via
/// [`recover_pending_publication`], which [`acquire_config_write_lock`]
/// runs before any read or write observes the tree) instead of stranding
/// global-new / workspace-old skew. An in-process abort swaps the journal to
/// the staged restores before applying them, so a crash mid-abort converges
/// to all-old rather than to a mix the commit journal could no longer
/// describe. Orphaned staged files are garbage-collected on the next
/// write-locked open.
pub(crate) fn commit_staged_config(
    journal_path: &Path,
    writes: &mut [StagedWrite],
    deletes: &mut [StagedDelete],
) -> crate::ConfigResult<()> {
    if writes.is_empty() && deletes.is_empty() {
        return Ok(());
    }
    commit_staged_config_with(
        journal_path,
        writes,
        deletes,
        write_publication_ops,
        StagedWrite::commit,
        stage_atomic_write_bytes,
    )
}

fn commit_staged_config_with<P, C, S>(
    journal_path: &Path,
    writes: &mut [StagedWrite],
    deletes: &mut [StagedDelete],
    publish: P,
    commit: C,
    stage_restore: S,
) -> crate::ConfigResult<()>
where
    P: FnMut(&Path, &[PublicationOp]) -> crate::ConfigResult<()>,
    C: FnMut(&mut StagedWrite) -> crate::ConfigResult<()>,
    S: FnMut(&Path, &[u8]) -> crate::ConfigResult<StagedWrite>,
{
    commit_staged_config_with_sync(
        journal_path,
        writes,
        deletes,
        (publish, commit, stage_restore),
        sync_parent,
    )
}

fn commit_staged_config_with_sync<P, C, S, D>(
    journal_path: &Path,
    writes: &mut [StagedWrite],
    deletes: &mut [StagedDelete],
    operations: (P, C, S),
    mut sync_staged_parent: D,
) -> crate::ConfigResult<()>
where
    P: FnMut(&Path, &[PublicationOp]) -> crate::ConfigResult<()>,
    C: FnMut(&mut StagedWrite) -> crate::ConfigResult<()>,
    S: FnMut(&Path, &[u8]) -> crate::ConfigResult<StagedWrite>,
    D: FnMut(&Path) -> crate::ConfigResult<()>,
{
    let (mut publish, mut commit, mut stage_restore) = operations;
    // The journal can only recover a staged generation whose directory
    // entries are durable, including entries in the workspace directory.
    sync_staged_parents(writes, &mut sync_staged_parent)?;
    // Installation may rename the journal and then fail its directory sync.
    // Recovery takes custody before that ambiguous boundary. If no journal
    // lands, the next locked recovery collects these files as orphans.
    for write in writes.iter_mut() {
        write.recovery_owned = true;
    }
    publish(journal_path, &publication_ops(writes, deletes))?;
    for write in writes.iter_mut() {
        if let Err(error) = commit(write) {
            return abort_staged_config(
                journal_path,
                writes,
                deletes,
                error,
                &mut publish,
                &mut stage_restore,
                &mut sync_staged_parent,
            );
        }
    }
    for delete in deletes.iter_mut() {
        if let Err(error) = delete.commit() {
            return abort_staged_config(
                journal_path,
                writes,
                deletes,
                error,
                &mut publish,
                &mut stage_restore,
                &mut sync_staged_parent,
            );
        }
    }
    remove_publication_journal(journal_path)
}

/// Sync each staged parent once before a journal can name its files.
fn sync_staged_parents<D>(writes: &[StagedWrite], sync: &mut D) -> crate::ConfigResult<()>
where
    D: FnMut(&Path) -> crate::ConfigResult<()>,
{
    let mut synced = HashSet::new();
    for write in writes {
        if synced.insert(write.tmp.parent()) {
            sync(&write.tmp)?;
        }
    }
    Ok(())
}

/// Restore every committed target to its pre-commit bytes.
///
/// Restores are staged first, then the journal is atomically swapped from the
/// commit ops to the restore ops before any restore is applied. If staging a
/// restore fails, the original commit journal is left in place so recovery
/// rolls the commit forward to all-new. If applying a restore fails, the
/// abort journal is left in place so recovery completes the abort to all-old.
/// Either way the on-disk outcome is deterministic.
fn abort_staged_config<P, S, D>(
    journal_path: &Path,
    writes: &mut [StagedWrite],
    deletes: &mut [StagedDelete],
    error: crate::ConfigError,
    publish: &mut P,
    stage_restore: &mut S,
    sync_staged_parent: &mut D,
) -> crate::ConfigResult<()>
where
    P: FnMut(&Path, &[PublicationOp]) -> crate::ConfigResult<()>,
    S: FnMut(&Path, &[u8]) -> crate::ConfigResult<StagedWrite>,
    D: FnMut(&Path) -> crate::ConfigResult<()>,
{
    let mut restores: Vec<PublicationOp> = Vec::new();
    let mut staged_restores = Vec::new();
    let mut restore_errors = Vec::new();
    for delete in deletes.iter_mut().rev() {
        if !delete.committed {
            continue;
        }
        match stage_restore(&delete.target, &delete.original) {
            Ok(staged) => {
                restores.extend(publication_ops(std::slice::from_ref(&staged), &[]));
                staged_restores.push(staged);
            }
            Err(stage_error) => restore_errors.push(stage_error.to_string()),
        }
    }
    for write in writes.iter_mut().rev() {
        if !write.committed {
            continue;
        }
        match &write.original {
            TargetState::Missing => {
                restores.push(PublicationOp::Delete {
                    target: write.target.clone(),
                });
            }
            TargetState::File(contents) => match stage_restore(&write.target, contents) {
                Ok(staged) => {
                    restores.extend(publication_ops(std::slice::from_ref(&staged), &[]));
                    staged_restores.push(staged);
                }
                Err(stage_error) => restore_errors.push(stage_error.to_string()),
            },
            TargetState::Other => restore_errors.push(format!(
                "cannot restore non-file config target {}",
                write.target.display()
            )),
        }
    }
    if !restore_errors.is_empty() {
        return Err(crate::ConfigError::msg(format_args!(
            "{error}; config rollback failed: {}",
            restore_errors.join("; ")
        )));
    }
    if let Err(sync_error) = sync_staged_parents(&staged_restores, sync_staged_parent) {
        return Err(crate::ConfigError::msg(format_args!(
            "{error}; config rollback failed: {sync_error}"
        )));
    }
    for staged in &mut staged_restores {
        staged.recovery_owned = true;
    }
    if let Err(journal_error) = publish(journal_path, &restores) {
        return Err(crate::ConfigError::msg(format_args!(
            "{error}; config rollback failed: {journal_error}"
        )));
    }
    // The durable abort journal no longer references forward staged files.
    // Return unused forward files to local cleanup; restore files stay owned
    // by recovery until their operations complete.
    for write in writes.iter_mut() {
        write.recovery_owned = false;
    }
    let mut apply_errors = Vec::new();
    for op in &restores {
        if let Err(apply_error) = apply_publication_op(op) {
            apply_errors.push(apply_error.to_string());
        }
    }
    if apply_errors.is_empty() {
        if let Err(journal_error) = remove_publication_journal(journal_path) {
            return Err(crate::ConfigError::msg(format_args!(
                "{error}; config rollback failed: {journal_error}"
            )));
        }
        return Err(error);
    }
    Err(crate::ConfigError::msg(format_args!(
        "{error}; config rollback failed: {}; abort journal left for recovery",
        apply_errors.join("; ")
    )))
}

fn apply_publication_op(op: &PublicationOp) -> crate::ConfigResult<()> {
    match op {
        PublicationOp::Write {
            target,
            tmp,
            expected_sha256,
        } => {
            if tmp.parent() != target.parent() {
                return Err(crate::ConfigError::msg(format_args!(
                    "publication journal staged file {} is not a sibling of {}",
                    tmp.display(),
                    target.display()
                )));
            }
            if !tmp.exists() {
                if target.is_file() {
                    verify_publication_bytes(target, expected_sha256)?;
                    return sync_parent(target);
                }
                return Err(crate::ConfigError::msg(format_args!(
                    "publication journal cannot complete write to {}: staged file {} is gone; \
                     hand-verify the config tree, then remove the journal to proceed",
                    target.display(),
                    tmp.display()
                )));
            }
            verify_publication_bytes(tmp, expected_sha256)?;
            ensure_replaceable_target(target)?;
            std::fs::rename(tmp, target).map_err(|rename_error| {
                anyhow::Error::new(rename_error).context(format!(
                    "renaming {} -> {}",
                    tmp.display(),
                    target.display()
                ))
            })?;
            sync_parent(target)
        }
        PublicationOp::Delete { target } => {
            match std::fs::remove_file(target) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(crate::ConfigError::msg(format_args!(
                        "publication journal cannot complete delete of {}: {error}",
                        target.display()
                    )));
                }
            }
            sync_parent(target)
        }
    }
}

fn verify_publication_bytes(path: &Path, expected_sha256: &[u8; 32]) -> crate::ConfigResult<()> {
    let actual: [u8; 32] = Sha256::digest(std::fs::read(path)?).into();
    if &actual != expected_sha256 {
        return Err(crate::ConfigError::msg(format_args!(
            "publication journal generation mismatch at {}; recovery artifacts retained",
            path.display()
        )));
    }
    Ok(())
}

fn open_staged_private(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn open_private(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Leak staged writes without running their cleanup `Drop`, simulating a
/// crash between staging and rename. Test-only: production drops always run.
#[cfg(test)]
pub(crate) fn leak_staged_writes(staged: Vec<StagedWrite>) {
    let _leaked = std::mem::ManuallyDrop::new(staged);
}

fn sync_parent(path: &Path) -> crate::ConfigResult<()> {
    if let Some(parent) = path.parent() {
        File::open(parent)
            .with_context(|| format!("opening parent directory {}", parent.display()))?
            .sync_all()
            .with_context(|| format!("syncing parent directory {}", parent.display()))?;
    }
    Ok(())
}

impl StagedWrite {
    pub(crate) fn commit(&mut self) -> crate::ConfigResult<()> {
        std::fs::rename(&self.tmp, &self.target).map_err(|rename_err| {
            anyhow::Error::new(rename_err).context(format!(
                "renaming {} -> {}",
                self.tmp.display(),
                self.target.display()
            ))
        })?;
        self.committed = true;
        sync_parent(&self.target)
    }
}

impl StagedDelete {
    fn commit(&mut self) -> crate::ConfigResult<()> {
        std::fs::remove_file(&self.target).map_err(|error| {
            anyhow::Error::new(error).context(format!("removing {}", self.target.display()))
        })?;
        self.committed = true;
        sync_parent(&self.target)
    }
}

impl Drop for StagedWrite {
    fn drop(&mut self) {
        if !self.recovery_owned && !self.committed {
            drop(std::fs::remove_file(&self.tmp));
        }
    }
}

#[cfg(test)]
mod tests;

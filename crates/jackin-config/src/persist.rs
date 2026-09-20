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

pub(crate) fn acquire_config_write_lock(
    config_file: &Path,
) -> crate::ConfigResult<ConfigWriteGuard> {
    let mut file = acquire_lock(config_file, LockMode::Exclusive, LOCK_TIMEOUT, LOCK_POLL)?;
    // Every writer enters through this lock, so a stale publication journal
    // (previous owner died mid-commit) is rolled forward here, before any
    // read or write observes the tree. The lock is exclusive, so no
    // concurrent committer can own the journal we are completing.
    recover_publication_journal(&publication_journal_path(config_file))?;
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
        Ok(_) => Err(crate::ConfigError::msg(format!(
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
        Ok(_) => Err(crate::ConfigError::msg(format!(
            "config target {} is not a regular file",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Sibling of `config.toml` recording a multi-file publication in progress.
///
/// Every [`commit_staged_config`] writes this journal (durably) before its
/// first rename and removes it after its last mutation. A crash in the rename
/// window leaves the journal behind; the next [`acquire_config_write_lock`]
/// rolls it forward before any read or write observes the tree, converging to
/// either all-new (crash during commit) or all-old (crash during abort).
pub(crate) fn publication_journal_path(config_file: &Path) -> PathBuf {
    config_file.with_file_name("config.publish.journal")
}

const PUBLICATION_JOURNAL_VERSION: u32 = 1;

/// Ordered rename/remove list a crashed publication left behind.
///
/// The same shape journals both directions: a commit lists the staged writes
/// then deletes, an abort lists the staged restores. Recovery always rolls
/// the listed ops forward, so it converges regardless of which phase died.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PublicationJournal {
    version: u32,
    ops: Vec<PublicationOp>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum PublicationOp {
    Write { target: PathBuf, tmp: PathBuf },
    Delete { target: PathBuf },
}

/// Commit every staged config mutation, restoring committed targets if a
/// later rename, delete, or directory sync fails.
///
/// The commit point is the journal write, not the last rename: once the
/// journal is durable, any crash converges (via
/// [`recover_publication_journal`]) instead of stranding global-new /
/// workspace-old skew. An in-process abort swaps the journal to the staged
/// restores before applying them, so a crash mid-abort converges to all-old
/// rather than to a mix the commit journal could no longer describe.
pub(crate) fn commit_staged_config(
    journal_path: &Path,
    writes: &mut [StagedWrite],
    deletes: &mut [StagedDelete],
) -> crate::ConfigResult<()> {
    if writes.is_empty() && deletes.is_empty() {
        return Ok(());
    }
    write_publication_journal(journal_path, &publication_ops(writes, deletes))?;
    for write in writes.iter_mut() {
        if let Err(error) = write.commit() {
            return abort_staged_config(journal_path, writes, deletes, error);
        }
    }
    for delete in deletes.iter_mut() {
        if let Err(error) = delete.commit() {
            return abort_staged_config(journal_path, writes, deletes, error);
        }
    }
    remove_publication_journal(journal_path)
}

fn publication_ops(writes: &[StagedWrite], deletes: &[StagedDelete]) -> Vec<PublicationOp> {
    writes
        .iter()
        .map(|write| PublicationOp::Write {
            target: write.target.clone(),
            tmp: write.tmp.clone(),
        })
        .chain(deletes.iter().map(|delete| PublicationOp::Delete {
            target: delete.target.clone(),
        }))
        .collect()
}

fn write_publication_journal(journal_path: &Path, ops: &[PublicationOp]) -> crate::ConfigResult<()> {
    let journal = PublicationJournal {
        version: PUBLICATION_JOURNAL_VERSION,
        ops: ops.to_vec(),
    };
    let contents = serde_json::to_string_pretty(&journal).map_err(|error| {
        crate::ConfigError::msg(format!("serializing publication journal: {error}"))
    })?;
    atomic_write(journal_path, &contents)
}

fn remove_publication_journal(journal_path: &Path) -> crate::ConfigResult<()> {
    match std::fs::remove_file(journal_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(crate::ConfigError::msg(format!(
                "config committed but removing publication journal {} failed: {error}; \
                 re-run any config write to converge",
                journal_path.display()
            )));
        }
    }
    sync_parent(journal_path)
}

/// Restore every committed target to its pre-commit bytes.
///
/// Restores are staged first, then the journal is atomically swapped from the
/// commit ops to the restore ops before any restore is applied. If staging a
/// restore fails, the original commit journal is left in place so recovery
/// rolls the commit forward to all-new. If applying a restore fails, the
/// abort journal is left in place so recovery completes the abort to all-old.
/// Either way the on-disk outcome is deterministic.
fn abort_staged_config(
    journal_path: &Path,
    writes: &mut [StagedWrite],
    deletes: &mut [StagedDelete],
    error: crate::ConfigError,
) -> crate::ConfigResult<()> {
    let mut restores: Vec<PublicationOp> = Vec::new();
    let mut restore_errors = Vec::new();
    for delete in deletes.iter_mut().rev() {
        if !delete.committed {
            continue;
        }
        match stage_atomic_write_bytes(&delete.target, &delete.original) {
            Ok(staged) => {
                let tmp = staged.tmp.clone();
                std::mem::forget(staged);
                restores.push(PublicationOp::Write {
                    target: delete.target.clone(),
                    tmp,
                });
                delete.committed = false;
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
                write.committed = false;
            }
            TargetState::File(contents) => match stage_atomic_write_bytes(&write.target, contents) {
                Ok(staged) => {
                    let tmp = staged.tmp.clone();
                    std::mem::forget(staged);
                    restores.push(PublicationOp::Write {
                        target: write.target.clone(),
                        tmp,
                    });
                    write.committed = false;
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
        return Err(crate::ConfigError::msg(format!(
            "{error}; config rollback failed: {}",
            restore_errors.join("; ")
        )));
    }
    if let Err(journal_error) = write_publication_journal(journal_path, &restores) {
        return Err(crate::ConfigError::msg(format!(
            "{error}; config rollback failed: {journal_error}"
        )));
    }
    let mut apply_errors = Vec::new();
    for op in &restores {
        if let Err(apply_error) = apply_publication_op(op) {
            apply_errors.push(apply_error.to_string());
        }
    }
    if apply_errors.is_empty() {
        if let Err(journal_error) = remove_publication_journal(journal_path) {
            return Err(crate::ConfigError::msg(format!(
                "{error}; config rollback failed: {journal_error}"
            )));
        }
        return Err(error);
    }
    Err(crate::ConfigError::msg(format!(
        "{error}; config rollback failed: {}; abort journal left for recovery",
        apply_errors.join("; ")
    )))
}

/// Roll a stale publication journal forward. Missing journal is a no-op.
///
/// Runs under the exclusive config write lock (see
/// [`acquire_config_write_lock`]), so no concurrent committer can own the
/// journal being completed. Each op is idempotent: an already-renamed write
/// (tmp gone, target present) and an already-applied delete are skipped. A
/// malformed journal, a version mismatch, or a write whose staged tmp AND
/// target are both gone fails closed with the journal left for forensics —
/// the operator hand-verifies the tree and removes the journal to proceed.
pub(crate) fn recover_publication_journal(journal_path: &Path) -> crate::ConfigResult<()> {
    let raw = match std::fs::read(journal_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(crate::ConfigError::msg(format!(
                "reading publication journal {} failed: {error}",
                journal_path.display()
            )));
        }
    };
    let journal: PublicationJournal =
        serde_json::from_slice(&raw).map_err(|parse_error| {
            crate::ConfigError::msg(format!(
                "publication journal {} is malformed ({parse_error}); hand-verify the config \
                 tree, then remove the journal to proceed",
                journal_path.display()
            ))
        })?;
    if journal.version != PUBLICATION_JOURNAL_VERSION {
        return Err(crate::ConfigError::msg(format!(
            "publication journal {} has unsupported version {}; hand-verify the config tree, \
             then remove the journal to proceed",
            journal_path.display(),
            journal.version
        )));
    }
    for op in &journal.ops {
        apply_publication_op(op)?;
    }
    remove_publication_journal(journal_path)
}

fn apply_publication_op(op: &PublicationOp) -> crate::ConfigResult<()> {
    match op {
        PublicationOp::Write { target, tmp } => {
            if tmp.parent() != target.parent() {
                return Err(crate::ConfigError::msg(format!(
                    "publication journal staged file {} is not a sibling of {}",
                    tmp.display(),
                    target.display()
                )));
            }
            if !tmp.exists() {
                if target.exists() {
                    return Ok(());
                }
                return Err(crate::ConfigError::msg(format!(
                    "publication journal cannot complete write to {}: staged file {} is gone; \
                     hand-verify the config tree, then remove the journal to proceed",
                    target.display(),
                    tmp.display()
                )));
            }
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
                    return Err(crate::ConfigError::msg(format!(
                        "publication journal cannot complete delete of {}: {error}",
                        target.display()
                    )));
                }
            }
            sync_parent(target)
        }
    }
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
        if !self.committed {
            drop(std::fs::remove_file(&self.tmp));
        }
    }
}

#[cfg(test)]
mod tests;

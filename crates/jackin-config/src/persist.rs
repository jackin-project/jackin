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
    file.set_len(0)?;
    file.rewind()?;
    writeln!(file, "{}", std::process::id())?;
    file.sync_all()?;
    Ok(ConfigWriteGuard { _file: file })
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
    stage_atomic_write(path, contents)?.commit()
}

pub(crate) fn stage_atomic_write(path: &Path, contents: &str) -> crate::ConfigResult<StagedWrite> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directory {}", parent.display()))?;
    }
    // Place the `.tmp` marker mid-filename rather than as the extension so
    // `load_workspace_files`'s `extension == "toml"` filter ignores leftover
    // staged files. PID + counter make the suffix unique across processes
    // and concurrent in-process writers.
    let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut staged_name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    staged_name.push(format!(".tmp.{}.{counter}", std::process::id()));
    let tmp = path.with_file_name(staged_name);

    stage_write(&tmp, contents)?;
    Ok(StagedWrite {
        target: path.to_path_buf(),
        tmp,
        committed: false,
    })
}

fn stage_write(tmp: &Path, contents: &str) -> anyhow::Result<()> {
    let mut file = open_staged_private(tmp)?;
    if let Err(err) = file
        .write_all(contents.as_bytes())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        drop(std::fs::remove_file(tmp));
        return Err(err.into());
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
    pub(crate) fn commit(mut self) -> crate::ConfigResult<()> {
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

impl Drop for StagedWrite {
    fn drop(&mut self) {
        if !self.committed {
            drop(std::fs::remove_file(&self.tmp));
        }
    }
}

#[cfg(test)]
mod tests;

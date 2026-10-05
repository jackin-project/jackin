// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tracks how long the operator has been "in the construct".
//!
//! The span runs from the launch that brought the first container up to the
//! exit of the last one. A marker in the permanent coordination namespace
//! holds the start instant; the exit ritual clears it to show elapsed time.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jackin_core::JackinPaths;
use jackin_docker::docker_client::DockerApi;

const FORCE_BOUNDARY_RITUALS_ENV: &str = "JACKIN_FORCE_BOUNDARY_RITUALS";
const FORCE_BOUNDARY_INTRO_ENV: &str = "JACKIN_FORCE_BOUNDARY_INTRO";
const FORCE_BOUNDARY_OUTRO_ENV: &str = "JACKIN_FORCE_BOUNDARY_OUTRO";

static CLAIM_COUNTER: AtomicU64 = AtomicU64::new(0);
const ENTRY_OBSERVATION_ATTEMPTS: usize = 8;

// Never unlink this file: replacing its inode would split concurrent locks.
fn boundary_lock(authority: &Path) -> std::io::Result<std::fs::File> {
    let file = super::coordination::open_in_namespace(authority, "universe-lock")?;
    file.lock()?;
    Ok(file)
}

async fn boundary_work<T: Send + 'static>(
    authority: &Path,
    action: impl FnOnce(&Path) -> std::io::Result<T> + Send + 'static,
) -> std::io::Result<T> {
    let authority = authority.to_owned();
    blocking_work(move || action(&authority)).await
}

async fn universe_authority(paths: &JackinPaths) -> std::io::Result<PathBuf> {
    let paths = paths.clone();
    blocking_work(move || super::coordination::universe_dir(&paths)).await
}

async fn blocking_work<T: Send + 'static>(
    action: impl FnOnce() -> std::io::Result<T> + Send + 'static,
) -> std::io::Result<T> {
    let dispatcher = tracing::dispatcher::get_default(Clone::clone);
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || {
        tracing::dispatcher::with_default(&dispatcher, || {
            let _span = span.enter();
            action()
        })
    })
    .await
    .map_err(std::io::Error::other)?
}

fn generation(authority: &Path) -> std::io::Result<Option<String>> {
    state_read(authority, "universe-generation")?
        .map(|value| String::from_utf8(value).map_err(std::io::Error::other))
        .transpose()
}

fn advance_generation(authority: &Path) -> std::io::Result<String> {
    let value = claim_token();
    state_write(authority, "universe-generation", value.as_bytes())?;
    Ok(value)
}

#[cfg(test)]
fn marker_path(authority: &Path) -> PathBuf {
    authority.join("universe-since")
}

fn pending_dir(authority: &Path) -> PathBuf {
    authority.join("universe-pending")
}

fn pending_path(authority: &Path, token: &str) -> PathBuf {
    pending_dir(authority).join(token)
}

fn state_read(directory: &Path, key: &str) -> std::io::Result<Option<Vec<u8>>> {
    let mut file = match super::coordination::open_state_in_namespace(directory, key, false) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut value = Vec::new();
    file.read_to_end(&mut value)?;
    Ok(Some(value))
}

fn state_write(directory: &Path, key: &str, value: &[u8]) -> std::io::Result<()> {
    let mut file = super::coordination::open_state_in_namespace(directory, key, true)?;
    // The shared opener validates the owned private regular inode before
    // truncation; an existing symlink/nonregular file cannot redirect writes.
    file.set_len(0)?;
    file.write_all(value)
}

fn pending_exists(path: &Path) -> std::io::Result<bool> {
    let Some((directory, key)) = path.parent().zip(path.file_name()) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid pending claim",
        ));
    };
    let key = key.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid pending claim key",
        )
    })?;
    match super::coordination::open_state_in_namespace(directory, key, false) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn state_remove(directory: &Path, key: &str) -> std::io::Result<()> {
    let _file = super::coordination::open_state_in_namespace(directory, key, false)?;
    let parent = super::coordination::open_directory_in_namespace(directory, false)?;
    #[cfg(unix)]
    {
        nix::unistd::unlinkat(&parent, key, nix::unistd::UnlinkatFlags::NoRemoveDir)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "private state removal requires Unix",
        ))
    }
}

fn pending_remove(path: &Path) -> std::io::Result<()> {
    let directory = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid pending claim")
    })?;
    let key = path
        .file_name()
        .and_then(|key| key.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid pending claim key",
            )
        })?;
    state_remove(directory, key)
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

fn claim_token() -> String {
    let counter = CLAIM_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{counter}", std::process::id(), now_millis())
}

pub(super) fn env_flag_enabled(value: Option<impl AsRef<std::ffi::OsStr>>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let Some(value) = value.as_ref().to_str() else {
        return true;
    };
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "no" | "off"
    )
}

fn force_boundary_rituals_enabled() -> bool {
    env_flag_enabled(std::env::var_os(FORCE_BOUNDARY_RITUALS_ENV))
}

#[must_use]
pub fn force_boundary_intro_enabled() -> bool {
    force_boundary_rituals_enabled() || env_flag_enabled(std::env::var_os(FORCE_BOUNDARY_INTRO_ENV))
}

#[must_use]
pub(super) fn force_boundary_outro_enabled() -> bool {
    force_boundary_rituals_enabled() || env_flag_enabled(std::env::var_os(FORCE_BOUNDARY_OUTRO_ENV))
}

/// Whether a launch enters an empty construct or joins one already running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartKind {
    /// No containers were running before this launch — (re)write the marker so
    /// the span starts now.
    FreshConstruct,
    /// A session is already ongoing — keep its original start instant.
    ResumeExisting,
}

/// A launch's claim on the construct-entry boundary.
///
/// Pending claims cover the short window before a role container exists. They
/// prevent concurrent launches from both playing the two-screen intro, and let
/// an early failed launch release only its own pending entry. The claim owns
/// its pending file and holds an advisory lease on it until activation or early
/// release. Process death drops the lease so the next boundary operation can
/// reclaim the orphaned token.
#[derive(Debug)]
pub struct EntryClaim {
    kind: StartKind,
    pending_file: Option<PathBuf>,
    _pending_lease: std::sync::Mutex<Option<std::fs::File>>,
}

impl PartialEq for EntryClaim {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.pending_file == other.pending_file
    }
}

impl Eq for EntryClaim {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ExitClaim {
    Missing,
    Claimed { elapsed: Option<Duration> },
}

impl EntryClaim {
    #[must_use]
    pub const fn start_kind(&self) -> StartKind {
        self.kind
    }

    #[must_use]
    const fn none(kind: StartKind) -> Self {
        Self {
            kind,
            pending_file: None,
            _pending_lease: std::sync::Mutex::new(None),
        }
    }

    fn release_pending_lease(&self) {
        if let Ok(mut lease) = self._pending_lease.lock() {
            drop(lease.take());
        }
    }

    /// Hand the launch boundary from this pending lease to its live container.
    /// Call only after the role container has started or is already running.
    pub async fn activate(&self) -> std::io::Result<()> {
        let Some(pending_file) = self.pending_file.as_ref() else {
            return Ok(());
        };
        let authority = pending_file
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid entry claim path")
            })?;
        let pending_file = pending_file.clone();
        let result = boundary_work(authority, move |authority| {
            let _lock = boundary_lock(authority)?;
            if !pending_exists(&pending_file)? {
                return Ok(());
            }
            advance_generation(authority)?;
            match pending_remove(&pending_file) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
        })
        .await;
        if result.is_ok() {
            self.release_pending_lease();
        }
        result
    }

    async fn release_if_idle(&self, docker: &impl DockerApi) {
        let Some(pending_file) = self.pending_file.as_ref() else {
            return;
        };
        let Some(authority) = pending_file.parent().and_then(Path::parent) else {
            return;
        };
        let pending_file = pending_file.clone();
        let observed_generation = boundary_work(authority, move |authority| {
            let _lock = boundary_lock(authority)?;
            if !pending_exists(&pending_file)? {
                return Ok(None);
            }
            let generation = advance_generation(authority)?;
            drop(pending_remove(&pending_file));
            Ok(Some(generation))
        })
        .await;
        let observed_generation = match observed_generation {
            Ok(Some(generation)) => {
                self.release_pending_lease();
                generation
            }
            Ok(None) => {
                self.release_pending_lease();
                return;
            }
            Err(_) => return,
        };

        let Ok(running) = super::discovery::list_running_agent_names(docker).await else {
            return;
        };
        if running.is_empty() {
            drop(
                boundary_work(authority, move |authority| {
                    release_marker_if_unchanged(authority, &observed_generation);
                    Ok(())
                })
                .await,
            );
        }
    }
}

fn release_marker_if_unchanged(authority: &Path, observed_generation: &str) {
    let Ok(_lock) = boundary_lock(authority) else {
        return;
    };
    if generation(authority).ok().flatten().as_deref() == Some(observed_generation)
        && prune_stale_pending_claims(authority).is_ok()
        && !has_pending_claims(authority)
        && advance_generation(authority).is_ok()
    {
        drop(state_remove(authority, "universe-since"));
        remove_empty_pending_dir(authority);
    }
}

impl Drop for EntryClaim {
    fn drop(&mut self) {
        if let Some(pending_file) = self.pending_file.as_ref() {
            // The shared marker needs asynchronous Docker proof before removal.
            // Scope cleanup to this launch's owned pending file.
            if let Some(authority) = pending_file.parent().and_then(Path::parent) {
                let Ok(_lock) = boundary_lock(authority) else {
                    return;
                };
                if !pending_exists(pending_file).unwrap_or(false) {
                    return;
                }
                // Invalidate Docker observations before removing a pending
                // launch. Activated or explicitly released leases are inert.
                if advance_generation(authority).is_ok() {
                    drop(pending_remove(pending_file));
                }
            }
        }
    }
}

/// Claim the construct-entry boundary for an actual launch.
///
/// A fresh launch is one where Docker reports no running role containers and
/// no pending claim exists for an already-starting launch.
pub async fn claim_entry(paths: &JackinPaths, docker: &impl DockerApi) -> EntryClaim {
    let Ok(authority) = universe_authority(paths).await else {
        return EntryClaim::none(StartKind::ResumeExisting);
    };
    for _ in 0..ENTRY_OBSERVATION_ATTEMPTS {
        let Ok(observed_generation) = boundary_work(&authority, |authority| {
            let _lock = boundary_lock(authority)?;
            generation(authority)
        })
        .await
        else {
            return EntryClaim::none(StartKind::ResumeExisting);
        };
        let Ok(names) = super::discovery::list_running_agent_names(docker).await else {
            return EntryClaim::none(StartKind::ResumeExisting);
        };
        match boundary_work(&authority, move |authority| {
            let _lock = boundary_lock(authority)?;
            if generation(authority)? != observed_generation {
                return Ok(None);
            }
            advance_generation(authority)?;
            register_pending_entry_locked(authority, names.is_empty()).map(Some)
        })
        .await
        {
            Ok(Some(claim)) => return claim,
            Ok(None) => {}
            Err(_) => return EntryClaim::none(StartKind::ResumeExisting),
        }
    }
    // Churn prevents a trustworthy empty-Docker observation, but this launch
    // still needs its own pending lease until an actual role container exists.
    boundary_work(&authority, |authority| {
        let _lock = boundary_lock(authority)?;
        advance_generation(authority)?;
        register_pending_entry_locked(authority, false)
    })
    .await
    .unwrap_or_else(|_| EntryClaim::none(StartKind::ResumeExisting))
}

fn register_pending_entry_locked(
    authority: &Path,
    allow_fresh: bool,
) -> std::io::Result<EntryClaim> {
    prune_stale_pending_claims(authority)?;
    let token = claim_token();
    let pending_lease = write_pending_claim(authority, &token);
    let pending_count = count_pending_claims(authority).unwrap_or(usize::MAX);
    let kind = if allow_fresh && pending_lease.is_some() && pending_count <= 1 {
        StartKind::FreshConstruct
    } else {
        StartKind::ResumeExisting
    };
    if let Err(error) = mark_start_locked(authority, kind) {
        if pending_lease.is_some() {
            drop(state_remove(&pending_dir(authority), &token));
        }
        return Err(error);
    }
    Ok(EntryClaim {
        kind,
        pending_file: pending_lease
            .as_ref()
            .map(|_| pending_path(authority, &token)),
        _pending_lease: std::sync::Mutex::new(pending_lease),
    })
}

/// Record the construct's start instant. A `FreshConstruct` launch (re)writes
/// the marker to now; a `ResumeExisting` launch only writes it if absent, so an
/// ongoing session keeps its original start.
pub(super) async fn mark_start(paths: &JackinPaths, kind: StartKind) {
    let Ok(authority) = universe_authority(paths).await else {
        return;
    };
    drop(
        boundary_work(&authority, move |authority| {
            let _lock = boundary_lock(authority)?;
            advance_generation(authority)?;
            mark_start_locked(authority, kind)?;
            Ok(())
        })
        .await,
    );
}

fn mark_start_locked(authority: &Path, kind: StartKind) -> std::io::Result<()> {
    if kind == StartKind::ResumeExisting && state_read(authority, "universe-since")?.is_some() {
        return Ok(());
    }
    state_write(
        authority,
        "universe-since",
        now_millis().to_string().as_bytes(),
    )
}

pub async fn release_entry_if_idle(docker: &impl DockerApi, claim: &EntryClaim) {
    claim.release_if_idle(docker).await;
}

fn write_pending_claim(authority: &Path, token: &str) -> Option<std::fs::File> {
    let dir = pending_dir(authority);
    let Ok(parent) = super::coordination::open_directory_in_namespace(&dir, true) else {
        return None;
    };
    #[cfg(unix)]
    {
        use nix::fcntl::{OFlag, openat};
        let flags = OFlag::O_WRONLY
            | OFlag::O_CREAT
            | OFlag::O_EXCL
            | OFlag::O_NOFOLLOW
            | OFlag::O_CLOEXEC
            | OFlag::O_NONBLOCK;
        let Ok(fd) = openat(
            &parent,
            token,
            flags,
            nix::sys::stat::Mode::from_bits_truncate(0o600),
        ) else {
            return None;
        };
        let mut file = std::fs::File::from(fd);
        if file.lock().is_err() {
            let _ignored_unlink_result =
                nix::unistd::unlinkat(&parent, token, nix::unistd::UnlinkatFlags::NoRemoveDir);
            return None;
        }
        if file.write_all(now_millis().to_string().as_bytes()).is_err() {
            let _ignored_unlink_result =
                nix::unistd::unlinkat(&parent, token, nix::unistd::UnlinkatFlags::NoRemoveDir);
            return None;
        }
        Some(file)
    }
    #[cfg(not(unix))]
    {
        let _ = (parent, token);
        None
    }
}

/// Reclaim tokens whose owning process no longer holds its advisory lease.
/// Call only while holding the permanent boundary lock.
fn prune_stale_pending_claims(authority: &Path) -> std::io::Result<()> {
    let directory = pending_dir(authority);
    let parent = match super::coordination::open_directory_in_namespace(&directory, false) {
        Ok(parent) => parent,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    #[cfg(unix)]
    {
        let mut entries = nix::dir::Dir::from_fd(parent.try_clone()?.into())?;
        for entry in entries.iter() {
            let entry = entry?;
            let bytes = entry.file_name().to_bytes();
            if matches!(bytes, b"." | b"..") {
                continue;
            }
            let key = std::str::from_utf8(bytes).map_err(std::io::Error::other)?;
            let file = match super::coordination::open_state_in_namespace(&directory, key, false) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            match file.try_lock() {
                Ok(()) => {
                    nix::unistd::unlinkat(&parent, key, nix::unistd::UnlinkatFlags::NoRemoveDir)?;
                }
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(std::fs::TryLockError::Error(error)) => return Err(error),
            }
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "pending claim recovery requires Unix file locks",
        ))
    }
}

fn count_pending_claims(authority: &Path) -> Option<usize> {
    let dir = pending_dir(authority);
    let parent = match super::coordination::open_directory_in_namespace(&dir, false) {
        Ok(parent) => parent,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(0),
        Err(_) => return None,
    };
    #[cfg(unix)]
    {
        let mut entries = nix::dir::Dir::from_fd(parent.into()).ok()?;
        entries.iter().try_fold(0, |count, entry| {
            let entry = entry.ok()?;
            Some(count + usize::from(!matches!(entry.file_name().to_bytes(), b"." | b"..")))
        })
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        None
    }
}

fn has_pending_claims(authority: &Path) -> bool {
    count_pending_claims(authority).is_none_or(|count| count > 0)
}

fn remove_empty_pending_dir(authority: &Path) {
    if !has_pending_claims(authority) {
        #[cfg(unix)]
        if let Ok(parent) = super::coordination::open_directory_in_namespace(authority, false) {
            let _ignored_unlink_result = nix::unistd::unlinkat(
                &parent,
                "universe-pending",
                nix::unistd::UnlinkatFlags::RemoveDir,
            );
        }
    }
}

fn record_exit_claim_recovery() {
    let _warning = jackin_telemetry::record_recovered_degradation();
}

/// Claim the construct-exit boundary.
///
/// The marker is the single-consumer close claim: whichever exit path removes
/// it is the one that may render the rich outro. A malformed marker still
/// grants the claim, but omits the elapsed line from the caption.
#[must_use]
#[cfg(test)]
pub(super) fn take_exit_claim(paths: &JackinPaths) -> ExitClaim {
    let Ok(authority) = super::coordination::universe_dir(paths) else {
        record_exit_claim_recovery();
        return ExitClaim::Missing;
    };
    let Ok(_lock) = boundary_lock(&authority) else {
        record_exit_claim_recovery();
        return ExitClaim::Missing;
    };
    take_exit_claim_locked(&authority)
}

/// Observe Docker and claim the exit only if no launch changed the boundary
/// while the Docker request was in flight. The file lock never crosses await.
pub(super) async fn observe_exit(
    paths: &JackinPaths,
    docker: &impl DockerApi,
) -> anyhow::Result<(Vec<String>, ExitClaim)> {
    let authority = universe_authority(paths).await;
    let observed_generation = if let Ok(authority) = authority.as_ref() {
        boundary_work(authority, |authority| {
            let _lock = boundary_lock(authority)?;
            generation(authority)
        })
        .await
    } else {
        Err(std::io::Error::other(
            "unavailable universe coordination authority",
        ))
    };
    let running = super::discovery::list_running_agent_names(docker).await?;
    let claim = if running.is_empty() {
        if let (Ok(observed), Ok(authority)) = (observed_generation, authority) {
            boundary_work(&authority, move |authority| {
                Ok(take_exit_claim_if_unchanged(authority, observed.as_deref()))
            })
            .await
            .unwrap_or_else(|_| {
                record_exit_claim_recovery();
                ExitClaim::Missing
            })
        } else {
            record_exit_claim_recovery();
            ExitClaim::Missing
        }
    } else {
        ExitClaim::Missing
    };
    Ok((running, claim))
}

fn take_exit_claim_if_unchanged(authority: &Path, observed: Option<&str>) -> ExitClaim {
    let Ok(_lock) = boundary_lock(authority) else {
        record_exit_claim_recovery();
        return ExitClaim::Missing;
    };
    match generation(authority) {
        Ok(current) if current.as_deref() == observed => take_exit_claim_locked(authority),
        Ok(_) => ExitClaim::Missing,
        Err(_) => {
            record_exit_claim_recovery();
            ExitClaim::Missing
        }
    }
}

fn take_exit_claim_locked(authority: &Path) -> ExitClaim {
    if prune_stale_pending_claims(authority).is_err() {
        record_exit_claim_recovery();
        return ExitClaim::Missing;
    }
    if has_pending_claims(authority) {
        return ExitClaim::Missing;
    }
    if advance_generation(authority).is_err() {
        record_exit_claim_recovery();
        return ExitClaim::Missing;
    }
    // Every exit holds the same permanent lock. Read and unlink the validated
    // marker via its pinned directory; exactly one exit can consume it.
    let content = match state_read(authority, "universe-since") {
        Ok(Some(content)) => content,
        Ok(None) => return ExitClaim::Missing,
        Err(_) => {
            record_exit_claim_recovery();
            return ExitClaim::Missing;
        }
    };
    if state_remove(authority, "universe-since").is_err() {
        record_exit_claim_recovery();
        return ExitClaim::Missing;
    }
    remove_empty_pending_dir(authority);
    let elapsed = String::from_utf8(content)
        .unwrap_or_default()
        .trim()
        .parse::<u128>()
        .ok()
        .and_then(|started| now_millis().checked_sub(started))
        .map(|elapsed_ms| Duration::from_millis(u64::try_from(elapsed_ms).unwrap_or(u64::MAX)));
    ExitClaim::Claimed { elapsed }
}

#[cfg(test)]
mod tests;

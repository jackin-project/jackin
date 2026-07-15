// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tracks how long the operator has been "in the construct".
//!
//! The span runs from the launch that brought the first container up to the
//! exit of the last one. A single marker file under the data dir holds the
//! start instant; the exit ritual reads and clears it to show elapsed time.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jackin_core::JackinPaths;
use jackin_docker::docker_client::DockerApi;

const FORCE_BOUNDARY_RITUALS_ENV: &str = "JACKIN_FORCE_BOUNDARY_RITUALS";
const FORCE_BOUNDARY_INTRO_ENV: &str = "JACKIN_FORCE_BOUNDARY_INTRO";
const FORCE_BOUNDARY_OUTRO_ENV: &str = "JACKIN_FORCE_BOUNDARY_OUTRO";

static CLAIM_COUNTER: AtomicU64 = AtomicU64::new(0);

fn marker_path(paths: &JackinPaths) -> PathBuf {
    paths.data_dir.join("universe-since")
}

fn pending_dir(paths: &JackinPaths) -> PathBuf {
    paths.data_dir.join("universe-pending")
}

fn pending_path(paths: &JackinPaths, token: &str) -> PathBuf {
    pending_dir(paths).join(token)
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
/// an early failed launch release only its own pending entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryClaim {
    kind: StartKind,
    token: Option<String>,
}

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
        Self { kind, token: None }
    }

    async fn release_if_idle(&self, paths: &JackinPaths, docker: &impl DockerApi) {
        let Some(token) = self.token.as_deref() else {
            return;
        };
        drop(std::fs::remove_file(pending_path(paths, token)));

        let Ok(running) = super::discovery::list_running_agent_names(docker).await else {
            return;
        };
        if running.is_empty() && !has_pending_claims(paths) {
            drop(std::fs::remove_file(marker_path(paths)));
            remove_empty_pending_dir(paths);
        }
    }
}

/// Claim the construct-entry boundary for an actual launch.
///
/// A fresh launch is one where Docker reports no running role containers and
/// no pending claim exists for an already-starting launch.
pub async fn claim_entry(paths: &JackinPaths, docker: &impl DockerApi) -> EntryClaim {
    let Ok(names) = super::discovery::list_running_agent_names(docker).await else {
        return EntryClaim::none(StartKind::ResumeExisting);
    };
    if !names.is_empty() {
        mark_start(paths, StartKind::ResumeExisting);
        return EntryClaim::none(StartKind::ResumeExisting);
    }

    let token = claim_token();
    let wrote_claim = write_pending_claim(paths, &token);
    let pending_count = count_pending_claims(paths).unwrap_or(usize::MAX);
    let kind = if wrote_claim && pending_count <= 1 {
        StartKind::FreshConstruct
    } else {
        StartKind::ResumeExisting
    };
    mark_start(paths, kind);
    EntryClaim {
        kind,
        token: wrote_claim.then_some(token),
    }
}

/// Record the construct's start instant. A `FreshConstruct` launch (re)writes
/// the marker to now; a `ResumeExisting` launch only writes it if absent, so an
/// ongoing session keeps its original start.
pub(super) fn mark_start(paths: &JackinPaths, kind: StartKind) {
    let file = marker_path(paths);
    if kind == StartKind::ResumeExisting && file.exists() {
        return;
    }
    drop(std::fs::write(&file, now_millis().to_string()));
}

pub async fn release_entry_if_idle(
    paths: &JackinPaths,
    docker: &impl DockerApi,
    claim: &EntryClaim,
) {
    claim.release_if_idle(paths, docker).await;
}

fn write_pending_claim(paths: &JackinPaths, token: &str) -> bool {
    let dir = pending_dir(paths);
    if std::fs::create_dir_all(&dir).is_err() {
        return false;
    }
    std::fs::write(pending_path(paths, token), now_millis().to_string()).is_ok()
}

fn count_pending_claims(paths: &JackinPaths) -> Option<usize> {
    let dir = pending_dir(paths);
    if !dir.exists() {
        return Some(0);
    }
    Some(std::fs::read_dir(dir).ok()?.filter_map(Result::ok).count())
}

fn has_pending_claims(paths: &JackinPaths) -> bool {
    count_pending_claims(paths).is_none_or(|count| count > 0)
}

fn remove_empty_pending_dir(paths: &JackinPaths) {
    if !has_pending_claims(paths) {
        drop(std::fs::remove_dir(pending_dir(paths)));
    }
}

/// Claim the construct-exit boundary.
///
/// The marker is the single-consumer close claim: whichever exit path removes
/// it is the one that may render the rich outro. A malformed marker still
/// grants the claim, but omits the elapsed line from the caption.
#[must_use]
pub(super) fn take_exit_claim(paths: &JackinPaths) -> ExitClaim {
    let file = marker_path(paths);
    // The rename is the claim, not the read: `rename` is atomic on POSIX, so
    // when parallel exits race only one can move the marker away — the losers
    // see ENOENT and bow out. A read-then-remove would let every racer observe
    // the marker first and render a duplicate outro.
    let claimed = file.with_file_name(format!("universe-since.claim.{}", std::process::id()));
    if let Err(error) = std::fs::rename(&file, &claimed) {
        // NotFound is the normal "no marker / already claimed" path. Any other
        // errno (e.g. a permissions drift on the data dir) is unexpected and
        // would silently suppress the outro, so leave a breadcrumb under
        // --debug to tell the two cases apart.
        if error.kind() != std::io::ErrorKind::NotFound {
            jackin_diagnostics::debug_log!("universe", "exit-claim rename failed: {error}");
        }
        return ExitClaim::Missing;
    }
    let content = std::fs::read_to_string(&claimed).unwrap_or_default();
    drop(std::fs::remove_file(&claimed));
    drop(std::fs::remove_dir_all(pending_dir(paths)));
    let elapsed = content
        .trim()
        .parse::<u128>()
        .ok()
        .and_then(|started| now_millis().checked_sub(started))
        .map(|elapsed_ms| Duration::from_millis(u64::try_from(elapsed_ms).unwrap_or(u64::MAX)));
    ExitClaim::Claimed { elapsed }
}

#[cfg(test)]
mod tests;

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Claim objects for the construct-entry/exit boundary.
//!
//! Split out of `jackin-runtime-universe` (S7 split 83) so remainder
//! signatures can name the claim types without depending on the T7
//! observation side. The observation entry points (`claim_entry`,
//! `observe_exit`, `take_exit_claim`) stay in `universe` and build
//! these claims through the constructors below.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::boundary::{
    advance_generation, boundary_lock, boundary_work, pending_exists, pending_remove,
};

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
    pending_lease: std::sync::Mutex<Option<std::fs::File>>,
}

impl PartialEq for EntryClaim {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.pending_file == other.pending_file
    }
}

impl Eq for EntryClaim {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitClaim {
    Missing,
    Claimed { elapsed: Option<Duration> },
}

impl EntryClaim {
    #[must_use]
    pub const fn start_kind(&self) -> StartKind {
        self.kind
    }

    /// A detached claim: no pending token, for degraded paths where the
    /// boundary authority is unavailable.
    #[must_use]
    pub const fn none(kind: StartKind) -> Self {
        Self {
            kind,
            pending_file: None,
            pending_lease: std::sync::Mutex::new(None),
        }
    }

    /// A claim owning the pending token at `path`, with its advisory `lease`
    /// held until activation or early release.
    #[must_use]
    pub fn pending(kind: StartKind, path: PathBuf, lease: std::fs::File) -> Self {
        Self {
            kind,
            pending_file: Some(path),
            pending_lease: std::sync::Mutex::new(Some(lease)),
        }
    }

    /// The owned pending-token path, if this claim holds one. The
    /// exit-observation side derives its boundary root from it.
    #[must_use]
    pub fn pending_file(&self) -> Option<&PathBuf> {
        self.pending_file.as_ref()
    }

    /// Drop the advisory lease without removing the token. The
    /// exit-observation side calls this after consuming the pending file,
    /// so dropping the claim cannot invalidate a newer observation.
    pub fn release_pending_lease(&self) {
        if let Ok(mut lease) = self.pending_lease.lock() {
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

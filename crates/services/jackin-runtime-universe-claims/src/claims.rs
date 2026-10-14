// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::boundary::{
    advance_generation, boundary_lock, boundary_work, pending_exists, pending_remove,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartKind {
    FreshConstruct,
    ResumeExisting,
}

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

    #[must_use]
    pub const fn none(kind: StartKind) -> Self {
        Self {
            kind,
            pending_file: None,
            pending_lease: std::sync::Mutex::new(None),
        }
    }

    #[must_use]
    pub fn pending(kind: StartKind, path: PathBuf, lease: std::fs::File) -> Self {
        Self {
            kind,
            pending_file: Some(path),
            pending_lease: std::sync::Mutex::new(Some(lease)),
        }
    }

    #[must_use]
    pub fn pending_file(&self) -> Option<&PathBuf> {
        self.pending_file.as_ref()
    }

    pub fn release_pending_lease(&self) {
        if let Ok(mut lease) = self.pending_lease.lock() {
            drop(lease.take());
        }
    }

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
        if let Some(pending_file) = self.pending_file.as_ref()
            && let Some(authority) = pending_file.parent().and_then(Path::parent)
        {
            let Ok(_lock) = boundary_lock(authority) else {
                return;
            };
            if !pending_exists(pending_file).unwrap_or(false) {
                return;
            }

            if advance_generation(authority).is_ok() {
                drop(pending_remove(pending_file));
            }
        }
    }
}

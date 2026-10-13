// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Auth directory transaction types, leases, and test hooks.

use super::remove_tree;

use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::fs::File;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use std::time::Duration;

pub(crate) const JOURNAL_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_JOURNAL_BYTES: usize = 16 * 1024;
pub(crate) const SOURCE_LOCK_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const SOURCE_LOCK_POLL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum SwapPhase {
    Prepared,
    BackedUp,
    Installed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SwapJournal {
    pub(crate) schema_version: u32,
    pub(crate) target: String,
    pub(crate) stage: String,
    pub(crate) previous: Option<String>,
    pub(crate) phase: SwapPhase,
}

#[derive(Debug)]
pub(crate) struct TargetLock {
    pub(crate) parent: File,
    pub(crate) target: CString,
    pub(crate) key: String,
    pub(crate) journal: CString,
    pub(crate) _lock: Arc<File>,
}

#[derive(Debug)]
pub struct AuthMountLease {
    pub(crate) _lock: Arc<File>,
}

#[derive(Debug)]
pub(crate) struct LockedSource {
    pub(crate) root: File,
}

pub(crate) struct SnapshotDirectory {
    pub(crate) path: PathBuf,
    pub(crate) parent: File,
    pub(crate) name: CString,
}

impl SnapshotDirectory {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SnapshotDirectory {
    fn drop(&mut self) {
        drop(remove_tree(
            &self.parent,
            self.name.as_c_str(),
            "auth source snapshot",
        ));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FailurePoint {
    JournalRewrite,
    Prepared,
    Backup,
    Installed,
}

#[cfg(test)]
thread_local! {
    pub(crate) static FAILURE_POINT: std::cell::Cell<Option<FailurePoint>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) struct FailureGuard;

#[cfg(test)]
impl Drop for FailureGuard {
    fn drop(&mut self) {
        FAILURE_POINT.with(|point| point.set(None));
    }
}

#[cfg(test)]
pub(crate) fn inject_failure(point: FailurePoint) -> FailureGuard {
    FAILURE_POINT.with(|failure| failure.set(Some(point)));
    FailureGuard
}

#[cfg(test)]
thread_local! {
    pub(crate) static HERMES_SNAPSHOT_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
    pub(crate) static SOURCE_OPEN_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn set_hermes_snapshot_hook(hook: Box<dyn FnOnce()>) {
    HERMES_SNAPSHOT_HOOK.with(|slot| *slot.borrow_mut() = Some(hook));
}

#[cfg(test)]
pub(crate) fn set_source_open_hook(hook: Box<dyn FnOnce()>) {
    SOURCE_OPEN_HOOK.with(|slot| *slot.borrow_mut() = Some(hook));
}

pub(crate) fn run_hermes_snapshot_hook() {
    #[cfg(test)]
    if let Some(hook) = HERMES_SNAPSHOT_HOOK.with(|slot| slot.borrow_mut().take()) {
        hook();
    }
}

pub(crate) fn run_source_open_hook() {
    #[cfg(test)]
    if let Some(hook) = SOURCE_OPEN_HOOK.with(|slot| slot.borrow_mut().take()) {
        hook();
    }
}

pub(crate) fn maybe_fail(point: FailurePoint) -> anyhow::Result<()> {
    #[cfg(test)]
    if FAILURE_POINT.with(|failure| failure.get() == Some(point)) {
        anyhow::bail!("injected auth directory crash at {point:?}");
    }
    #[cfg(not(test))]
    let _ = point;
    Ok(())
}

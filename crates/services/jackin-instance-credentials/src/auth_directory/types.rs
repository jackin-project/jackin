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

pub const JOURNAL_SCHEMA_VERSION: u32 = 1;
pub const MAX_JOURNAL_BYTES: usize = 16 * 1024;
pub const SOURCE_LOCK_TIMEOUT: Duration = Duration::from_secs(5);
pub const SOURCE_LOCK_POLL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SwapPhase {
    Prepared,
    BackedUp,
    Installed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SwapJournal {
    pub schema_version: u32,
    pub target: String,
    pub stage: String,
    pub previous: Option<String>,
    pub phase: SwapPhase,
}

#[derive(Debug)]
pub struct TargetLock {
    pub parent: File,
    pub target: CString,
    pub key: String,
    pub journal: CString,
    pub(crate) _lock: Arc<File>,
}

#[derive(Debug)]
pub struct AuthMountLease {
    pub(crate) _lock: Arc<File>,
}

#[derive(Debug)]
pub struct LockedSource {
    pub root: File,
}

#[derive(Debug)]
pub struct SnapshotDirectory {
    pub path: PathBuf,
    pub parent: File,
    pub name: CString,
}

impl SnapshotDirectory {
    pub fn path(&self) -> &Path {
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
pub enum FailurePoint {
    JournalRewrite,
    Prepared,
    Backup,
    Installed,
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    pub static FAILURE_POINT: std::cell::Cell<Option<FailurePoint>> = const { std::cell::Cell::new(None) };
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub struct FailureGuard;

#[cfg(any(test, feature = "test-support"))]
impl Drop for FailureGuard {
    fn drop(&mut self) {
        FAILURE_POINT.with(|point| point.set(None));
    }
}

#[cfg(any(test, feature = "test-support"))]
pub fn inject_failure(point: FailurePoint) -> FailureGuard {
    FAILURE_POINT.with(|failure| failure.set(Some(point)));
    FailureGuard
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    pub static HERMES_SNAPSHOT_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
    pub static SOURCE_OPEN_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(any(test, feature = "test-support"))]
pub fn set_hermes_snapshot_hook(hook: Box<dyn FnOnce()>) {
    HERMES_SNAPSHOT_HOOK.with(|slot| *slot.borrow_mut() = Some(hook));
}

#[cfg(any(test, feature = "test-support"))]
pub fn set_source_open_hook(hook: Box<dyn FnOnce()>) {
    SOURCE_OPEN_HOOK.with(|slot| *slot.borrow_mut() = Some(hook));
}

pub fn run_hermes_snapshot_hook() {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(hook) = HERMES_SNAPSHOT_HOOK.with(|slot| slot.borrow_mut().take()) {
        hook();
    }
}

pub fn run_source_open_hook() {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(hook) = SOURCE_OPEN_HOOK.with(|slot| slot.borrow_mut().take()) {
        hook();
    }
}

pub fn maybe_fail(point: FailurePoint) -> anyhow::Result<()> {
    #[cfg(any(test, feature = "test-support"))]
    if FAILURE_POINT.with(|failure| failure.get() == Some(point)) {
        anyhow::bail!("injected auth directory crash at {point:?}");
    }
    #[cfg(not(any(test, feature = "test-support")))]
    let _ = point;
    Ok(())
}

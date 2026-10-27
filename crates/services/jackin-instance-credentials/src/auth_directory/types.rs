// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

pub fn run_hermes_snapshot_hook() {}

pub fn run_source_open_hook() {}

pub fn maybe_fail(point: FailurePoint) -> anyhow::Result<()> {
    let _ = point;
    Ok(())
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Git repo context inside the container: branch, ahead/behind counts, dirty
//! state, and PR metadata for the status bar.
//!
//! Not responsible for: rendering the status bar (see `tui`) or host-side git
//! operations.
//!
//! Key invariant: all `git` and `gh` calls are bounded by
//! `GIT_CONTEXT_COMMAND_TIMEOUT` / `GH_PULL_REQUEST_COMMAND_TIMEOUT` so a
//! slow repo cannot stall the daemon tick.

mod context;
mod packed_refs;
mod refs;
mod watch;
mod workdir;

pub(crate) use context::git_current_context;
#[cfg(target_os = "linux")]
pub(crate) use context::git_metadata_dirs;
#[cfg(test)]
pub(crate) use context::read_branch_from_git_head;
#[cfg(test)]
pub(crate) use context::read_context_from_git_metadata;
pub(crate) use packed_refs::read_packed_git_ref_oid;
#[cfg(test)]
pub(crate) use packed_refs::{
    PACKED_REFS_CACHE_MAX_ENTRIES, PACKED_REFS_MAX_BYTES, with_packed_refs_cache,
};
pub(crate) use refs::read_git_ref_oid;
#[cfg(not(target_os = "linux"))]
pub(crate) use watch::start_git_context_watcher;
#[cfg(target_os = "linux")]
pub(crate) use watch::start_git_context_watcher;
#[cfg(test)]
pub(crate) use workdir::GIT_CONTEXT_COMMAND_TIMEOUT;
pub(crate) use workdir::{
    GH_PULL_REQUEST_COMMAND_TIMEOUT, WorkdirContext, git_capture_at_workdir, resolve_default_branch,
};

pub(crate) fn record_recovered_degradation() {
    let _warning = jackin_telemetry::record_recovered_degradation();
}

#[cfg(target_os = "linux")]
pub(crate) fn record_io_error() {
    let _error =
        jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::IoError);
}

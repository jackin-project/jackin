// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Runtime-home pruning.
//!
//! [`prune_jackin_home`] removes the remaining runtime home state
//! after the coordination gate confirms nothing prunable is still
//! running, reporting through the shared prune-output rows.
//! Instance, role, and cache pruning stays in the `jackin-runtime`
//! hub.

use jackin_core::JackinPaths;
use jackin_runtime_cleanup_timing::timing::{cleanup_failure, cleanup_timing};
use jackin_runtime_coordination::coordination::ensure_prunable;
use jackin_runtime_isolation::isolation::safe_remove::safe_remove_dir_all;
use jackin_runtime_prune_output::prune_output;

/// Remove the remaining runtime home state.
///
/// Refuses while the coordination gate reports unprunable state;
/// a deletion failure is reported through the prune-output row and
/// propagates with the home path as context.
pub fn prune_jackin_home(paths: &JackinPaths) -> anyhow::Result<()> {
    ensure_prunable(paths, &paths.jackin_home)?;
    let _timing = cleanup_timing("runtime_home");
    prune_output::section("Runtime Home", "removing remaining runtime state");
    let row = prune_output::start("Deleting", "runtime home");
    match safe_remove_dir_all(&paths.jackin_home) {
        Err(err) => {
            cleanup_failure(format!("could not remove runtime home: {err}"));
            row.failed(format!("could not remove runtime home: {err}"));
            return Err(err.into());
        }
        Ok(()) => row.ok(),
    }
    Ok(())
}

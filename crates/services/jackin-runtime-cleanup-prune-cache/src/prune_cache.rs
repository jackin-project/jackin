// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared-cache pruning.
//!
//! [`prune_cache`] removes the rebuildable shared cache
//! after the coordination gate confirms nothing prunable is
//! still running, delegating the guarded deletion to the
//! shared prune-one-directory helper. Instance pruning stays
//! in the `jackin-runtime` hub; image, home, and role pruning
//! live in their own leaves.

use jackin_core::JackinPaths;
use jackin_runtime_cleanup_prune_dir::prune_dir::prune_dir;
use jackin_runtime_coordination::coordination::ensure_prunable;

/// Remove the rebuildable shared cache.
///
/// Refuses while the coordination gate reports unprunable state;
/// deletion runs through the shared [`prune_dir`] helper with
/// the shared-cache labels.
pub fn prune_cache(paths: &JackinPaths) -> anyhow::Result<()> {
    ensure_prunable(paths, &paths.cache_dir)?;
    prune_dir(
        &paths.cache_dir,
        "Shared Cache",
        "removing rebuildable shared cache",
        "shared cache",
    )
}

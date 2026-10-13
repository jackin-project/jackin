// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Role-cache pruning.
//!
//! [`prune_roles`] removes the cached role repositories
//! after the coordination gate confirms nothing prunable is
//! still running, delegating the guarded deletion to the
//! shared prune-one-directory helper. Instance, cache, and
//! home pruning stays in the `jackin-runtime` hub.

use jackin_core::JackinPaths;
use jackin_runtime_cleanup_prune_dir::prune_dir::prune_dir;
use jackin_runtime_coordination::coordination::ensure_prunable;

/// Remove the cached role repositories.
///
/// Refuses while the coordination gate reports unprunable state;
/// deletion runs through the shared [`prune_dir`] helper with
/// the role-cache labels.
pub fn prune_roles(paths: &JackinPaths) -> anyhow::Result<()> {
    ensure_prunable(paths, &paths.roles_dir)?;
    prune_dir(
        &paths.roles_dir,
        "Role Cache",
        "removing cached role repositories",
        "role cache",
    )
}

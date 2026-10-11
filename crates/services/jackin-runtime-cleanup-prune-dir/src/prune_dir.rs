// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared prune-one-directory helper.
//!
//! [`prune_dir`] deletes a single directory through the
//! owned-validated remover, reporting through the shared
//! prune-output rows with the failure recorded as a cleanup
//! failure. Role, cache, and home pruning call it from the
//! `jackin-runtime` hub.

use jackin_runtime_cleanup_timing::timing::{cleanup_failure, cleanup_timing};
use jackin_runtime_isolation::isolation::safe_remove::safe_remove_dir_all;
use jackin_runtime_prune_output::prune_output;

/// Delete one directory, reporting through prune-output rows.
///
/// A deletion failure is recorded via [`cleanup_failure`] and
/// propagates with the target label and path as context.
pub fn prune_dir(
    path: &std::path::Path,
    section_label: &str,
    section_detail: &str,
    target_label: &str,
) -> anyhow::Result<()> {
    let _timing = cleanup_timing("prune_dir");
    prune_output::section(section_label, section_detail);
    let row = prune_output::start("Deleting", target_label);
    let result: anyhow::Result<()> = match safe_remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) => Err(anyhow::Error::from(error).context(format!(
            "failed to remove {target_label} at {}",
            path.display()
        ))),
    };
    row.complete(result, |error| {
        cleanup_failure(format!("could not remove {target_label}: {error}"));
        format!("could not remove {target_label}: {error}")
    })
}

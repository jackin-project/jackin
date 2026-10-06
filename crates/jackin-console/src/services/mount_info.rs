// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Non-TUI mount source inspection services.

/// Inspect mount sources for display metadata.
pub fn inspect_entries(sources: Vec<String>) -> Vec<(String, crate::mount_info::MountKind)> {
    sources
        .into_iter()
        .map(|src| {
            let kind = crate::mount_info::inspect(&src);
            (src, kind)
        })
        .collect()
}

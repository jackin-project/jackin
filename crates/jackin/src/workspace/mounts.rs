// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Mount spec parsing — re-exported from `jackin-config`.
pub use jackin_config::mounts::covers;
pub use jackin_config::{parse_mount_spec, parse_mount_spec_resolved};

#[cfg(test)]
mod tests;

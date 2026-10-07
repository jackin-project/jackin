// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Docker network creation and `DinD` sidecar launch for the role container.
//!
//! Moved to [`jackin_runtime_launch_dind::launch_dind`]; this module keeps the
//! `jackin_runtime::runtime::launch::launch_dind::*` paths stable for existing
//! callers. Unit coverage stays in the hub suite below (it names
//! `PREWARM_STATE_FILE` plus hub-only fixtures).

pub use jackin_runtime_launch_dind::launch_dind::*;

// Test-only scope restoration: the hub suite below was written against
// `use super::*` when this module owned the items plus these imports.
#[cfg(test)]
use jackin_docker::docker_client::ContainerState;

#[cfg(test)]
mod tests;

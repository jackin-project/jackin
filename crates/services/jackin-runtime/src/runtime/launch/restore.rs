// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Restore candidates: discovery, the launch-dialog choice, and
//! preserved-status persistence.
//!
//! Moved to [`jackin_runtime_launch_restore::restore`]; this module keeps the
//! `jackin_runtime::runtime::launch::restore::*` paths stable for existing
//! callers. Unit coverage stays in the hub suite below (it names
//! `super::launch_candidate_for_manifest` plus hub-only fixtures).

pub(crate) use jackin_runtime_launch_restore::restore::*;

// Test-only scope restoration: the hub suite below was written against
// `use super::*` when this module owned the items plus these imports.
#[cfg(test)]
use jackin_core::JackinPaths;
#[cfg(test)]
use jackin_docker::docker_client::ContainerState;
#[cfg(test)]
use jackin_instance::InstanceManifest;

#[cfg(test)]
mod tests;

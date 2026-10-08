// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Backend selection for persisted runtime instances.
//!
//! The selector reads the backend recorded in the instance manifest
//! (Docker vs Apple Container) and defaults legacy manifests without
//! recorded resources to Docker. Lifecycle dispatch over the selected
//! backend stays in the `jackin-runtime` hub.

use jackin_core::JackinPaths;
use jackin_instance::{BackendResources, InstanceManifest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceBackend {
    Docker,
    AppleContainer,
}

pub fn backend_for_manifest(manifest: Option<&InstanceManifest>) -> InstanceBackend {
    match manifest.and_then(|manifest| manifest.backend.as_ref()) {
        Some(BackendResources::AppleContainer(_)) => InstanceBackend::AppleContainer,
        Some(BackendResources::Docker(_)) | None => InstanceBackend::Docker,
    }
}

pub fn backend_for_state(paths: &JackinPaths, container_name: &str) -> InstanceBackend {
    let state_dir = paths.data_dir.join(container_name);
    let manifest = InstanceManifest::read_optional_lossy(&state_dir);
    backend_for_manifest(manifest.as_ref())
}

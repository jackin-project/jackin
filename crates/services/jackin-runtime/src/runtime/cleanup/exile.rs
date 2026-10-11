// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Full exile helpers.
//!
//! The shared prune-one-directory helper lives in
//! `jackin_runtime_cleanup_prune_dir` (S7 split 102); its hub
//! re-export was retired by S7 split 104 once the last hub
//! consumer moved out.

use crate::instance::InstanceManifest;
use crate::runtime::prune_output;
use jackin_core::JackinPaths;

use jackin_docker::docker_client::DockerApi;

use crate::runtime::backend::InstanceBackend;
use crate::runtime::discovery::list_managed_role_names;

use super::{cleanup_timing, eject_role};

pub async fn exile_all(paths: &JackinPaths, docker: &impl DockerApi) -> anyhow::Result<()> {
    let _timing = cleanup_timing("exile_all");
    let mut names = prune_output::start("Finding", "managed containers")
        .complete(list_managed_role_names(docker).await, |error| {
            format!("could not list containers: {error}")
        })?;
    for name in apple_container_instance_names(paths)? {
        if !names.iter().any(|existing| existing == &name) {
            names.push(name);
        }
    }

    for name in &names {
        prune_output::start("Stopping", name)
            .complete(eject_role(paths, name, docker).await, |error| {
                format!("could not remove Docker resources: {error}")
            })?;
    }
    Ok(())
}

pub(crate) fn apple_container_instance_names(paths: &JackinPaths) -> anyhow::Result<Vec<String>> {
    if !paths.data_dir.exists() {
        return Ok(vec![]);
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&paths.data_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(manifest) = InstanceManifest::read_optional_lossy(&entry.path()) else {
            continue;
        };
        if matches!(
            crate::runtime::backend::backend_for_manifest(Some(&manifest)),
            InstanceBackend::AppleContainer
        ) {
            names.push(name);
        }
    }
    Ok(names)
}

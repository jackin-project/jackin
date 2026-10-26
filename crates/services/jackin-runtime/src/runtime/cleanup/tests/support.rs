// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::instance::{InstanceIndex, InstanceStatus};
pub(super) fn tempdir() -> std::io::Result<TempDir> {
    tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir())?)
}

pub(super) fn gc_test_paths() -> JackinPaths {
    let temp = tempdir().unwrap();
    JackinPaths::for_tests(temp.path())
}

pub(super) fn write_owned_cleanup_manifest(paths: &JackinPaths, role: &str, dind: &str) {
    let mut manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: role,
        workspace_name: None,
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: DockerResources {
            dind_container: Some(dind.to_owned()),
            ..DockerResources::from_container_name(role)
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.docker_identity = Some(crate::instance::DockerIdentity {
        role_container_id: role.to_owned(),
        dind_container_id: Some(dind.to_owned()),
    });
    manifest.write(&paths.data_dir.join(role)).unwrap();
}

pub(super) fn make_instance_at(paths: &JackinPaths, container: &str, status: InstanceStatus) {
    let mut manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: container,
        workspace_name: Some("ws"),
        workspace_label: "ws",
        workdir: "/ws",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: DockerResources {
            role_container: container.to_owned(),
            dind_container: Some(format!("{container}-dind")),
            network: format!("{container}-net"),
            certs_volume: Some(format!("{container}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.mark_status(status);
    let state_dir = paths.data_dir.join(container);
    std::fs::create_dir_all(&state_dir).unwrap();
    manifest.write(&state_dir).unwrap();
    InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
}

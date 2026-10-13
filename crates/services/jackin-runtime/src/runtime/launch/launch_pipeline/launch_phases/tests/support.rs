// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn test_manifest(container: &str) -> InstanceManifest {
    let role_source_git = "https://example.invalid/agent-smith.git";
    InstanceManifest::new(NewInstanceManifest {
        container_base: container,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: Agent::Claude,
        role_source_git,
        role_source_ref: None,
        image_tag: "projectjackin/agent-smith:test",
        docker: DockerResources::from_container_name(container),
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    })
}

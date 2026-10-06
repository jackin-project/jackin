// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn sample_manifest() -> InstanceManifest {
    InstanceManifest::new(NewInstanceManifest {
        container_base: "jk-k7p9m2xq-workspace-agent",
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "org/agent",
        role_display_name: "Agent",
        agent_runtime: Agent::Claude,
        role_source_git: "https://example.invalid/role.git",
        role_source_ref: Some("main"),
        image_tag: "jk_org_agent",
        docker: DockerResources {
            role_container: "jk-k7p9m2xq-workspace-agent".to_owned(),
            dind_container: Some("jk-k7p9m2xq-workspace-agent-dind".to_owned()),
            network: "jk-k7p9m2xq-workspace-agent-net".to_owned(),
            certs_volume: Some("jk-k7p9m2xq-workspace-agent-dind-certs".to_owned()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    })
}

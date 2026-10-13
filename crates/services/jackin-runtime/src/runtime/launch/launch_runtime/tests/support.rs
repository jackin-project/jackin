// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn ownership_manifest() -> crate::instance::InstanceManifest {
    crate::instance::InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: "ownership-role",
        workspace_name: None,
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "org/role",
        role_display_name: "Role",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/role.git",
        role_source_ref: None,
        image_tag: "image",
        docker: crate::instance::DockerResources::from_container_name("ownership-role"),
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    })
}

pub(super) fn capsule_config_with(instances: &[(&str, &str)]) -> jackin_protocol::CapsuleConfig {
    let mut config = jackin_protocol::CapsuleConfig::default();
    for (id, agent) in instances {
        config.instances.push((*id).to_owned());
        config.agents.insert((*id).to_owned(), (*agent).to_owned());
    }
    config
}

pub(super) fn final_hook_state(paths: &JackinPaths) -> RoleState {
    let root = paths.data_dir.join("instances/current");
    std::fs::create_dir_all(root.join("home/.codex")).unwrap();
    RoleState {
        gh_config_dir: root.join("gh"),
        root,
        gh_provision_outcome: crate::instance::GithubProvisionOutcome::Skipped,
        agent_runtime: crate::instance::AgentRuntimeState {
            agent: jackin_core::Agent::Codex,
            model: None,
        },
        auth: crate::instance::ProvisionedAuth::default(),
        auth_outcomes: std::collections::BTreeMap::default(),
        auth_mount_paths: std::collections::BTreeSet::default(),
        auth_mount_leases: Vec::new(),
        provider_config_mounts: Vec::new(),
    }
}

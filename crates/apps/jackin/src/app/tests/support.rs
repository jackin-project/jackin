// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
impl std::fmt::Debug for MovedPathEntryStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancel => write!(f, "Cancel"),
            Self::Accepted(p) => write!(f, "Accepted({})", p.display()),
            Self::Retry(s) => write!(f, "Retry({s})"),
        }
    }
}

pub(super) fn ad_hoc_manifest_for_workdir(workdir: &std::path::Path) -> instance::InstanceManifest {
    let workdir = workdir.display().to_string();
    instance::InstanceManifest::new(instance::NewInstanceManifest {
        container_base: "jk-k7p9m2xq-agentsmith",
        workspace_name: None,
        workspace_label: &workdir,
        workdir: &workdir,
        host_workdir_fingerprint: &instance::manifest::host_path_fingerprint(&workdir),
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/agent-smith.git",
        role_source_ref: None,
        image_tag: "jk_agent-smith",
        docker: instance::DockerResources {
            role_container: "jk-k7p9m2xq-agentsmith".to_owned(),
            dind_container: Some("jk-k7p9m2xq-agentsmith-dind".to_owned()),
            network: "jk-k7p9m2xq-agentsmith-net".to_owned(),
            certs_volume: Some("jk-k7p9m2xq-agentsmith-dind-certs".to_owned()),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: Vec::new(),
    })
}

pub(super) fn write_stop_test_manifest(
    paths: &JackinPaths,
    workdir: &std::path::Path,
    status: instance::InstanceStatus,
) -> String {
    let mut manifest = ad_hoc_manifest_for_workdir(workdir);
    manifest.mark_status(status);
    let container = manifest.container_base.clone();
    manifest.write(&paths.data_dir.join(&container)).unwrap();
    instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
    container
}

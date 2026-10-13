// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}

pub(super) fn sample_record() -> IsolationRecord {
    IsolationRecord {
        workspace_name: Some(WorkspaceName::parse("jackin").unwrap()),
        mount_dst: "/workspace/jackin".into(),
        original_src: "/home/u/projects/jackin".into(),
        isolation: MountIsolation::Worktree,
        worktree_path: "/home/u/.jackin/data/jackin-x/isolated/workspace/jackin".into(),
        scratch_branch: "jackin/scratch/the-architect".into(),
        base_commit: "deadbeef".into(),
        selector_key: "the-architect".into(),
        container_name: "jk-a1b2c3d4-thearchitect".into(),
        cleanup_status: CleanupStatus::Active,
    }
}

pub(super) fn write_v1_fixture(
    state_dir: &Path,
    workspace_name: Option<&str>,
    label: &str,
) -> Vec<u8> {
    use jackin_instance::manifest::{DockerResources, InstanceManifest, NewInstanceManifest};
    let container = state_dir.file_name().unwrap().to_str().unwrap();
    let manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: container,
        workspace_name,
        workspace_label: label,
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key: "role",
        role_display_name: "Role",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "https://example.invalid/role.git",
        role_source_ref: None,
        image_tag: "image",
        docker: DockerResources::from_container_name(container),
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.write(state_dir).unwrap();
    let mut record = serde_json::to_value(sample_record()).unwrap();
    record.as_object_mut().unwrap().remove("workspace_name");
    record["workspace"] = label.into();
    record["container_name"] = container.into();
    let bytes =
        serde_json::to_vec(&serde_json::json!({"version": 1, "records": [record]})).unwrap();
    std::fs::write(isolation_file_path(state_dir), &bytes).unwrap();
    bytes
}

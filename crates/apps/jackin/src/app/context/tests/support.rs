// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn config_with_workspace(
    project_dir: &Path,
    allowed_roles: Vec<String>,
    last_role: Option<String>,
) -> AppConfig {
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        jackin_config::RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.roles.insert(
        "the-architect".to_owned(),
        jackin_config::RoleSource {
            git: "https://github.com/jackin-project/jackin-the-architect.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "my-app".to_owned(),
        WorkspaceConfig {
            version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: "/workspace".to_owned(),
            mounts: vec![workspace::MountConfig {
                src: project_dir.display().to_string(),
                dst: "/workspace".to_owned(),
                readonly: false,
                isolation: jackin_core::MountIsolation::Shared,
            }],
            allowed_roles,
            default_role: None,
            default_agent: None,
            last_role,
            env: std::collections::BTreeMap::new(),
            roles: std::collections::BTreeMap::new(),
            keep_awake: workspace::KeepAwakeConfig::default(),
            accounts: Vec::new(),
            account_bindings: std::collections::BTreeMap::new(),
            github: None,
            git_pull_on_entry: false,
            runtime: jackin_config::WorkspaceRuntimeConfig::default(),
            dirty_exit_policy: None,
            docker: None,
            default_launch: None,
        },
    );
    config
}

pub(super) fn fake_docker_with_running_agents(
    names: &[&str],
) -> jackin_test_support::FakeDockerClient {
    use jackin_docker::docker_client::ContainerRow;
    let rows: Vec<ContainerRow> = names
        .iter()
        .map(|name| ContainerRow {
            name: name.to_string(),
            id: "container-id".to_owned(),
            labels: std::collections::HashMap::default(),
        })
        .collect();
    jackin_test_support::FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([rows])),
        ..Default::default()
    }
}

pub(super) fn persisted_config_with_workspace(paths: &JackinPaths, temp_path: &Path) -> AppConfig {
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    config.workspaces.insert(
        "my-app".to_owned(),
        WorkspaceConfig {
            version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
            workdir: "/workspace".to_owned(),
            mounts: vec![workspace::MountConfig {
                src: temp_path.display().to_string(),
                dst: "/workspace".to_owned(),
                readonly: false,
                isolation: jackin_core::MountIsolation::Shared,
            }],
            ..Default::default()
        },
    );
    let serialized = toml::to_string_pretty(&config).unwrap();
    std::fs::write(&paths.config_file, serialized).unwrap();
    config
}

pub(super) fn write_role_manifest(role_dir: &Path, body: &str) {
    std::fs::create_dir_all(role_dir).unwrap();
    std::fs::write(role_dir.join("jackin.role.toml"), body).unwrap();
}

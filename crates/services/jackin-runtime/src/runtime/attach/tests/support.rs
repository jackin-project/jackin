// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn test_paths() -> (TempDir, JackinPaths) {
    let dir = TempDir::new().unwrap();
    let paths = JackinPaths::for_tests(dir.path());
    (dir, paths)
}

pub(super) fn short_test_paths() -> (TempDir, JackinPaths) {
    let dir = tempfile::Builder::new()
        .prefix("jk-attach-")
        .tempdir_in("/tmp")
        .unwrap();
    let paths = JackinPaths::for_tests(dir.path());
    (dir, paths)
}

pub(super) fn test_container_handle(name: &str) -> ContainerHandle {
    ContainerHandle::new(name, format!("{name}-id")).unwrap()
}

pub(super) fn spawn_capsule_preface_ack(
    listener: std::os::unix::net::UnixListener,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        stream.set_nonblocking(true).unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let mut stream = tokio::net::UnixStream::from_std(stream).unwrap();
                jackin_protocol::capsule_transport::server_handshake_async(&mut stream)
                    .await
                    .unwrap();
            });
    })
}

pub(super) type ScheduledConfigRotation = (String, PathBuf, Vec<u8>);

pub(super) fn config_rotation_slot() -> &'static Mutex<Vec<ScheduledConfigRotation>> {
    static SLOT: OnceLock<Mutex<Vec<ScheduledConfigRotation>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(Vec::new()))
}

pub(super) struct ConfigRotationGuard {
    operation_prefix: String,
    path: PathBuf,
}

pub(super) fn schedule_config_rotation(
    paths: &JackinPaths,
    operation_prefix: impl Into<String>,
    config: &jackin_config::AppConfig,
) -> ConfigRotationGuard {
    let operation_prefix = operation_prefix.into();
    let path = paths.config_file.clone();
    config_rotation_slot().lock().unwrap().push((
        operation_prefix.clone(),
        path.clone(),
        toml::to_string(config).unwrap().into_bytes(),
    ));
    ConfigRotationGuard {
        operation_prefix,
        path,
    }
}

pub(super) fn rotate_config_on_operation(operation: &str) {
    let scheduled = {
        let mut slot = config_rotation_slot().lock().unwrap();
        let position = slot.iter().position(|(prefix, _, _)| {
            operation == prefix || operation.starts_with(&format!("{prefix} "))
        });
        position.map(|position| slot.remove(position))
    };
    if let Some((_, path, bytes)) = scheduled {
        std::fs::write(path, bytes).unwrap();
    }
}

impl Drop for ConfigRotationGuard {
    fn drop(&mut self) {
        config_rotation_slot()
            .lock()
            .unwrap()
            .retain(|(prefix, path, _)| prefix != &self.operation_prefix || path != &self.path);
    }
}

pub(super) fn provision_account_admission(paths: &JackinPaths, container_name: &str) {
    let config = jackin_config::AppConfig::default();
    write_admission_fixture(paths, container_name, &config, None, &[]);
}

pub(super) fn provision_agent_admission(
    paths: &JackinPaths,
    container_name: &str,
    agent: jackin_core::Agent,
) {
    use jackin_config::{AccountConfig, AccountCredential, AgentConfiguration, AiProvider};

    let account_id = "fixture-account";
    let config_id = format!("{}-fixture", agent.slug());
    let mut config = jackin_config::AppConfig::default();
    config.accounts.insert(
        account_id.into(),
        AccountConfig {
            enabled: true,
            name: "Fixture account".into(),
            provider: AiProvider::for_agent(agent).expect("fixture agent has a provider"),
            credential: AccountCredential::ApiKey {
                value: "fixture-key".into(),
                base_url: None,
                model: None,
            },
        },
    );
    config.agent_configurations.insert(
        config_id.clone(),
        AgentConfiguration {
            agent,
            account: account_id.into(),
            model: None,
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
        },
    );
    let admitted = [crate::instance::AdmittedInstance::new(
        config_id, agent, account_id,
    )];
    write_admission_fixture(paths, container_name, &config, None, &admitted);
}

pub(super) fn write_admission_fixture(
    paths: &JackinPaths,
    container_name: &str,
    config: &jackin_config::AppConfig,
    workspace: Option<&str>,
    admitted: &[crate::instance::AdmittedInstance],
) {
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::write(
        paths.config_dir.join("config.toml"),
        toml::to_string(config).unwrap(),
    )
    .unwrap();
    std::fs::File::create(paths.config_file.with_file_name("config.lock")).unwrap();
    let snapshot = jackin_config::load_read_only_config_snapshot(paths).unwrap();
    assert!(
        snapshot.diagnostics.is_empty(),
        "invalid admission fixture: {:?}",
        snapshot.diagnostics
    );
    let root = paths.data_dir.join(container_name);
    std::fs::create_dir_all(&root).unwrap();
    let mut manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
        container_base: container_name,
        workspace_name: workspace,
        workspace_label: "test",
        workdir: "/workspace",
        host_workdir_fingerprint: "fixture",
        role_key: "agent-smith",
        role_display_name: "Agent Smith",
        agent_runtime: jackin_core::Agent::Claude,
        role_source_git: "",
        role_source_ref: None,
        image_tag: "fixture",
        docker: crate::instance::DockerResources {
            role_container: container_name.into(),
            dind_container: Some(format!("{container_name}-dind")),
            network: format!("{container_name}-net"),
            certs_volume: Some(format!("{container_name}-dind-certs")),
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    });
    manifest.docker_identity = Some(crate::instance::DockerIdentity {
        role_container_id: container_name.to_owned(),
        dind_container_id: Some(format!("{container_name}-dind")),
    });
    manifest.set_admitted_instances(admitted.iter().cloned());
    manifest.write(&root).unwrap();
    let workspace = workspace
        .map(jackin_core::WorkspaceName::parse)
        .transpose()
        .unwrap();
    let digest = launch::account_configuration_fingerprint(
        config,
        workspace.as_ref(),
        "agent-smith",
        &manifest.admitted_instances,
    )
    .unwrap();
    std::fs::write(root.join("account-admission.sha256"), digest).unwrap();
}

pub(super) fn provision_duplicate_agent_admission(
    paths: &JackinPaths,
    container_name: &str,
) -> jackin_config::AppConfig {
    use jackin_config::{AccountConfig, AccountCredential, AgentConfiguration, AiProvider};

    let mut config = jackin_config::AppConfig::default();
    for (id, name) in [("work", "Work"), ("personal", "Personal")] {
        config.accounts.insert(
            id.into(),
            AccountConfig {
                enabled: true,
                name: name.into(),
                provider: AiProvider::Anthropic,
                credential: AccountCredential::ApiKey {
                    value: format!("{id}-key").into(),
                    base_url: None,
                    model: None,
                },
            },
        );
    }
    for (id, account) in [("claude-work", "work"), ("claude-personal", "personal")] {
        config.agent_configurations.insert(
            id.into(),
            AgentConfiguration {
                agent: jackin_core::Agent::Claude,
                account: account.into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
    }
    let admitted = [
        crate::instance::AdmittedInstance::new("claude-work", jackin_core::Agent::Claude, "work"),
        crate::instance::AdmittedInstance::new(
            "claude-personal",
            jackin_core::Agent::Claude,
            "personal",
        ),
    ];
    write_admission_fixture(paths, container_name, &config, None, &admitted);
    config
}

pub(super) fn ensure_socket_parent(paths: &JackinPaths, container_name: &str) -> PathBuf {
    let socket_path = snapshot::socket_path(paths, container_name);
    std::fs::create_dir_all(socket_path.parent().unwrap()).unwrap();
    socket_path
}

pub(super) fn pending_entry_count(paths: &JackinPaths) -> usize {
    std::fs::read_dir(
        crate::runtime::coordination::universe_dir(paths)
            .unwrap()
            .join("universe-pending"),
    )
    .map_or(0, Iterator::count)
}

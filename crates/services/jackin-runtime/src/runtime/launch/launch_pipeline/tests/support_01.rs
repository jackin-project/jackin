// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const INTEGRATED_LAUNCH_WIRE_CHILD: &str = "JACKIN_INTEGRATED_LAUNCH_WIRE_CHILD";

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
    config: &AppConfig,
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

pub(super) type ScheduledIsolationCorruption = (String, PathBuf);

pub(super) fn isolation_corruption_slot() -> &'static Mutex<Vec<ScheduledIsolationCorruption>> {
    static SLOT: OnceLock<Mutex<Vec<ScheduledIsolationCorruption>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(Vec::new()))
}

pub(super) struct IsolationCorruptionGuard {
    operation: String,
    path: PathBuf,
}

pub(super) fn schedule_isolation_corruption(
    operation: impl Into<String>,
    path: PathBuf,
) -> IsolationCorruptionGuard {
    let operation = operation.into();
    isolation_corruption_slot()
        .lock()
        .unwrap()
        .push((operation.clone(), path.clone()));
    IsolationCorruptionGuard { operation, path }
}

pub(super) fn corrupt_isolation_on_operation(operation: &str) {
    let scheduled = {
        let mut slot = isolation_corruption_slot().lock().unwrap();
        let position = slot.iter().position(|(expected, _)| operation == expected);
        position.map(|position| slot.remove(position))
    };
    if let Some((_, path)) = scheduled {
        std::fs::write(path, r#"{"version":999,"records":[]}"#).unwrap();
    }
}

impl Drop for IsolationCorruptionGuard {
    fn drop(&mut self) {
        isolation_corruption_slot()
            .lock()
            .unwrap()
            .retain(|(operation, path)| operation != &self.operation || path != &self.path);
    }
}

pub(super) fn observe_launch_process(command: &str) {
    let program = command.split_whitespace().next().unwrap_or("unknown");
    let operation = jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::PROCESS_COMMAND,
        &[jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXECUTABLE_NAME,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::process::classify_executable(std::path::Path::new(program))
                    .as_str(),
            ),
        }],
    );
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
}

pub(super) fn observe_launch_docker(operation_name: &str) {
    let (method, template) = if operation_name.starts_with("docker inspect image:") {
        ("GET", "/images/{name}/json")
    } else if operation_name.starts_with("docker inspect ") {
        ("GET", "/containers/{id}/json")
    } else if operation_name.starts_with("docker ps") {
        ("GET", "/containers/json")
    } else if operation_name.starts_with("create_container:") {
        ("POST", "/containers/create")
    } else if operation_name.starts_with("start_container:") {
        ("POST", "/containers/{id}/start")
    } else if operation_name.starts_with("docker exec ") {
        ("POST", "/exec/{id}/start")
    } else if operation_name.starts_with("docker network create ") {
        ("POST", "/networks/create")
    } else if operation_name.starts_with("docker network inspect ") {
        ("GET", "/networks/{id}")
    } else if operation_name.starts_with("docker network ls") {
        ("GET", "/networks")
    } else if operation_name.starts_with("docker network rm ") {
        ("DELETE", "/networks/{id}")
    } else if operation_name.starts_with("docker pull ") {
        ("POST", "/images/create")
    } else if operation_name.starts_with("docker rmi ") {
        ("DELETE", "/images/{name}")
    } else if operation_name.starts_with("docker volume rm ") {
        ("DELETE", "/volumes/{name}")
    } else if operation_name.starts_with("docker rm ") {
        ("DELETE", "/containers/{id}")
    } else {
        ("GET", "/_ping")
    };
    let operation = jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::HTTP_CLIENT,
        &[
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::HTTP_REQUEST_METHOD,
                value: jackin_telemetry::Value::Str(method),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::URL_TEMPLATE,
                value: jackin_telemetry::Value::Str(template),
            },
        ],
    );
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
}

pub(super) struct LaunchCoreFixture {
    _temp: TempDir,
    pub(super) paths: JackinPaths,
    pub(super) config: AppConfig,
    pub(super) selector: RoleSelector,
    pub(super) workspace: jackin_config::ResolvedWorkspace,
    pub(super) docker: FakeDockerClient,
    pub(super) runner: FakeRunner,
    steps: StepCounter,
    pub(super) opts: super::super::super::LoadOptions,
    cached_repo: jackin_manifest::repo::CachedRepo,
    validated_repo: jackin_manifest::repo::ValidatedRoleRepo,
    source: jackin_config::RoleSource,
    pub(super) container_name: String,
    pub(super) image: String,
    supported_agents: Vec<Agent>,
    restore_instances: Option<Vec<jackin_config::ResolvedInstance>>,
}

impl LaunchCoreFixture {
    pub(super) fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        crate::runtime::stubs::install_all_test_stubs(&paths);
        paths.ensure_base_dirs().unwrap();

        let selector = RoleSelector::new(None, "agent-smith");
        let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
        seed_valid_role_repo(&cached_repo.repo_dir);
        std::fs::write(
            cached_repo.repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
        )
        .unwrap();
        let validated_repo =
            jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();

        let mut config = AppConfig::load_or_init(&paths).unwrap();
        config.accounts.insert(
            "test-codex".into(),
            jackin_config::AccountConfig {
                enabled: true,
                name: "Test".into(),
                provider: jackin_config::AiProvider::OpenAi,
                credential: jackin_config::AccountCredential::ApiKey {
                    value: "test-key".into(),
                    base_url: None,
                    model: None,
                },
            },
        );
        config.agent_configurations.insert(
            "codex-main".into(),
            jackin_config::AgentConfiguration {
                agent: Agent::Codex,
                account: "test-codex".into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
        config.default_launch = Some(vec!["codex-main".into()]);
        std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();
        let workspace = jackin_config::ResolvedWorkspace {
            name: String::new(),
            label: cached_repo.repo_dir.display().to_string(),
            workdir: "/workspace".to_owned(),
            mounts: vec![jackin_config::MountConfig {
                src: cached_repo.repo_dir.display().to_string(),
                dst: "/workspace".to_owned(),
                readonly: false,
                isolation: jackin_config::MountIsolation::Shared,
            }],
            default_agent: None,
            keep_awake_enabled: false,
            git_pull_on_entry: false,
            mount_heal: jackin_config::MountHealReport::default(),
        };

        let container_name = "jk-harness-agentsmith".to_owned();
        let image = "jk_agent-smith:harness".to_owned();

        let docker = FakeDockerClient {
            exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
                String::new(),
                String::new(),
                "Sessions: 1\n".to_owned(),
                "Sessions: 0\n".to_owned(),
            ])),
            ..Default::default()
        };

        Self {
            _temp: temp,
            paths,
            config,
            selector,
            workspace,
            docker,
            runner: FakeRunner::default(),
            steps: StepCounter::new(
                "agent-smith",
                jackin_telemetry::schema::enums::LaunchTargetKind::Directory,
            ),
            opts: super::super::super::LoadOptions {
                agent: Some(Agent::Codex),
                ..Default::default()
            },
            cached_repo,
            validated_repo,
            source: jackin_config::RoleSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
                trusted: true,
                env: BTreeMap::new(),
            },
            container_name,
            image,
            supported_agents: vec![Agent::Codex],
            restore_instances: None,
        }
    }

    pub(super) fn configure_account_selection_case(&mut self) {
        std::fs::write(
            self.cached_repo.repo_dir.join("jackin.role.toml"),
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex", "claude"]

[codex]

[claude]
"#,
        )
        .unwrap();
        self.validated_repo =
            jackin_manifest::repo::validate_role_repo(&self.cached_repo.repo_dir).unwrap();
        self.supported_agents = vec![Agent::Codex, Agent::Claude];

        if let Some(jackin_config::AccountCredential::ApiKey { value, .. }) = self
            .config
            .accounts
            .get_mut("test-codex")
            .map(|account| &mut account.credential)
        {
            *value = "saved-codex-key-a".into();
        }
        self.config.accounts.insert(
            "saved-claude-account".into(),
            jackin_config::AccountConfig {
                enabled: true,
                name: "Saved Claude".into(),
                provider: jackin_config::AiProvider::Anthropic,
                credential: jackin_config::AccountCredential::ApiKey {
                    value: "saved-claude-key-a".into(),
                    base_url: None,
                    model: None,
                },
            },
        );
        self.config.agent_configurations.insert(
            "claude-saved".into(),
            jackin_config::AgentConfiguration {
                agent: Agent::Claude,
                account: "saved-claude-account".into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
        self.config.accounts.insert(
            "default-codex-account".into(),
            jackin_config::AccountConfig {
                enabled: true,
                name: "Default Codex".into(),
                provider: jackin_config::AiProvider::OpenAi,
                credential: jackin_config::AccountCredential::ApiKey {
                    value: "default-codex-key-b".into(),
                    base_url: None,
                    model: None,
                },
            },
        );
        self.config.agent_configurations.insert(
            "codex-default-b".into(),
            jackin_config::AgentConfiguration {
                agent: Agent::Codex,
                account: "default-codex-account".into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
        self.config.default_launch = Some(vec!["codex-default-b".into()]);
        std::fs::write(
            &self.paths.config_file,
            toml::to_string(&self.config).unwrap(),
        )
        .unwrap();
    }

    pub(super) fn admit_saved_account_selection(&mut self) {
        let selected = vec!["codex-main".to_owned(), "claude-saved".to_owned()];
        self.restore_instances = Some(
            jackin_config::resolve_launch(
                &self.config,
                None,
                &self.selector.key(),
                Some(&selected),
                None,
            )
            .unwrap(),
        );
    }

    pub(super) fn with_bad_grants(mut self) -> Self {
        self.config.docker.grants = Some(DockerGrants {
            user: Some("root".to_owned()),
            sudo: Some(true),
            ..Default::default()
        });
        self
    }

    pub(super) fn plant_valid_isolation_for_finalize_error(&self) -> PathBuf {
        let state = self.paths.data_dir.join(&self.container_name);
        let iso_dir = state.join(".jackin");
        std::fs::create_dir_all(&iso_dir).unwrap();
        let path = iso_dir.join("isolation.json");
        std::fs::write(&path, r#"{"version":2,"records":[]}"#).unwrap();
        path
    }

    pub(super) fn as_core(&mut self) -> LaunchCore<'_, FakeDockerClient, FakeRunner> {
        let account_revision =
            account_identity::AccountConfigRevision::acquire(&self.paths).unwrap();
        let admission_config = self.config.clone();
        LaunchCore {
            paths: &self.paths,
            config: &mut self.config,
            selector: &self.selector,
            workspace: &self.workspace,
            docker: &self.docker,
            runner: &mut self.runner,
            opts: &self.opts,
            git: GitIdentity::for_tests("Harness", "harness@example.invalid"),
            workspace_name: None,
            steps: &mut self.steps,
            role_key: self.selector.key(),
            agent_display_name: "Agent Smith".to_owned(),
            agent: Agent::Codex,
            supported_agents: self.supported_agents.clone(),
            cached_repo: self.cached_repo.clone(),
            validated_repo: self.validated_repo.clone(),
            source: self.source.clone(),
            auth_mode: jackin_core::AuthForwardMode::Ignore,
            backend: Backend::Docker,
            image_decision: ImageDecision::Reuse {
                image: self.image.clone(),
            },
            repo_lock: None,
            restoring: false,
            container_name: self.container_name.clone(),
            exec_bindings: Vec::new(),
            recipe_role_git_sha: None,
            recipe_base_image_ref: None,
            selected_refresh_reason: None,
            resolved_env: ResolvedEnv { vars: vec![] },
            rebuild: false,
            restore_pinned_sha: None,
            restore_instances: self.restore_instances.clone(),
            git_pull_join: None,
            account_revision,
            admission_config,
        }
    }
}

#[tokio::test]
async fn saved_restore_selection_drives_credentials_and_capsule_config() {
    let mut fix = LaunchCoreFixture::new();
    fix.configure_account_selection_case();
    fix.admit_saved_account_selection();

    launch_core::run_launch_core(fix.as_core())
        .await
        .expect("saved account selection launches");

    assert_staged_instance_account(&fix, "codex-main", "test-codex", "saved-codex-key-a");
    assert_staged_instance_account(
        &fix,
        "claude-saved",
        "saved-claude-account",
        "saved-claude-key-a",
    );
    assert_staged_instance_is_absent(&fix, "codex-default-b");

    let capsule = read_capsule_config(&fix);
    assert_eq!(
        capsule.instances,
        vec!["codex-main".to_owned(), "claude-saved".to_owned()],
        "CapsuleConfig must keep the saved mixed-agent order"
    );
    assert_eq!(
        capsule.accounts.get("codex-main").map(String::as_str),
        Some("test-codex")
    );
    assert_eq!(
        capsule.accounts.get("claude-saved").map(String::as_str),
        Some("saved-claude-account")
    );
    assert_eq!(
        capsule.agents.get("codex-main").map(String::as_str),
        Some("codex")
    );
    assert_eq!(
        capsule.agents.get("claude-saved").map(String::as_str),
        Some("claude")
    );
    assert!(
        !capsule
            .instances
            .iter()
            .any(|instance| instance == "codex-default-b")
    );
}

#[tokio::test]
async fn fresh_launch_uses_current_default_for_credentials_and_capsule_config() {
    let mut fix = LaunchCoreFixture::new();
    fix.configure_account_selection_case();

    launch_core::run_launch_core(fix.as_core())
        .await
        .expect("fresh account selection launches");

    assert_staged_instance_account(
        &fix,
        "codex-default-b",
        "default-codex-account",
        "default-codex-key-b",
    );
    assert_staged_instance_is_absent(&fix, "codex-main");
    assert_staged_instance_is_absent(&fix, "claude-saved");

    let capsule = read_capsule_config(&fix);
    assert_eq!(capsule.instances, vec!["codex-default-b".to_owned()]);
    assert_eq!(
        capsule.accounts.get("codex-default-b").map(String::as_str),
        Some("default-codex-account")
    );
}

fn assert_staged_instance_account(
    fix: &LaunchCoreFixture,
    instance: &str,
    account_id: &str,
    credential_marker: &str,
) {
    let path = fix
        .paths
        .data_dir
        .join(&fix.container_name)
        .join("credentials")
        .join(jackin_protocol::account_credentials_filename(instance));
    let staged: jackin_protocol::StagedInstanceCredential =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(staged.instance, instance);
    assert_eq!(staged.credential.account_id, account_id);
    assert!(
        staged
            .credential
            .env
            .values()
            .any(|value| value == credential_marker)
    );
}

fn assert_staged_instance_is_absent(fix: &LaunchCoreFixture, instance: &str) {
    let path = fix
        .paths
        .data_dir
        .join(&fix.container_name)
        .join("credentials")
        .join(jackin_protocol::account_credentials_filename(instance));
    assert!(
        !path.exists(),
        "unexpected staged credential for {instance}"
    );
}

fn read_capsule_config(fix: &LaunchCoreFixture) -> jackin_protocol::CapsuleConfig {
    let path = fix
        .paths
        .jackin_home
        .join("sockets")
        .join(&fix.container_name)
        .join(jackin_protocol::CAPSULE_CONFIG_FILENAME);
    toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

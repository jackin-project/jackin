// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn load_agent_fixture(
    manifest_body: &str,
    admission_toml: Option<&str>,
) -> LoadAgentFixture {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        format!(
            r#"{}[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
            admission_toml.unwrap_or_default()
        ),
    )
    .unwrap();
    let config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let runner = FakeRunner::for_load_agent([String::new()]);
    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(repo_dir.join("jackin.role.toml"), manifest_body).unwrap();
    let workspace = repo_workspace(&repo_dir);
    LoadAgentFixture {
        _temp: temp,
        paths,
        config,
        selector,
        runner,
        workspace,
        docker: jackin_test_support::FakeDockerClient::default(),
    }
}

pub(super) struct ConsoleResolutionFixture {
    _temp: tempfile::TempDir,
    pub(super) paths: JackinPaths,
    pub(super) selector: RoleSelector,
    pub(super) repo_dir: PathBuf,
    pub(super) config: AppConfig,
    pub(super) runner: FakeRunner,
}

pub(super) const MULTI_AGENT_MANIFEST: &str = r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#;

pub(super) const CODEX_ONLY_MANIFEST: &str = r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#;

pub(super) fn console_resolution_fixture() -> ConsoleResolutionFixture {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        jackin_config::RoleSource {
            git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    ConsoleResolutionFixture {
        _temp: temp,
        paths,
        selector,
        repo_dir,
        config,
        runner: FakeRunner::default(),
    }
}

pub(super) fn write_role_repo(repo_dir: &Path, manifest: &str) {
    std::fs::create_dir_all(repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(repo_dir.join("jackin.role.toml"), manifest).unwrap();
}

pub(super) fn seed_cached_repo(repo_dir: &Path, manifest: &str) {
    write_role_repo(repo_dir, manifest);
    std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
}

pub(super) fn materialize_on_clone(runner: &mut FakeRunner, repo_dir: PathBuf, manifest: String) {
    runner.side_effects.push((
        "clone".to_owned(),
        Box::new(move || {
            write_role_repo(&repo_dir, &manifest);
            std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        }),
    ));
}

pub(super) fn task_override_current_role_fixture(
    state: ContainerState,
) -> (
    tempfile::TempDir,
    JackinPaths,
    AppConfig,
    RoleSelector,
    jackin_config::ResolvedWorkspace,
    jackin_test_support::FakeDockerClient,
    FakeRunner,
    String,
) {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, CODEX_ADMISSION_TOML).unwrap();
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    std::fs::create_dir_all(&cached_repo.repo_dir).unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        "version = \"v1alpha5\"\ndockerfile = \"Dockerfile\"\nagents = [\"codex\"]\n\n[codex]\nmodel = \"role-default\"\n",
    )
    .unwrap();
    config.workspaces.insert(
        "workspace".to_owned(),
        jackin_config::WorkspaceConfig {
            accounts: vec!["test".to_owned()],
            workdir: "/workspace".to_owned(),
            mounts: repo_workspace(&cached_repo.repo_dir).mounts,
            default_agent: Some(jackin_core::Agent::Codex),
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    persist_test_config(&paths, &config);

    let current_container = "jk-k7p9m2xq-workspace-agentsmith".to_owned();
    let mut manifest = workspace_manifest(
        &current_container,
        "agent-smith",
        "Agent Smith",
        jackin_core::Agent::Codex,
    );
    manifest.mark_status(InstanceStatus::Running);
    manifest.docker_identity = Some(crate::instance::DockerIdentity {
        role_container_id: current_container.clone(),
        dind_container_id: manifest.docker.dind_container.clone(),
    });
    write_indexed_manifest(&paths, &manifest);
    provision_restore_account_policy(&paths, &config, &manifest);

    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .container_id_by_name
        .borrow_mut()
        .insert(current_container.clone(), "current-role-id".to_owned());
    docker
        .inspect_state_by_name
        .borrow_mut()
        .insert(current_container.clone(), state);
    let runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
    ]);
    let mut workspace = repo_workspace(&cached_repo.repo_dir);
    workspace.label = "workspace".to_owned();
    workspace.name = "workspace".to_owned();
    workspace.default_agent = Some(jackin_core::Agent::Codex);
    (
        temp,
        paths,
        config,
        selector,
        workspace,
        docker,
        runner,
        current_container,
    )
}

pub(super) async fn task_overrides_launch_fresh_codex_for_current_state(state: ContainerState) {
    let preserve_existing = matches!(&state, ContainerState::Running);
    let (_temp, paths, mut config, selector, workspace, docker, mut runner, current_container) =
        task_override_current_role_fixture(state);
    let opts = LoadOptions {
        agent: Some(jackin_core::Agent::Codex),
        model: Some("gpt-6-luna".to_owned()),
        effort: Some(jackin_core::ReasoningEffort::Max),
        ..LoadOptions::default()
    };

    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();

    let calls = docker.recorded.borrow();
    assert!(
        !calls
            .iter()
            .any(|call| call == &format!("start_container:{current_container}")),
        "task-scoped overrides must not start the existing role; calls: {calls:?}"
    );
    if preserve_existing {
        assert!(
            !calls
                .iter()
                .any(|call| call == &format!("docker rm -f {current_container}")),
            "a fresh task-scoped launch must preserve the unrelated running session; calls: {calls:?}"
        );
        assert_eq!(
            docker
                .inspect_state_by_name
                .borrow()
                .get(&current_container),
            Some(&ContainerState::Running),
            "the unrelated running container must stay running"
        );
        assert_eq!(
            docker
                .container_id_by_name
                .borrow()
                .get(&current_container)
                .map(String::as_str),
            Some("current-role-id"),
            "the unrelated running container identity must remain registered"
        );
    }
    let recorded = runner.recorded.join("\n");
    assert!(
        recorded.contains("docker run -d")
            && !recorded
                .lines()
                .any(|line| { line.contains("docker exec") && line.contains(&current_container) }),
        "task-scoped overrides must pass through a fresh launch; recorded:\n{recorded}"
    );
    let launched_container = launched_role_container_name(&runner);
    let capsule_config_path = paths
        .jackin_home
        .join("sockets")
        .join(launched_container)
        .join(jackin_protocol::CAPSULE_CONFIG_FILENAME);
    let capsule_config: jackin_protocol::CapsuleConfig =
        toml::from_str(&std::fs::read_to_string(capsule_config_path).unwrap()).unwrap();
    assert_eq!(capsule_config.models["codex-main"], "gpt-6-luna");
    assert_eq!(capsule_config.efforts["codex-main"], "max");
}

pub(super) async fn run_load_with_env(
    entries: &[(&str, &str)],
) -> (String, String, tempfile::TempDir) {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    for (k, v) in entries {
        config.env.insert(
            (*k).to_owned(),
            jackin_core::EnvValue::Plain((*v).to_owned()),
        );
    }
    persist_test_config(&paths, &config);
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);
    let observed_env = observe_host_env_file(&mut runner, &paths);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &compat_dind_load_options(),
    )
    .await
    .unwrap();

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap()
        .clone();
    let observed = observed_env.lock().unwrap().clone().unwrap();
    assert!(!observed.path.exists());
    (run_cmd, observed.contents, temp)
}

pub(super) fn inspect_docker(state: ContainerState) -> jackin_test_support::FakeDockerClient {
    jackin_test_support::FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([state])),
        ..Default::default()
    }
}

pub(super) struct ConcurrentGithubOpRunner {
    pub(super) active: Arc<AtomicUsize>,
    pub(super) max_active: Arc<AtomicUsize>,
}

impl ConcurrentGithubOpRunner {
    fn record_active(&self, active: usize) {
        let mut observed = self.max_active.load(Ordering::SeqCst);
        while active > observed {
            match self.max_active.compare_exchange(
                observed,
                active,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return,
                Err(next) => observed = next,
            }
        }
    }
}

impl jackin_env::OpRunner for ConcurrentGithubOpRunner {
    fn read(&self, reference: &str) -> anyhow::Result<String> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.record_active(active);
        #[expect(
            clippy::disallowed_methods,
            reason = "test runner deliberately holds worker OS threads open to prove overlap"
        )]
        std::thread::sleep(std::time::Duration::from_millis(25));
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(format!("secret-for-{reference}"))
    }
}

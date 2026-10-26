// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::Backend;
use super::super::LoadOptions;
use super::super::account_identity;
use super::super::progress_helpers::StepCounter;
use super::breadcrumb_auth_mode;
use super::ensure_role_trust;
use super::launch_core::{self, LaunchCore};
use crate::runtime::identity::GitIdentity;
use crate::runtime::image::ImageDecision;
use jackin_config::AppConfig;
use jackin_config::RoleSource;
use jackin_core::Agent;
use jackin_core::AuthForwardMode;
use jackin_core::ContainerState;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_env::ResolvedEnv;
use jackin_test_support::{FakeDockerClient, FakeRunner, seed_valid_role_repo};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;

const FIXTURE_CODEX_CONFIG: &str = r#"default_launch = ["codex-main"]

[accounts.test-codex]
name = "Test"
provider = "openai"
[accounts.test-codex.credential]
type = "api_key"
value = "test-key"

[agent_configurations.codex-main]
agent = "codex"
account = "test-codex"
"#;

const BAD_GRANTS_CODEX_CONFIG: &str = r#"default_launch = ["codex-main"]

[accounts.test-codex]
name = "Test"
provider = "openai"
[accounts.test-codex.credential]
type = "api_key"
value = "test-key"

[agent_configurations.codex-main]
agent = "codex"
account = "test-codex"

[docker.grants]
user = "root"
sudo = true
"#;

fn install_test_stubs(paths: &JackinPaths) {
    for agent in [
        Agent::Claude,
        Agent::Codex,
        Agent::Amp,
        Agent::Kimi,
        Agent::Opencode,
    ] {
        jackin_image::agent_binary::install_test_stub(paths, agent)
            .expect("install agent binary test stub");
    }
    jackin_image::capsule_binary::install_test_stub(paths).expect("install capsule test stub");
}

type ScheduledIsolationCorruption = (String, PathBuf);

fn isolation_corruption_slot() -> &'static Mutex<Vec<ScheduledIsolationCorruption>> {
    static SLOT: OnceLock<Mutex<Vec<ScheduledIsolationCorruption>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(Vec::new()))
}

struct IsolationCorruptionGuard {
    operation: String,
    path: PathBuf,
}

fn schedule_isolation_corruption(
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

fn corrupt_isolation_on_operation(operation: &str) {
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

struct LaunchCoreFixture {
    _temp: TempDir,
    paths: JackinPaths,
    config: AppConfig,
    selector: RoleSelector,
    workspace: jackin_config::ResolvedWorkspace,
    docker: FakeDockerClient,
    runner: FakeRunner,
    steps: StepCounter,
    opts: LoadOptions,
    cached_repo: jackin_manifest::repo::CachedRepo,
    validated_repo: jackin_manifest::repo::ValidatedRoleRepo,
    source: RoleSource,
    container_name: String,
    image: String,
    supported_agents: Vec<Agent>,
    restore_instances: Option<Vec<jackin_config::ResolvedInstance>>,
}

impl LaunchCoreFixture {
    fn new() -> Self {
        Self::new_with_config(FIXTURE_CODEX_CONFIG)
    }

    fn with_bad_grants() -> Self {
        Self::new_with_config(BAD_GRANTS_CODEX_CONFIG)
    }

    fn new_with_config(config_toml: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        install_test_stubs(&paths);
        paths.ensure_base_dirs().unwrap();

        let selector = RoleSelector::new(None, "agent-smith");
        let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
        seed_valid_role_repo(&cached_repo.repo_dir);
        std::fs::write(
            cached_repo.repo_dir.join("jackin.role.toml"),
            "version = \"v1alpha3\"\ndockerfile = \"Dockerfile\"\nagents = [\"codex\"]\n\n[codex]\n",
        )
        .unwrap();
        let validated_repo =
            jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();

        std::fs::write(&paths.config_file, config_toml).unwrap();
        let config = AppConfig::load_or_init(&paths).unwrap();
        let workspace = jackin_config::ResolvedWorkspace {
            name: String::new(),
            label: cached_repo.repo_dir.display().to_string(),
            workdir: "/workspace".to_owned(),
            mounts: vec![jackin_config::MountConfig {
                src: cached_repo.repo_dir.display().to_string(),
                dst: "/workspace".to_owned(),
                readonly: false,
                isolation: jackin_core::MountIsolation::Shared,
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
            opts: LoadOptions {
                agent: Some(Agent::Codex),
                ..Default::default()
            },
            cached_repo,
            validated_repo,
            source: RoleSource {
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

    fn plant_valid_isolation_for_finalize_error(&self) -> PathBuf {
        let state = self.paths.data_dir.join(&self.container_name);
        let iso_dir = state.join(".jackin");
        std::fs::create_dir_all(&iso_dir).unwrap();
        let path = iso_dir.join("isolation.json");
        std::fs::write(&path, r#"{"version":2,"records":[]}"#).unwrap();
        path
    }

    fn as_core(&mut self) -> LaunchCore<'_, FakeDockerClient, FakeRunner> {
        let account_revision = account_identity::AccountConfigRevision::acquire(&self.paths)
            .expect("fixture acquires account revision");
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
            auth_mode: AuthForwardMode::Ignore,
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

fn untrusted_source() -> RoleSource {
    RoleSource {
        git: "https://example.com/role.git".to_owned(),
        trusted: false,
        env: BTreeMap::new(),
    }
}

fn test_steps() -> StepCounter {
    StepCounter::new(
        "loadtest",
        jackin_telemetry::schema::enums::LaunchTargetKind::Workspace,
    )
}

#[test]
fn ensure_role_trust_marks_source_trusted_on_accept() -> anyhow::Result<()> {
    let mut config = AppConfig::default();
    let selector = RoleSelector::parse("loadtest")?;
    config.roles.insert(selector.key(), untrusted_source());
    let mut steps = test_steps();
    let accepted = ensure_role_trust(
        &mut config,
        &selector,
        &untrusted_source(),
        &mut steps,
        |_, _| Ok(()),
    )?;
    assert!(accepted);
    assert!(
        config
            .roles
            .get(&selector.key())
            .is_some_and(|role| role.trusted)
    );

    let mut confirmed_config = AppConfig::default();
    let mut confirmed_steps = test_steps();
    let skipped = ensure_role_trust(
        &mut confirmed_config,
        &selector,
        &RoleSource {
            trusted: true,
            ..untrusted_source()
        },
        &mut confirmed_steps,
        |_, _| anyhow::bail!("confirmation must not run for trusted sources"),
    )?;
    assert!(!skipped);
    Ok(())
}

#[test]
fn ensure_role_trust_aborts_when_confirmation_fails() {
    let mut config = AppConfig::default();
    let selector = RoleSelector::parse("loadtest").expect("selector parses");
    let mut steps = test_steps();
    let declined = ensure_role_trust(
        &mut config,
        &selector,
        &untrusted_source(),
        &mut steps,
        |_, _| anyhow::bail!("operator declined trust"),
    );
    assert!(declined.is_err());
    assert!(
        declined
            .unwrap_err()
            .to_string()
            .contains("operator declined trust")
    );
}

#[test]
fn assigned_account_auth_mode_resolves_from_binding() -> anyhow::Result<()> {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "work".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "work".to_owned(),
            provider: jackin_config::AiProvider::Anthropic,
            credential: jackin_config::AccountCredential::ApiKey {
                value: jackin_core::EnvValue::Plain("test-key".to_owned()),
                base_url: None,
                model: None,
            },
        },
    );
    config
        .account_bindings
        .insert(Agent::Claude, "work".to_owned());
    assert_eq!(
        breadcrumb_auth_mode(&config, Agent::Claude, None, "loadtest")?,
        AuthForwardMode::ApiKey
    );
    assert_eq!(
        breadcrumb_auth_mode(&AppConfig::default(), Agent::Claude, None, "loadtest")?,
        AuthForwardMode::Ignore
    );
    Ok(())
}

#[tokio::test]
async fn run_launch_core_happy_path_returns_container_name() {
    let mut fix = LaunchCoreFixture::new();
    let core = fix.as_core();
    let future = launch_core::run_launch_core(core);
    let future_bytes = size_of_val(&future);
    assert!(
        future_bytes <= 8 * 1024,
        "launch core must bound propagated phase state to 8 KiB; future is {future_bytes} bytes"
    );
    let name = future.await.expect("happy path");
    assert_eq!(name, fix.container_name);
    let recorded = fix.docker.recorded.borrow();
    assert!(
        !recorded.is_empty(),
        "happy path must exercise Docker via FakeDocker; recorded empty"
    );
}

#[tokio::test]
async fn run_launch_core_suite_a_grant_failure_cleans_up_before_return() {
    let mut fix = LaunchCoreFixture::with_bad_grants();
    let core = fix.as_core();
    let err = launch_core::run_launch_core(core)
        .await
        .expect_err("root+sudo grants must fail suite A validation");
    assert!(
        err.to_string().contains("docker grants validation failed")
            || err.to_string().contains("grants"),
        "suite A must surface grant validation: {err}"
    );
    let recorded = fix.docker.recorded.borrow();
    assert!(
        !recorded
            .iter()
            .any(|c| c.contains("docker rm -f") || c.contains("docker network rm")),
        "grant validation before resource ownership must not issue Docker teardown by name: {recorded:?}"
    );
}

#[tokio::test]
async fn run_launch_core_finalize_error_runs_cleanup_before_return() {
    let mut fix = LaunchCoreFixture::new();
    let isolation_path = fix.plant_valid_isolation_for_finalize_error();
    let _corruption = schedule_isolation_corruption(
        format!("create_container:{}", fix.container_name),
        isolation_path,
    );
    fix.docker.operation_hook = Some(corrupt_isolation_on_operation);
    fix.docker.exec_capture_queue = std::cell::RefCell::new(VecDeque::from([
        String::new(),
        String::new(),
        "Sessions: 0\n".to_owned(),
        "Sessions: 0\n".to_owned(),
    ]));
    fix.docker.inspect_queue = std::cell::RefCell::new(VecDeque::from([
        ContainerState::Running,
        ContainerState::Running,
        ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        },
    ]));

    let core = fix.as_core();
    let err = launch_core::run_launch_core(core)
        .await
        .expect_err("corrupt isolation.json must fail finalization");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("version") || msg.contains("isolation") || msg.contains("Unsupported"),
        "error should name isolation/version failure: {msg}"
    );
    let recorded = fix.docker.recorded.borrow();
    assert!(
        recorded
            .iter()
            .any(|c| c.contains("docker rm -f") || c.contains("network rm")),
        "finalize error must run armed LoadCleanup before return; recorded: {recorded:?}"
    );
}

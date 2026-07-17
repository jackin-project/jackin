//! `run_launch_core` boundary harness (plan 016).
//!
//! Builds a fully-populated [`LaunchCore`] over `FakeDockerClient` /
//! `FakeRunner` + real grant/profile/config fixtures and drives the real
//! pipeline boundary (not helper-only substitutes).

use super::launch_core::{self, LaunchCore};
use super::*;
use crate::runtime::docker_profile::DockerGrants;
use crate::runtime::identity::GitIdentity;
use crate::runtime::image::ImageDecision;
use jackin_config::AppConfig;
use jackin_core::Agent;
use jackin_core::ContainerState;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_env::ResolvedEnv;
use jackin_test_support::{FakeDockerClient, FakeRunner, seed_valid_role_repo};
use std::collections::{BTreeMap, VecDeque};
use tempfile::TempDir;

const INTEGRATED_LAUNCH_WIRE_CHILD: &str = "JACKIN_INTEGRATED_LAUNCH_WIRE_CHILD";

/// Fully-populated `LaunchCore` fixture over fakes + real grant/profile config.
struct LaunchCoreFixture {
    _temp: TempDir,
    paths: JackinPaths,
    config: AppConfig,
    selector: RoleSelector,
    workspace: jackin_config::ResolvedWorkspace,
    docker: FakeDockerClient,
    runner: FakeRunner,
    steps: super::super::StepCounter,
    opts: super::super::LoadOptions,
    cached_repo: jackin_manifest::repo::CachedRepo,
    validated_repo: jackin_manifest::repo::ValidatedRoleRepo,
    source: jackin_config::RoleSource,
    container_name: String,
    image: String,
}

impl LaunchCoreFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        crate::runtime::test_support::install_all_test_stubs(&paths);
        paths.ensure_base_dirs().unwrap();

        let selector = RoleSelector::new(None, "agent-smith");
        let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
        seed_valid_role_repo(&cached_repo.repo_dir);
        // Codex-only role: single agent so load path needs no multi-agent dialog.
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

        let config = AppConfig::load_or_init(&paths).unwrap();
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
        };

        let container_name = "jk-harness-agentsmith".to_owned();
        let image = "jk_agent-smith:harness".to_owned();

        // Match launch suite `fake_docker_for_clean_attached_exit`: empty
        // inspect_queue (NotFound default is fine for pre-attach / post-exit
        // probes that tolerate missing containers) + session inventory probes.
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
            steps: super::super::StepCounter::new(
                "agent-smith",
                jackin_telemetry::schema::enums::LaunchTargetKind::Directory,
            ),
            opts: super::super::LoadOptions {
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
        }
    }

    fn with_bad_grants(mut self) -> Self {
        self.config.docker.grants = Some(DockerGrants {
            user: Some("root".to_owned()),
            sudo: Some(true),
            ..Default::default()
        });
        self
    }

    /// Corrupt isolation.json so post-success finalization fails while cleanup
    /// is still armed (proves cleanup-before-error at the pipeline boundary).
    fn plant_corrupt_isolation_for_finalize_error(&self) {
        let state = self.paths.data_dir.join(&self.container_name);
        let iso_dir = state.join(".jackin");
        std::fs::create_dir_all(&iso_dir).unwrap();
        std::fs::write(
            iso_dir.join("isolation.json"),
            r#"{"version":999,"records":[]}"#,
        )
        .unwrap();
    }

    fn as_core(&mut self) -> LaunchCore<'_, FakeDockerClient, FakeRunner> {
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
            supported_agents: vec![Agent::Codex],
            cached_repo: self.cached_repo.clone(),
            validated_repo: self.validated_repo.clone(),
            source: self.source.clone(),
            auth_mode: jackin_core::AuthForwardMode::Ignore,
            backend: super::super::Backend::Docker,
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
            operator_env: BTreeMap::new(),
            git_pull_join: None,
        }
    }
}

#[test]
fn launch_trust_rejection_exports_typed_decision_without_source_identity() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let selector = RoleSelector::new(Some("private-owner"), "private-role");
    let source = jackin_config::RoleSource {
        git: "https://secret.example/private-repository.git".to_owned(),
        trusted: false,
        env: BTreeMap::new(),
    };
    let mut config = AppConfig::default();
    config.roles.insert(selector.key(), source.clone());
    let mut steps = super::super::StepCounter::new(
        "private-role",
        jackin_telemetry::schema::enums::LaunchTargetKind::Directory,
    );

    let result = ensure_role_trust(&mut config, &selector, &source, &mut steps, |_, _| {
        anyhow::bail!("private prompt transport failure")
    });
    result.unwrap_err();

    export.force_flush();
    assert_eq!(export.event_count("trust.decision"), 1);
    for expected in ["rejected", "external", "failure", "trust_error"] {
        assert!(export.contains_log_text(expected));
    }
    for private in [
        "private-owner",
        "private-role",
        "secret.example",
        "private-repository",
        "private prompt transport failure",
    ] {
        assert!(!export.contains_log_text(private));
    }
}

#[test]
fn persisted_launch_trust_grant_exports_success_without_error_type() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        emit_launch_trust_decision(
            jackin_telemetry::schema::enums::TrustDecision::Granted,
            None,
        );
    });

    export.force_flush();
    assert_eq!(export.event_count("trust.decision"), 1);
    for expected in ["granted", "external", "success"] {
        assert!(export.contains_log_text(expected));
    }
    assert!(!export.contains_log_text("error.type"));
}

#[test]
fn tag_errors_prefixes_each_with_source_tag() {
    let out = tag_errors("workspace", vec!["root+sudo", "bad pids"]);
    assert_eq!(
        out,
        [
            "  - [workspace] root+sudo".to_owned(),
            "  - [workspace] bad pids".to_owned(),
        ]
    );
}

#[test]
fn tag_errors_empty_input_yields_empty() {
    assert!(tag_errors::<&str>("config", Vec::new()).is_empty());
}

#[test]
fn bail_on_grant_errors_ok_when_empty() {
    bail_on_grant_errors(Vec::new()).unwrap();
}

#[test]
fn bail_on_grant_errors_bails_when_present() {
    let err = bail_on_grant_errors(vec!["  - [config] x".to_owned()]).unwrap_err();
    assert!(
        err.to_string().contains("docker grants validation failed"),
        "bail message must name the failure: {err}"
    );
    assert!(err.to_string().contains("[config] x"));
}

#[test]
fn tagged_grant_errors_tags_layer_and_catches_root_and_sudo() {
    let grants = DockerGrants {
        user: Some("root".to_owned()),
        sudo: Some(true),
        ..Default::default()
    };
    let errs = tagged_grant_errors("role", &grants);
    assert_eq!(errs.len(), 1, "root + sudo is one validation error");
    assert!(
        errs[0].starts_with("  - [role] "),
        "error must carry its source tag: {:?}",
        errs[0]
    );
}

#[test]
fn tagged_grant_errors_clean_grant_yields_nothing() {
    assert!(tagged_grant_errors("config", &DockerGrants::default()).is_empty());
}

#[tokio::test]
async fn run_launch_core_happy_path_returns_container_name() {
    let mut fix = LaunchCoreFixture::new();
    let core = fix.as_core();
    let name = launch_core::run_launch_core(core)
        .await
        .expect("happy path");
    assert_eq!(name, fix.container_name);
    // Sidecar/network teardown or role run must have touched Docker.
    let recorded = fix.docker.recorded.borrow();
    assert!(
        !recorded.is_empty(),
        "happy path must exercise Docker via FakeDocker; recorded empty"
    );
}

#[test]
fn conformance_wire_public_launch_controller_exports_complete_pipeline() -> anyhow::Result<()> {
    if std::env::var_os(INTEGRATED_LAUNCH_WIRE_CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg(
                "runtime::launch::launch_pipeline::tests::conformance_wire_public_launch_controller_exports_complete_pipeline",
            )
            .arg("--nocapture")
            .env(INTEGRATED_LAUNCH_WIRE_CHILD, "1")
            .status()?;
        anyhow::ensure!(
            status.success(),
            "isolated integrated launch wire test failed"
        );
        return Ok(());
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = {
        let _entered = runtime.enter();
        jackin_otlp_testbed::Testbed::start()?
    };
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )?;

    let mut fixture = LaunchCoreFixture::new();
    let private_source = "https://integrated-private.invalid/role.git?token=private-launch-token";
    fixture.config.roles.insert(
        fixture.selector.key(),
        jackin_config::RoleSource {
            git: private_source.to_owned(),
            trusted: true,
            env: BTreeMap::new(),
        },
    );
    fixture.runner = FakeRunner::for_load_agent([
        private_source.to_owned(),
        String::new(),
        "false 0 false".to_owned(),
        "false 0 false".to_owned(),
    ]);
    runtime.block_on(load_role(
        &fixture.paths,
        &mut fixture.config,
        &fixture.selector,
        &fixture.workspace,
        &fixture.docker,
        &mut fixture.runner,
        &fixture.opts,
    ))?;
    assert!(
        !fixture.runner.recorded.is_empty(),
        "public launch must cross the injected process port"
    );
    assert!(
        !fixture.docker.recorded.borrow().is_empty(),
        "public launch must cross the injected Docker port"
    );

    jackin_diagnostics::flush_wire_test_export()?;
    assert!(runtime.block_on(testbed.wait_for_all_signals(std::time::Duration::from_secs(2))));
    let spans = testbed.spans();
    let roots = spans
        .iter()
        .filter(|span| span.name == "launch")
        .collect::<Vec<_>>();
    let stages = spans
        .iter()
        .filter(|span| span.name == "launch.stage")
        .collect::<Vec<_>>();
    let stage_wire = format!("{stages:?}");
    assert_eq!(roots.len(), 1, "public controller must own one launch root");
    assert_eq!(
        stages.len(),
        11,
        "public controller must own every stage once: {stage_wire}"
    );
    for stage in &stages {
        assert_eq!(stage.trace_id, roots[0].trace_id);
        assert_eq!(stage.parent_span_id, roots[0].span_id);
    }
    for expected in jackin_telemetry::schema::enums::LaunchStageName::ALL {
        assert!(
            stage_wire.contains(expected.as_str()),
            "missing {expected:?}: {stage_wire}"
        );
    }
    assert_eq!(
        testbed.prohibited_value_violations(&[
            private_source,
            "integrated-private.invalid",
            "private-launch-token",
            &fixture.container_name,
            &fixture.image,
        ]),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}

#[tokio::test]
async fn run_launch_core_suite_a_grant_failure_cleans_up_before_return() {
    let mut fix = LaunchCoreFixture::new().with_bad_grants();
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
    // LoadCleanup uses resource names from from_container_name.
    assert!(
        recorded
            .iter()
            .any(|c| c.contains("docker rm -f") && c.contains("dind"))
            || recorded.iter().any(|c| c.contains("docker network rm")),
        "grant-failure path must run LoadCleanup (DinD/network rm); recorded: {recorded:?}"
    );
}

#[tokio::test]
async fn run_launch_core_finalize_error_runs_cleanup_before_return() {
    let mut fix = LaunchCoreFixture::new();
    fix.plant_corrupt_isolation_for_finalize_error();
    // Drive to finalization with sessions=0 so finalize_clean_exit reads isolation.json.
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

#[test]
fn launch_core_builder_populates_required_fields() {
    let mut fix = LaunchCoreFixture::new();
    let core = fix.as_core();
    assert_eq!(core.container_name, "jk-harness-agentsmith");
    assert_eq!(core.agent, Agent::Codex);
    assert!(matches!(core.image_decision, ImageDecision::Reuse { .. }));
    assert_eq!(core.role_key, "agent-smith");
    drop(core);
}

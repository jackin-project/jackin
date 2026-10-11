// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    let mut steps = StepCounter::new(
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
    let future = launch_core::run_launch_core(core);
    let future_bytes = size_of_val(&future);
    assert!(
        future_bytes <= 8 * 1024,
        "launch core must bound propagated phase state to 8 KiB; future is {future_bytes} bytes"
    );
    let name = future.await.expect("happy path");
    assert_eq!(name, fix.container_name);
    // Sidecar/network teardown or role run must have touched Docker.
    let recorded = fix.docker.recorded.borrow();
    assert!(
        !recorded.is_empty(),
        "happy path must exercise Docker via FakeDocker; recorded empty"
    );
}

#[tokio::test]
async fn run_launch_core_removes_container_if_generation_rotates_during_docker_run() {
    let mut fix = LaunchCoreFixture::new();
    let mut rotated = fix.config.clone();
    rotated
        .env
        .insert("ROTATED_DURING_DOCKER_RUN".into(), "new".into());
    let _rotation = schedule_config_rotation(&fix.paths, "docker run", &rotated);
    fix.runner.command_hook = Some(rotate_config_on_operation);

    let error = launch_core::run_launch_core(fix.as_core())
        .await
        .expect_err("launch must fail after config rotates during docker run");
    assert!(
        error
            .to_string()
            .contains("configuration changed during launch"),
        "unexpected rotation error: {error:#}"
    );
    assert!(
        fix.docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker rm -f {}", fix.container_name)),
        "stale started container must be force-removed: {:?}",
        fix.docker.recorded.borrow()
    );
}

#[test]
fn conformance_wire_public_launch_controller_exports_complete_pipeline() -> anyhow::Result<()> {
    if std::env::var_os(INTEGRATED_LAUNCH_WIRE_CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg(
                "runtime::launch::launch_pipeline::tests::case_01::conformance_wire_public_launch_controller_exports_complete_pipeline",
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
    fixture.docker.operation_hook = Some(observe_launch_docker);
    let private_source = "https://integrated-private.invalid/role.git?token=private-launch-token";
    fixture.config.roles.insert(
        fixture.selector.key(),
        jackin_config::RoleSource {
            git: private_source.to_owned(),
            trusted: true,
            env: BTreeMap::new(),
        },
    );
    std::fs::write(
        &fixture.paths.config_file,
        toml::to_string(&fixture.config).unwrap(),
    )
    .unwrap();
    fixture.runner = FakeRunner::for_load_agent([
        private_source.to_owned(),
        String::new(),
        "false 0 false".to_owned(),
        "false 0 false".to_owned(),
    ]);
    fixture.runner.command_hook = Some(observe_launch_process);
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
    assert!(runtime.block_on(testbed.wait_for_span_count(
        "launch",
        1,
        std::time::Duration::from_secs(2),
    )));
    let spans = testbed.spans();
    let roots = spans
        .iter()
        .filter(|span| span.name == "launch")
        .collect::<Vec<_>>();
    let stages = spans
        .iter()
        .filter(|span| span.name == "launch.stage")
        .collect::<Vec<_>>();
    let process_children = spans
        .iter()
        .filter(|span| span.name == "process.command")
        .collect::<Vec<_>>();
    let docker_children = spans
        .iter()
        .filter(|span| span.name == "http.client")
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
    assert!(
        !process_children.is_empty(),
        "public launch must contain governed subprocess children"
    );
    assert!(
        !docker_children.is_empty(),
        "public launch must contain governed Docker HTTP children"
    );
    for child in process_children.iter().chain(&docker_children) {
        assert_eq!(child.trace_id, roots[0].trace_id);
        assert!(
            child.parent_span_id == roots[0].span_id
                || stages
                    .iter()
                    .any(|stage| stage.span_id == child.parent_span_id),
            "boundary child must belong to the launch tree: {child:?}"
        );
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
    // Role creation runs strictly after `prepare_instance` migration and
    // before finalization; nothing else reads the envelope between them.
    let _corruption = schedule_isolation_corruption(
        format!("create_container:{}", fix.container_name),
        isolation_path,
    );
    fix.docker.operation_hook = Some(corrupt_isolation_on_operation);
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

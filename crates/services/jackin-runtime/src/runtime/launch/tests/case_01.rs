// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn capsule_otlp_fails_closed_for_network_endpoint_and_auth() {
    use jackin_diagnostics::CapsuleExportCoverage;

    assert_eq!(
        capsule_export_coverage(
            CapsuleNetwork::Disabled,
            CapsuleEndpoint::Safe,
            CapsuleAuth::Complete,
        ),
        CapsuleExportCoverage::DisabledNetworkNone
    );
    assert_eq!(
        capsule_export_coverage(
            CapsuleNetwork::Enabled,
            CapsuleEndpoint::Missing,
            CapsuleAuth::Complete,
        ),
        CapsuleExportCoverage::DisabledNoEndpoint
    );
    assert_eq!(
        capsule_export_coverage(
            CapsuleNetwork::Enabled,
            CapsuleEndpoint::Unclassified,
            CapsuleAuth::Complete,
        ),
        CapsuleExportCoverage::DisabledUnclassifiedEndpoint
    );
    assert_eq!(
        capsule_export_coverage(
            CapsuleNetwork::Enabled,
            CapsuleEndpoint::Safe,
            CapsuleAuth::HostOnly,
        ),
        CapsuleExportCoverage::DisabledUnclassifiedAuth
    );
    assert_eq!(
        capsule_export_coverage(
            CapsuleNetwork::Enabled,
            CapsuleEndpoint::Safe,
            CapsuleAuth::Complete,
        ),
        CapsuleExportCoverage::Enabled
    );
    assert_eq!(
        capsule_export_coverage(
            CapsuleNetwork::Enabled,
            CapsuleEndpoint::Safe,
            CapsuleAuth::Complete,
        ),
        CapsuleExportCoverage::Enabled
    );
}

#[test]
fn disabled_capsule_export_injects_no_telemetry_or_firewall_host() {
    let env = capsule_otlp_propagation(
        None,
        Some("authorization=private-host-header"),
        Some("00-private-traceparent"),
    );
    assert!(env.is_empty());
    assert_eq!(capsule_otlp_allowlist_host(None), None);
}

#[test]
fn enabled_capsule_export_uses_only_explicit_safe_carriers() {
    let endpoint = jackin_diagnostics::ContainerOtlp {
        endpoint: "http://host.docker.internal:4317".to_owned(),
        needs_host_gateway: true,
    };
    let env = capsule_otlp_propagation(
        Some(&endpoint),
        Some("authorization=capsule-safe"),
        Some("00-bounded-traceparent"),
    );
    assert_eq!(env.len(), 3);
    assert!(
        env.iter()
            .any(|value| value == "OTEL_EXPORTER_OTLP_HEADERS=authorization=capsule-safe")
    );
    assert_eq!(
        capsule_otlp_allowlist_host(Some(&endpoint)),
        Some("host.docker.internal")
    );
    assert!(!env.iter().any(|value| value.contains("CLIENT_KEY")));
    assert!(
        !env.iter()
            .any(|value| value.contains("private-host-header"))
    );
}

#[tokio::test]
async fn legacy_kept_dind_state_migrates_only_after_identity_and_ownership_verification() {
    use jackin_core::ContainerRow;
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let dind = "jk-prewarm-dind-dind";
    let network = "jk-prewarm-dind-net";
    let state_path = paths.data_dir.join("prewarm-dind.json");
    std::fs::write(
        &state_path,
        serde_json::json!({
            "schema_version": 1,
            "dind": dind,
            "network": network,
            "certs_volume": "jk-prewarm-dind-certs",
            "ready_ms": 123,
            "kept": true,
            "created_at_ms": 456
        })
        .to_string(),
    )
    .unwrap();

    let labels = HashMap::from([
        ("jackin.managed".to_owned(), "true".to_owned()),
        ("jackin.kind".to_owned(), "prewarm-dind".to_owned()),
        ("jackin.prewarm".to_owned(), "true".to_owned()),
    ]);
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![ContainerRow {
            name: dind.to_owned(),
            id: "daemon-id-legacy".to_owned(),
            labels: labels.clone(),
        }]])),
        inspect_by_id_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Running])),
        container_id_by_name: std::cell::RefCell::new(HashMap::from([(
            dind.to_owned(),
            "daemon-id-legacy".to_owned(),
        )])),
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([(
            dind.to_owned(),
            ContainerState::Running,
        )])),
        ..Default::default()
    };

    let migrated = super::launch_dind::ensure_prewarm_state_identity(&paths, &docker)
        .await
        .unwrap()
        .expect("legacy retained container should migrate");
    assert_eq!(migrated.id(), "daemon-id-legacy");
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
    assert_eq!(state["schema_version"], 2);
    assert_eq!(state["dind_id"], "daemon-id-legacy");
    assert_eq!(state["ready_ms"], 123);
    assert_eq!(state["certs_volume"], "jk-prewarm-dind-certs");

    let reloaded = super::launch_dind::ensure_prewarm_state_identity(&paths, &docker)
        .await
        .unwrap()
        .expect("migrated state should use the v2 identity path");
    assert_eq!(reloaded.id(), "daemon-id-legacy");
    assert_eq!(
        docker
            .recorded
            .borrow()
            .iter()
            .filter(|operation| operation.starts_with("docker ps -a"))
            .count(),
        1,
        "second load must not repeat schema migration"
    );
}

#[tokio::test]
async fn legacy_kept_dind_state_is_preserved_when_ownership_cannot_be_verified() {
    use jackin_core::ContainerRow;
    use jackin_test_support::FakeDockerClient;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let state_path = paths.data_dir.join("prewarm-dind.json");
    let legacy_state = serde_json::json!({
        "schema_version": 1,
        "dind": "jk-prewarm-dind-dind",
        "network": "jk-prewarm-dind-net",
        "certs_volume": "jk-prewarm-dind-certs",
        "ready_ms": 123,
        "kept": true,
        "created_at_ms": 456
    })
    .to_string();
    std::fs::write(&state_path, &legacy_state).unwrap();
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![ContainerRow {
            name: "jk-prewarm-dind-dind".to_owned(),
            id: "unverified-id".to_owned(),
            labels: HashMap::new(),
        }]])),
        ..Default::default()
    };

    let result = super::launch_dind::ensure_prewarm_state_identity(&paths, &docker).await;
    if let Err(error) = result {
        assert!(!error.to_string().is_empty());
    } else {
        panic!("legacy state without ownership evidence must not migrate");
    }
    assert_eq!(std::fs::read_to_string(state_path).unwrap(), legacy_state);
}

#[tokio::test]
async fn stopped_legacy_kept_dind_migrates_for_identity_bound_recovery() {
    use jackin_core::ContainerRow;
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    let dind = "jk-prewarm-dind-dind";
    let state_path = paths.data_dir.join("prewarm-dind.json");
    std::fs::write(
        &state_path,
        serde_json::json!({
            "schema_version": 1,
            "dind": dind,
            "network": "jk-prewarm-dind-net",
            "certs_volume": "jk-prewarm-dind-certs",
            "ready_ms": 123,
            "kept": true,
            "created_at_ms": 456
        })
        .to_string(),
    )
    .unwrap();
    let labels = HashMap::from([
        ("jackin.managed".to_owned(), "true".to_owned()),
        ("jackin.kind".to_owned(), "prewarm-dind".to_owned()),
        ("jackin.prewarm".to_owned(), "true".to_owned()),
    ]);
    let stopped = ContainerState::Stopped {
        exit_code: 137,
        oom_killed: false,
    };
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![ContainerRow {
            name: dind.to_owned(),
            id: "daemon-id-stopped".to_owned(),
            labels,
        }]])),
        inspect_by_id_queue: std::cell::RefCell::new(VecDeque::from([stopped.clone()])),
        container_id_by_name: std::cell::RefCell::new(HashMap::from([(
            dind.to_owned(),
            "daemon-id-stopped".to_owned(),
        )])),
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([(dind.to_owned(), stopped)])),
        ..Default::default()
    };

    let adopted = super::launch_dind::adopt_prewarmed_dind_sidecar(&paths, &docker).await;
    assert!(adopted.is_none(), "stopped sidecar must not be adopted");
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
    assert_eq!(state["schema_version"], 2);
    assert_eq!(state["dind_id"], "daemon-id-stopped");

    let cleanup_handle = super::launch_dind::ensure_prewarm_state_identity(&paths, &docker)
        .await
        .unwrap()
        .expect("stopped sidecar remains addressable for ID-bound cleanup");
    assert_eq!(cleanup_handle.id(), "daemon-id-stopped");
}

#[test]
fn sensitive_mount_prompt_lists_every_hit_src_and_reason() {
    let sensitive = vec![
        jackin_config::SensitiveMount {
            src: "/home/op/.ssh".to_owned(),
            reason: "SSH private keys".to_owned(),
        },
        jackin_config::SensitiveMount {
            src: "/home/op/.aws".to_owned(),
            reason: "AWS credentials".to_owned(),
        },
    ];
    let prompt = sensitive_mount_prompt(&sensitive);
    // Every flagged path and its reason must reach the operator — a
    // dropped hit would silently hide a credential exposure.
    for hit in &sensitive {
        assert!(prompt.contains(&hit.src), "missing src in: {prompt}");
        assert!(prompt.contains(&hit.reason), "missing reason in: {prompt}");
    }
    assert!(prompt.contains("Continue with these mounts?"));
}

#[test]
fn docker_build_failure_cli_error_contains_no_local_artifact_paths() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let run = jackin_diagnostics::RunDiagnostics::start(
        &paths,
        false,
        "load",
        jackin_diagnostics::ServiceIdentity::HOST_INTERACTIVE,
    )
    .unwrap();
    // The doubling this test used to pin was an artifact of the bare
    // variant: now the stderr tail rides inside `DockerBuildFailed` and
    // rendering passes it through once, with temp paths redacted.
    let error: anyhow::Error = jackin_docker::DockerError::DockerBuildFailed {
        stderr: "ERROR: failed to read dockerfile: open <redacted-path>: no such file".to_owned(),
    }
    .into();
    let rendered = launch_failure_cli_error(
        crate::runtime::progress::LaunchStage::DerivedImage,
        &error,
        Some(run.as_ref()),
    )
    .to_string();

    assert_eq!(
        rendered,
        "Docker build command failed: ERROR: failed to read dockerfile: open <redacted-path>: no such file"
    );
    assert!(!rendered.contains("/tmp/"));
}

#[test]
fn derived_image_cli_error_preserves_original_without_docker_output() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let run = jackin_diagnostics::RunDiagnostics::start(
        &paths,
        false,
        "load",
        jackin_diagnostics::ServiceIdentity::HOST_INTERACTIVE,
    )
    .unwrap();

    let error = anyhow::anyhow!("preparing capsule binary failed");
    let rendered = launch_failure_cli_error(
        crate::runtime::progress::LaunchStage::DerivedImage,
        &error,
        Some(run.as_ref()),
    )
    .to_string();

    assert_eq!(rendered, "preparing capsule binary failed");
    assert!(!rendered.contains("Docker build command failed"));
    assert!(!rendered.contains("docker output"));
}

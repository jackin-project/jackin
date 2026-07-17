// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `docker_client`.
use super::*;

// Compile-time guard: bollard's `connect_with_ssl_defaults` exists only
// when its TLS feature (`aws-lc-rs`) is enabled. Dropping the feature from
// Cargo.toml stops this compiling — turning the otherwise-silent "plain
// HTTP to a `tcp://…:2376` TLS daemon → 400" runtime failure into a
// build-time error.
const _: fn() -> Result<Docker, bollard::errors::Error> = Docker::connect_with_ssl_defaults;

#[tokio::test]
async fn docker_response_failure_stays_inside_http_owner_without_payload() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let result: anyhow::Result<()> = docker_http(EXEC_START, async {
        anyhow::bail!("private-container private-command private-output")
    })
    .await;
    assert!(result.is_err());

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 1);
    assert_eq!(export.error_span_count(), 1);
    assert!(export.contains_span_text("/exec/{id}/start"));
    assert!(export.contains_span_text("http_error"));
    for prohibited in ["private-container", "private-command", "private-output"] {
        assert!(!export.contains_span_text(prohibited));
        assert!(!export.contains_log_text(prohibited));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conformance_wire_docker_http_exports_bounded_private_shapes() -> anyhow::Result<()> {
    let testbed = jackin_otlp_testbed::Testbed::start()?;
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )?;

    docker_http(CONTAINER_LIST, async { Ok::<_, anyhow::Error>(()) }).await?;
    let failure: anyhow::Result<()> = docker_http(EXEC_START, async {
        anyhow::bail!("private-container private-command private-output")
    })
    .await;
    assert!(failure.is_err());
    jackin_diagnostics::flush_wire_test_export()?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let spans = loop {
        let spans = testbed
            .spans()
            .into_iter()
            .filter(|span| span.name == "http.client")
            .collect::<Vec<_>>();
        if spans.len() == 2 {
            break spans;
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "Docker HTTP wire spans did not arrive"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    };
    let wire_text = format!("{spans:?}");
    for expected in [
        "/containers/json",
        "/exec/{id}/start",
        "success",
        "failure",
        "http_error",
    ] {
        assert!(
            wire_text.contains(expected),
            "missing {expected}: {wire_text}"
        );
    }
    for prohibited in ["private-container", "private-command", "private-output"] {
        assert!(!wire_text.contains(prohibited), "exported {prohibited}");
    }
    assert_eq!(
        testbed.prohibited_value_violations(&[
            "private-container",
            "private-command",
            "private-output",
        ]),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}

#[test]
fn choose_connection_env_only_returns_defaults() {
    assert_eq!(choose_connection(true, None), ConnectionChoice::Defaults);
}

#[test]
fn choose_connection_env_overrides_context() {
    assert_eq!(
        choose_connection(true, Some(context_endpoint("unix:///ignored"))),
        ConnectionChoice::Defaults
    );
}

#[test]
fn choose_connection_uses_context_when_env_unset() {
    assert_eq!(
        choose_connection(false, Some(context_endpoint("unix:///ctx"))),
        ConnectionChoice::Host("unix:///ctx".to_owned())
    );
}

#[test]
fn choose_connection_rejects_ssh_context() {
    assert_eq!(
        choose_connection(false, Some(context_endpoint("ssh://me@docker-host"))),
        ConnectionChoice::unsupported(UnsupportedReason::SshTransport, "ssh://me@docker-host")
    );
}

#[test]
fn choose_connection_rejects_https_context() {
    assert_eq!(
        choose_connection(false, Some(context_endpoint("https://docker-host:2376"))),
        ConnectionChoice::unsupported(UnsupportedReason::TlsTransport, "https://docker-host:2376")
    );
}

#[test]
fn choose_connection_rejects_context_with_tls_material() {
    let endpoint = DockerContextEndpoint {
        has_tls_material: true,
        ..context_endpoint("tcp://docker-host:2376")
    };
    assert_eq!(
        choose_connection(false, Some(endpoint)),
        ConnectionChoice::unsupported(
            UnsupportedReason::ContextTlsMaterial,
            "tcp://docker-host:2376"
        )
    );
}

#[test]
fn choose_connection_rejects_context_with_tls_skip_verify() {
    let endpoint = DockerContextEndpoint {
        skip_tls_verify: true,
        ..context_endpoint("tcp://docker-host:2376")
    };
    assert_eq!(
        choose_connection(false, Some(endpoint)),
        ConnectionChoice::unsupported(
            UnsupportedReason::ContextTlsMaterial,
            "tcp://docker-host:2376"
        )
    );
}

#[test]
fn choose_connection_rejects_context_with_unknown_uri() {
    assert_eq!(
        choose_connection(false, Some(context_endpoint("fd://0"))),
        ConnectionChoice::unsupported(UnsupportedReason::UnsupportedUri, "fd://0")
    );
}

#[test]
fn choose_connection_accepts_http_context() {
    assert_eq!(
        choose_connection(false, Some(context_endpoint("http://docker-host:2375"))),
        ConnectionChoice::Host("http://docker-host:2375".to_owned())
    );
}

#[test]
#[cfg(not(windows))]
fn choose_connection_rejects_npipe_on_unix() {
    assert_eq!(
        choose_connection(
            false,
            Some(context_endpoint("npipe:////./pipe/docker_engine"))
        ),
        ConnectionChoice::unsupported(
            UnsupportedReason::UnsupportedUri,
            "npipe:////./pipe/docker_engine"
        )
    );
}

#[test]
#[cfg(windows)]
fn choose_connection_accepts_npipe_on_windows() {
    assert_eq!(
        choose_connection(
            false,
            Some(context_endpoint("npipe:////./pipe/docker_engine"))
        ),
        ConnectionChoice::Host("npipe:////./pipe/docker_engine".to_string())
    );
}

#[test]
fn connection_choice_empty_host_returns_defaults() {
    assert_eq!(
        DockerContextEndpoint::new("", false, false).connection_choice(),
        ConnectionChoice::Defaults
    );
    assert_eq!(
        DockerContextEndpoint::new("   ", false, false).connection_choice(),
        ConnectionChoice::Defaults
    );
}

#[test]
fn connection_choice_ssh_takes_precedence_over_tls_material() {
    assert_eq!(
        DockerContextEndpoint::new("ssh://me@docker-host", true, true).connection_choice(),
        ConnectionChoice::unsupported(UnsupportedReason::SshTransport, "ssh://me@docker-host")
    );
}

#[test]
fn unsupported_message_ssh_includes_host_and_hint() {
    let msg = ConnectionChoice::unsupported_message(
        &UnsupportedReason::SshTransport,
        "ssh://me@docker-host",
    );
    assert!(msg.contains("SSH transport"));
    assert!(msg.contains("ssh://me@docker-host"));
    assert!(msg.ends_with(
            ". Set DOCKER_HOST to a unix:// socket, a plain tcp:// endpoint, or a TLS tcp:// endpoint with DOCKER_TLS_VERIFY and DOCKER_CERT_PATH set, to override."
        ));
}

#[test]
fn unsupported_message_tls_transport_includes_host() {
    let msg = ConnectionChoice::unsupported_message(
        &UnsupportedReason::TlsTransport,
        "https://docker-host:2376",
    );
    assert!(msg.contains("TLS transport"));
    assert!(msg.contains("https://docker-host:2376"));
}

#[test]
fn unsupported_message_tls_material_includes_host() {
    let msg = ConnectionChoice::unsupported_message(
        &UnsupportedReason::ContextTlsMaterial,
        "tcp://docker-host:2376",
    );
    assert!(msg.contains("includes TLS settings"));
    assert!(msg.contains("tcp://docker-host:2376"));
}

#[test]
fn unsupported_message_unknown_uri_includes_host() {
    let msg = ConnectionChoice::unsupported_message(&UnsupportedReason::UnsupportedUri, "fd://0");
    assert!(msg.contains("unsupported Docker host URI"));
    assert!(msg.contains("fd://0"));
}

#[test]
fn choose_connection_falls_back_to_defaults_when_no_context() {
    assert_eq!(choose_connection(false, None), ConnectionChoice::Defaults);
}

#[test]
fn docker_host_env_is_set_from_recognises_unset_and_empty_as_unset() {
    assert!(!docker_host_env_is_set_from(None));
    assert!(!docker_host_env_is_set_from(Some(OsStr::new(""))));
}

#[test]
fn docker_host_env_is_set_from_recognises_non_empty_as_set() {
    assert!(docker_host_env_is_set_from(Some(OsStr::new(
        "tcp://127.0.0.1:2375"
    ))));
    assert!(docker_host_env_is_set_from(Some(OsStr::new(
        "unix:///var/run/docker.sock"
    ))));
}

#[test]
fn parse_docker_context_endpoint_reads_host_and_tls_flags() {
    let endpoint = parse_docker_context_endpoint(
        br#"{
                "Endpoints": {
                    "docker": {
                        "Host": "tcp://docker-host:2376",
                        "SkipTLSVerify": true
                    }
                },
                "TLSMaterial": {
                    "docker": ["ca.pem", "cert.pem", "key.pem"]
                }
            }"#,
    )
    .unwrap();
    assert_eq!(endpoint.host, "tcp://docker-host:2376");
    assert!(endpoint.skip_tls_verify);
    assert!(endpoint.has_tls_material);
}

#[test]
fn parse_docker_context_endpoint_returns_none_without_docker_endpoint() {
    assert_eq!(
        parse_docker_context_endpoint(br#"{"Endpoints": {}, "TLSMaterial": {}}"#),
        None
    );
}

#[test]
fn parse_docker_context_endpoint_returns_none_on_malformed_json() {
    assert_eq!(parse_docker_context_endpoint(b""), None);
    assert_eq!(parse_docker_context_endpoint(b"not json"), None);
    assert_eq!(
        parse_docker_context_endpoint(br#"{"Endpoints": {"docker"#),
        None
    );
}

#[test]
fn parse_docker_context_endpoint_trims_host_whitespace() {
    let endpoint = parse_docker_context_endpoint(
        br#"{
                "Endpoints": {
                    "docker": {
                        "Host": "  unix:///var/run/docker.sock  "
                    }
                }
            }"#,
    )
    .unwrap();
    assert_eq!(endpoint.host, "unix:///var/run/docker.sock");
}

#[test]
fn parse_docker_context_endpoint_handles_missing_tls_material_field() {
    let endpoint = parse_docker_context_endpoint(
        br#"{
                "Endpoints": {
                    "docker": {
                        "Host": "unix:///var/run/docker.sock"
                    }
                }
            }"#,
    )
    .unwrap();
    assert!(!endpoint.skip_tls_verify);
    assert!(!endpoint.has_tls_material);
}

#[test]
fn parse_docker_context_endpoint_handles_skip_tls_verify_false() {
    let endpoint = parse_docker_context_endpoint(
        br#"{
                "Endpoints": {
                    "docker": {
                        "Host": "tcp://docker-host:2375",
                        "SkipTLSVerify": false
                    }
                },
                "TLSMaterial": {"docker": null}
            }"#,
    )
    .unwrap();
    assert!(!endpoint.skip_tls_verify);
    assert!(!endpoint.has_tls_material);
}

#[test]
fn tls_material_present_treats_emptiness_as_absent() {
    use serde_json::Value;
    assert!(!tls_material_present(&Value::Null));
    assert!(!tls_material_present(&serde_json::json!([])));
    assert!(!tls_material_present(&serde_json::json!({})));
    assert!(!tls_material_present(&Value::String(String::new())));
    assert!(!tls_material_present(&Value::String("   ".to_owned())));
    assert!(!tls_material_present(&Value::Bool(false)));
}

#[test]
fn tls_material_present_treats_populated_values_as_present() {
    use serde_json::Value;
    assert!(tls_material_present(&serde_json::json!(["ca.pem"])));
    assert!(tls_material_present(&serde_json::json!({"ca": "ca.pem"})));
    assert!(tls_material_present(&Value::String("ca.pem".to_owned())));
    assert!(tls_material_present(&Value::Bool(true)));
    assert!(tls_material_present(&serde_json::json!(1)));
}

fn context_endpoint(host: &str) -> DockerContextEndpoint {
    DockerContextEndpoint::new(host, false, false)
}

#[test]
fn container_state_short_label() {
    let cases: &[(ContainerState, &str)] = &[
        (ContainerState::Running, "running"),
        (ContainerState::Paused, "paused"),
        (ContainerState::Restarting, "restarting"),
        (ContainerState::Removing, "removing"),
        (ContainerState::Created, "created"),
        (ContainerState::Dead, "dead"),
        (
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            },
            "stopped exit:0",
        ),
        (
            ContainerState::Stopped {
                exit_code: 1,
                oom_killed: false,
            },
            "stopped exit:1",
        ),
        (
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: true,
            },
            "stopped oom_killed",
        ),
        (ContainerState::NotFound, "missing"),
        (
            ContainerState::InspectUnavailable("reason".to_owned()),
            "unavailable",
        ),
    ];

    for (state, expected) in cases {
        assert_eq!(
            state.short_label(),
            *expected,
            "short_label mismatch for {state:?}"
        );
    }
}

#[test]
fn docker_http_routes_are_static_bounded_templates() {
    let routes = [
        PING,
        CONTAINER_INSPECT,
        CONTAINER_REMOVE,
        CONTAINER_LIST,
        CONTAINER_CREATE,
        CONTAINER_START,
        VOLUME_REMOVE,
        NETWORK_CREATE,
        NETWORK_REMOVE,
        NETWORK_LIST,
        NETWORK_INSPECT,
        IMAGE_LIST,
        IMAGE_REMOVE,
        IMAGE_INSPECT,
        IMAGE_PULL,
        EXEC_CREATE,
        EXEC_START,
        EXEC_INSPECT,
    ];
    for route in routes {
        assert!(matches!(route.method, "GET" | "POST" | "DELETE"));
        assert!(route.template.starts_with('/'));
        assert!(!route.template.contains('?'));
        assert!(!route.template.contains("private"));
        for segment in route.template.split('/') {
            if segment.starts_with('{') {
                assert!(matches!(segment, "{id}" | "{name}"));
            }
        }
    }
}

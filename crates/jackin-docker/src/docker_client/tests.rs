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

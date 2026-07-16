// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `exec_host`.
use super::*;

#[test]
fn validate_op_source_accepts_well_formed_ref() {
    validate_op_source("op://vault/item/field").unwrap();
}

#[test]
fn validate_op_source_rejects_non_op_scheme() {
    validate_op_source("https://evil/x").unwrap_err();
    validate_op_source("vault/item/field").unwrap_err();
}

#[test]
fn validate_op_source_rejects_flag_segments() {
    // A path segment that looks like a CLI flag could inject arguments into
    // `op read` — must be rejected before the subprocess is spawned.
    validate_op_source("op://vault/-rf/field").unwrap_err();
    validate_op_source("op://-vault/item").unwrap_err();
}

/// Drive `handle_connection` over an in-memory socket pair and return the
/// decoded JSON reply (`{"values":…}` or `{"error":…}`).
async fn roundtrip(
    allowed: Vec<ExecBinding>,
    request_refs: serde_json::Value,
) -> serde_json::Value {
    #[cfg(target_os = "linux")]
    let caller_auth = CallerAuth::PeerPid(std::process::id());
    #[cfg(not(target_os = "linux"))]
    let caller_auth = CallerAuth::CapsuleDaemon;

    roundtrip_with_auth(allowed, request_refs, caller_auth)
        .await
        .expect("authenticated roundtrip should return a reply")
}

async fn roundtrip_with_auth(
    allowed: Vec<ExecBinding>,
    request_refs: serde_json::Value,
    caller_auth: CallerAuth,
) -> Option<serde_json::Value> {
    let (mut client, server) = UnixStream::pair().unwrap();
    let server_task =
        tokio::spawn(async move { handle_connection(server, &allowed, caller_auth).await });

    let body = serde_json::to_vec(&serde_json::json!({
        "ctx": { "v": jackin_telemetry::propagation::VERSION },
        "refs": request_refs,
    }))
    .unwrap();
    if client
        .write_all(&(body.len() as u32).to_be_bytes())
        .await
        .is_err()
        || client.write_all(&body).await.is_err()
    {
        server_task.await.unwrap().unwrap();
        return None;
    }

    let mut len_buf = [0u8; 4];
    if client.read_exact(&mut len_buf).await.is_err() {
        server_task.await.unwrap().unwrap();
        return None;
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut reply = vec![0u8; len];
    client.read_exact(&mut reply).await.unwrap();

    server_task.await.unwrap().unwrap();
    Some(serde_json::from_slice(&reply).unwrap())
}

async fn exported_exec_roundtrip(
    context: jackin_protocol::TelemetryContext,
) -> (serde_json::Value, Vec<jackin_diagnostics::TestSpanSnapshot>) {
    #[cfg(target_os = "linux")]
    let caller_auth = CallerAuth::PeerPid(std::process::id());
    #[cfg(not(target_os = "linux"))]
    let caller_auth = CallerAuth::CapsuleDaemon;
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    let (mut client, server) = UnixStream::pair().expect("host socket pair");
    client
        .write_all(&frame(&CredRequest {
            ctx: context,
            refs: Vec::new(),
        }))
        .await
        .expect("write credential request");
    handle_connection(server, &[], caller_auth)
        .await
        .expect("handle credential request");
    let mut len = [0_u8; 4];
    client
        .read_exact(&mut len)
        .await
        .expect("read reply length");
    let mut body = vec![0_u8; u32::from_be_bytes(len) as usize];
    client.read_exact(&mut body).await.expect("read reply body");
    drop(guard);
    export.force_flush();
    (
        serde_json::from_slice(&body).expect("decode credential reply"),
        export.finished_spans(),
    )
}

#[tokio::test(flavor = "current_thread")]
async fn exec_socket_exports_client_parent_server_after_reply_write() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str("jackin.host.Credentials/Resolve"),
        },
    ];
    let client_operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_CLIENT, &attrs)
            .expect("client operation");
    let mut context = jackin_protocol::TelemetryContext::v1();
    client_operation
        .span()
        .in_scope(|| jackin_telemetry::propagation::inject(&mut context));
    #[cfg(target_os = "linux")]
    let caller_auth = CallerAuth::PeerPid(std::process::id());
    #[cfg(not(target_os = "linux"))]
    let caller_auth = CallerAuth::CapsuleDaemon;
    let (mut client, server) = UnixStream::pair().expect("host socket pair");
    client
        .write_all(&frame(&CredRequest {
            ctx: context,
            refs: Vec::new(),
        }))
        .await
        .expect("write credential request");
    handle_connection(server, &[], caller_auth)
        .await
        .expect("handle credential request");
    let mut len = [0_u8; 4];
    client
        .read_exact(&mut len)
        .await
        .expect("read reply length");
    let mut body = vec![0_u8; u32::from_be_bytes(len) as usize];
    client.read_exact(&mut body).await.expect("read reply body");
    let reply: CredReply = serde_json::from_slice(&body).expect("decode reply");
    assert!(matches!(reply, CredReply::Ok { .. }));
    client_operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    drop(guard);
    export.force_flush();

    let spans = export.finished_spans();
    assert_eq!(spans.len(), 2);
    let client = spans
        .iter()
        .find(|span| span.name == "rpc.client")
        .expect("client span");
    let server = spans
        .iter()
        .find(|span| span.name == "rpc.server")
        .expect("server span");
    assert_eq!(server.trace_id, client.trace_id);
    assert_eq!(server.parent_span_id, client.span_id);
    assert!(!client.error && !server.error);
}

#[tokio::test(flavor = "current_thread")]
async fn exec_socket_propagation_matrix_handles_remote_context_and_bad_ids() {
    let trace_id = "4bf92f3577b34da6a3ce929d0e0e4736";
    let parent_id = "00f067aa0ba902b7";
    let mut sampled = jackin_protocol::TelemetryContext::v1();
    sampled.traceparent = Some(format!("00-{trace_id}-{parent_id}-01"));
    let (reply, spans) = exported_exec_roundtrip(sampled).await;
    assert_eq!(reply["status"], "ok");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].trace_id, trace_id);
    assert_eq!(spans[0].parent_span_id, parent_id);

    for context in [
        jackin_protocol::TelemetryContext::v1(),
        jackin_protocol::TelemetryContext {
            traceparent: Some("malformed".to_owned()),
            ..jackin_protocol::TelemetryContext::v1()
        },
    ] {
        let (reply, spans) = exported_exec_roundtrip(context).await;
        assert_eq!(reply["status"], "ok");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].parent_span_id, "0000000000000000");
    }

    let mut unsampled = jackin_protocol::TelemetryContext::v1();
    unsampled.traceparent = Some(format!("00-{trace_id}-{parent_id}-00"));
    let (reply, spans) = exported_exec_roundtrip(unsampled).await;
    assert_eq!(reply["status"], "ok");
    assert!(spans.is_empty());

    let bad_id = jackin_protocol::TelemetryContext {
        job_id: Some("not-a-uuid".to_owned()),
        ..jackin_protocol::TelemetryContext::v1()
    };
    let (reply, spans) = exported_exec_roundtrip(bad_id).await;
    assert_eq!(reply["status"], "error");
    assert!(spans.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn exec_socket_marks_server_failure_when_peer_closes_before_reply() {
    use std::net::Shutdown;

    #[cfg(target_os = "linux")]
    let caller_auth = CallerAuth::PeerPid(std::process::id());
    #[cfg(not(target_os = "linux"))]
    let caller_auth = CallerAuth::CapsuleDaemon;
    let mut context = jackin_protocol::TelemetryContext::v1();
    context.traceparent =
        Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_owned());
    let (mut client, server) = UnixStream::pair().expect("host socket pair");
    client
        .write_all(&frame(&CredRequest {
            ctx: context,
            refs: Vec::new(),
        }))
        .await
        .expect("write credential request");
    client
        .into_std()
        .expect("convert client socket")
        .shutdown(Shutdown::Both)
        .expect("close client socket");
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    assert!(handle_connection(server, &[], caller_auth).await.is_err());
    drop(guard);
    export.force_flush();
    assert_eq!(export.error_span_count(), 1);
}

#[tokio::test]
async fn unauthorized_credential_payload_is_absent_from_telemetry() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(true);
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let secret_name = "PRIVATE_TOKEN_NAME";
    let secret_source = "op://private-vault/private-item/private-field";
    let reply = roundtrip(
        Vec::new(),
        serde_json::json!([{
            "name": secret_name,
            "kind": "op",
            "source": secret_source,
        }]),
    )
    .await;
    assert!(reply.get("error").is_some());
    export.force_flush();
    assert!(!export.contains_log_text(secret_name));
    assert!(!export.contains_log_text(secret_source));
}

#[tokio::test]
async fn approved_literal_ref_resolves() {
    let allowed = vec![ExecBinding {
        name: "TOKEN".into(),
        kind: ExecKind::Literal,
        source: "s3cr3t".into(),
    }];
    let reply = roundtrip(
        allowed,
        serde_json::json!([{ "name": "TOKEN", "kind": "literal", "source": "s3cr3t" }]),
    )
    .await;
    assert_eq!(reply["values"]["TOKEN"], "s3cr3t");
    assert!(reply.get("error").is_none());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn unauthenticated_peer_is_rejected_before_resolution() {
    let allowed = vec![ExecBinding {
        name: "TOKEN".into(),
        kind: ExecKind::Literal,
        source: "s3cr3t".into(),
    }];
    let reply = roundtrip_with_auth(
        allowed,
        serde_json::json!([{ "name": "TOKEN", "kind": "literal", "source": "s3cr3t" }]),
        CallerAuth::PeerPid(std::process::id().saturating_add(1)),
    )
    .await;

    assert!(reply.is_none(), "unauthenticated peer must be closed");
}

#[cfg(target_os = "linux")]
#[test]
fn container_init_peer_status_requires_innermost_nspid_one() {
    assert!(peer_is_container_init_process_status(
        "Name:\tjackin-capsule\nNSpid:\t424242\t1\n"
    ));
    assert!(!peer_is_container_init_process_status(
        "Name:\tagent\nNSpid:\t424243\t37\n"
    ));
    assert!(!peer_is_container_init_process_status("Name:\tno-nspid\n"));
}

#[tokio::test]
async fn unapproved_source_is_rejected() {
    // Same name + kind, but a `source` the operator never approved. A
    // name-only match would let a compromised container swap the source to
    // read a different secret — the allow-list must reject this.
    let allowed = vec![ExecBinding {
        name: "TOKEN".into(),
        kind: ExecKind::Literal,
        source: "approved".into(),
    }];
    let reply = roundtrip(
        allowed,
        serde_json::json!([{ "name": "TOKEN", "kind": "literal", "source": "attacker-swapped" }]),
    )
    .await;
    assert!(reply.get("values").is_none());
    assert_eq!(reply["error"], "credential reference is not approved");
}

#[tokio::test]
async fn resolution_failure_reply_does_not_echo_the_credential_source() {
    let secret_source = "not-an-op-uri/private-vault/private-item/private-field";
    let allowed = vec![ExecBinding {
        name: "TOKEN".into(),
        kind: ExecKind::Op,
        source: secret_source.into(),
    }];
    let reply = roundtrip(
        allowed,
        serde_json::json!([{ "name": "TOKEN", "kind": "op", "source": secret_source }]),
    )
    .await;
    assert_eq!(reply["error"], "credential resolution failed");
    assert!(!reply.to_string().contains(secret_source));
}

#[tokio::test]
async fn approved_env_ref_resolves_from_host_env() {
    // Use an existing var (PATH is always set) — the crate forbids `unsafe`, so
    // `std::env::set_var` is unavailable.
    let expected = std::env::var("PATH").expect("PATH is set in the test env");
    let allowed = vec![ExecBinding {
        name: "X".into(),
        kind: ExecKind::Env,
        source: "$PATH".into(),
    }];
    let reply = roundtrip(
        allowed,
        serde_json::json!([{ "name": "X", "kind": "env", "source": "$PATH" }]),
    )
    .await;
    assert_eq!(reply["values"]["X"], expected);
}

#[tokio::test]
async fn unknown_name_is_rejected() {
    let allowed = vec![ExecBinding {
        name: "TOKEN".into(),
        kind: ExecKind::Literal,
        source: "x".into(),
    }];
    let reply = roundtrip(
        allowed,
        serde_json::json!([{ "name": "OTHER", "kind": "literal", "source": "x" }]),
    )
    .await;
    assert!(reply.get("values").is_none());
    assert!(reply.get("error").is_some());
}

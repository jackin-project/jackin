// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `client`.
use super::*;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::net::UnixListener;

#[tokio::test]
async fn control_ack_reader_consumes_and_decodes_full_frame() {
    let (mut writer, mut reader) = UnixStream::pair().unwrap();
    writer
        .write_all(&control_frame(&ServerMsg::Ack))
        .await
        .unwrap();
    assert!(matches!(
        read_control_reply(&mut reader).await.unwrap(),
        ServerMsg::Ack
    ));
}

#[tokio::test]
async fn control_ack_reader_rejects_truncated_body() {
    let (mut writer, mut reader) = UnixStream::pair().unwrap();
    writer.write_all(&8_u32.to_be_bytes()).await.unwrap();
    writer.write_all(b"{}").await.unwrap();
    writer.shutdown().await.unwrap();
    read_control_reply(&mut reader).await.unwrap_err();
}

fn account(provider: &str, lifecycle: &str, phase: &str) -> serde_json::Value {
    serde_json::json!({
        "provider_id": provider,
        "display_name": provider,
        "rank": 0,
        "membership_state": "current",
        "freshness": {"generation": 1, "phase": phase, "is_stale": false},
        "accounts": [{
            "canonical_account_id": format!("{provider}-account"),
            "refresh_capabilities": [],
            "identity_kind": "provider_account_id",
            "rank": 0,
            "display_label": format!("{provider} account"),
            "lifecycle": lifecycle,
            "freshness": {"generation": 1, "phase": phase, "is_stale": false},
            "provenance_count": 1,
            "windows": [{
                "window_id": "session",
                "rank": 0,
                "category": "session",
                "label": "Session",
                "value_label": "63% used",
                "reset_label": "soon",
                "used_percent": 63,
                "used_raw_percent": 63,
                "quota_state": "available"
            }],
            "issues": []
        }],
        "issues": []
    })
}

fn membership(mut providers: Vec<serde_json::Value>) -> UsageAccountMembershipV1 {
    for (rank, provider) in providers.iter_mut().enumerate() {
        provider["rank"] = serde_json::json!(rank);
    }
    serde_json::from_value(serde_json::json!({
        "state": "current",
        "projection": {
            "schema_version": 2,
            "projection_id": "projection-1",
            "generated_at_epoch": 1_781_185_680,
            "discovery_revision": "discovery-1",
            "broker_instance_id": "broker-1",
            "broker_generation": 1,
            "refresh_state": "idle",
            "providers": providers,
            "unresolved": [],
            "unresolved_grants": [],
            "issues": []
        }
    }))
    .unwrap()
}

#[test]
fn usage_verify_accepts_trusted_rows_for_every_provider() {
    let accounts = membership(vec![
        account("openai", "available", "current"),
        account("anthropic", "available", "current"),
        account("amp", "available", "current"),
        account("xai", "available", "current"),
        account("zai", "available", "current"),
        account("kimi", "available", "current"),
        account("minimax", "available", "current"),
    ]);

    let checks = verify_usage_accounts(&accounts);

    assert_eq!(checks.len(), 7);
    assert!(
        checks.iter().all(|check| check.status == "ok"),
        "{checks:?}"
    );
}

#[test]
fn usage_verify_reports_missing_and_untrusted_providers() {
    let accounts = membership(vec![
        account("openai", "needs_login", "failed"),
        account("amp", "available", "current"),
    ]);

    let checks = verify_usage_accounts(&accounts);

    let codex = checks
        .iter()
        .find(|check| check.label == "OpenAI")
        .expect("OpenAI check");
    assert_eq!(codex.status, "untrusted");
    assert!(
        codex
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("needs_login")),
        "{codex:?}"
    );
    let anthropic = checks
        .iter()
        .find(|check| check.label == "Anthropic")
        .expect("Anthropic check");
    assert_eq!(anthropic.status, "missing");
    let amp = checks
        .iter()
        .find(|check| check.label == "Amp")
        .expect("Amp check");
    assert_eq!(amp.status, "ok");
}

#[tokio::test]
async fn attach_proxy_relays_binary_bytes_without_interpreting_frames() {
    let tmp = TempDir::new().unwrap();
    let socket_path = short_socket_path(&tmp, "proxy.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let client_frame = vec![0x01, 0x00, 0x00, 0x00, 0x02, 0xff, 0x00];
    let server_frame = vec![0x82, 0x00, 0x00, 0x00, 0x03, b'o', b'u', b't'];
    let expected_client_frame = client_frame.clone();
    let server_frame_for_task = server_frame.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        jackin_protocol::capsule_transport::server_handshake_async(&mut stream)
            .await
            .unwrap();
        let mut received = vec![0u8; expected_client_frame.len()];
        stream.read_exact(&mut received).await.unwrap();
        assert_eq!(received, expected_client_frame);
        stream.write_all(&server_frame_for_task).await.unwrap();
        stream.shutdown().await.unwrap();
    });

    let input = tokio::io::duplex(1024);
    let output = tokio::io::duplex(1024);
    let (mut input_writer, input_reader) = input;
    let (output_writer, mut output_reader) = output;

    input_writer.write_all(&client_frame).await.unwrap();
    input_writer.shutdown().await.unwrap();

    run_attach_proxy_at(socket_path.to_str().unwrap(), input_reader, output_writer)
        .await
        .unwrap();

    let mut received = Vec::new();
    output_reader.read_to_end(&mut received).await.unwrap();
    assert_eq!(received, server_frame);
    server.await.unwrap();
}

#[tokio::test]
async fn attach_proxy_exits_when_socket_closes_before_stdin() {
    let tmp = TempDir::new().unwrap();
    let socket_path = short_socket_path(&tmp, "proxy.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server_frame = vec![0x84, 0x00, 0x00, 0x00, 0x00];
    let server_frame_for_task = server_frame.clone();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        jackin_protocol::capsule_transport::server_handshake_async(&mut stream)
            .await
            .unwrap();
        stream.write_all(&server_frame_for_task).await.unwrap();
        stream.shutdown().await.unwrap();
    });

    let (_input_writer, input_reader) = tokio::io::duplex(1024);
    let (output_writer, mut output_reader) = tokio::io::duplex(1024);

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        run_attach_proxy_at(socket_path.to_str().unwrap(), input_reader, output_writer),
    )
    .await
    .expect("proxy should exit after socket EOF")
    .unwrap();

    let mut received = Vec::new();
    output_reader.read_to_end(&mut received).await.unwrap();
    assert_eq!(received, server_frame);
    server.await.unwrap();
}

fn short_socket_path(tmp: &TempDir, file_name: &str) -> PathBuf {
    tmp.path().join(file_name)
}

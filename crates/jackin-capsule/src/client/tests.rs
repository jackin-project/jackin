use super::*;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::net::UnixListener;

fn account(
    provider: &str,
    status: &str,
    source: &str,
    confidence: &str,
) -> AccountUsageSnapshotView {
    AccountUsageSnapshotView {
        provider: provider.to_owned(),
        account_label: format!("{provider} account"),
        source: source.to_owned(),
        confidence: confidence.to_owned(),
        window_kind: "Session".to_owned(),
        used_amount: Some(63),
        used_unit: Some("percent".to_owned()),
        limit_amount: Some(100),
        limit_unit: Some("percent".to_owned()),
        resets_at: Some(1_781_186_000),
        fetched_at: 1_781_185_680,
        expires_at: None,
        status: status.to_owned(),
        last_error: None,
    }
}

#[test]
fn usage_verify_accepts_trusted_rows_for_every_provider() {
    let accounts = [
        account("Codex", "fresh", "provider_api", "authoritative"),
        account("Claude", "fresh", "cli", "authoritative"),
        account("Amp", "fresh", "provider_api", "authoritative"),
        account("Grok Build", "fresh", "cli", "authoritative"),
        account("GLM / Z.AI", "fresh", "provider_api", "authoritative"),
        account("Kimi", "fresh", "provider_api", "authoritative"),
        account("MiniMax", "fresh", "provider_api", "authoritative"),
    ];

    let checks = verify_usage_accounts(&accounts);

    assert_eq!(checks.len(), 7);
    assert!(
        checks.iter().all(|check| check.status == "ok"),
        "{checks:?}"
    );
}

#[test]
fn usage_verify_reports_missing_and_untrusted_providers() {
    let mut untrusted = account("Codex", "needs_login", "none", "none");
    untrusted.account_label = "needs Codex login".to_owned();
    untrusted.last_error = Some("Codex auth not available".to_owned());
    let accounts = [
        untrusted,
        account("Amp", "fresh", "provider_api", "authoritative"),
    ];

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

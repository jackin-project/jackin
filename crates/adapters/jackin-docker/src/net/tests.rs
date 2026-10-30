// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[test]
fn user_agent_includes_crate_version() {
    assert!(USER_AGENT.starts_with("jackin/"), "got: {USER_AGENT}");
    let version = USER_AGENT.strip_prefix("jackin/").unwrap();
    assert!(!version.is_empty(), "version segment empty: {USER_AGENT}");
}

#[tokio::test]
async fn http_client_sends_user_agent_header() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 2048];
        let n = sock.read(&mut buf).await.unwrap();
        let request = String::from_utf8_lossy(&buf[..n]).to_string();
        sock.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .await
            .unwrap();
        let _shutdown = sock.shutdown().await;
        request
    });

    let client = http_client(HeaderMap::new()).unwrap();
    let body = get_text(&client, &format!("http://{addr}/")).await.unwrap();
    assert_eq!(body, "ok");

    let request = server.await.unwrap();
    let ua_line = request
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("user-agent:"))
        .unwrap_or_else(|| panic!("no User-Agent header in request:\n{request}"));
    assert!(
        ua_line.contains(USER_AGENT),
        "User-Agent line {ua_line:?} missing {USER_AGENT:?}"
    );
}

async fn oversized_metadata_server(size: u64) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            let count = socket.read(&mut request).await.unwrap();
            let range = String::from_utf8_lossy(&request[..count])
                .to_ascii_lowercase()
                .contains("range: bytes=0-0");
            let response = if range {
                format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/{size}\r\nContent-Length: 1\r\nConnection: close\r\n\r\nx"
                )
            } else {
                format!("HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n")
            };
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
        }
    });
    (format!("http://{address}/artifact"), server)
}

#[tokio::test]
async fn untrusted_size_cannot_allocate_before_authenticated_size_check() {
    let temporary = tempfile::tempdir().unwrap();
    let destination = temporary.path().join("uncreated-parent/artifact");
    let (url, server) = oversized_metadata_server(16).await;
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        download_parallel(&url, &destination, Some(8)),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.to_string().contains("authenticated artifact size"));
    assert!(!destination.parent().unwrap().exists());
    server.await.unwrap();
}

#[tokio::test]
async fn downloads_without_signed_lengths_still_have_an_allocation_bound() {
    let temporary = tempfile::tempdir().unwrap();
    let destination = temporary.path().join("uncreated-parent/artifact");
    let (url, server) = oversized_metadata_server(MAX_DOWNLOAD_BYTES + 1).await;
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        download_parallel(&url, &destination, None),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.to_string().contains("supported artifact bound"));
    assert!(!destination.parent().unwrap().exists());
    server.await.unwrap();
}

#[tokio::test]
async fn authenticated_length_download_preserves_exact_artifact_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            let count = socket.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..count]).to_ascii_lowercase();
            let response = if request.contains("range: bytes=0-0") {
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/8\r\nContent-Length: 1\r\nConnection: close\r\n\r\na"
            } else if request.contains("range: bytes=") {
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-7/8\r\nContent-Length: 8\r\nConnection: close\r\n\r\nartifact"
            } else {
                "HTTP/1.1 200 OK\r\nContent-Length: 8\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\nartifact"
            };
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
        }
    });
    let temporary = tempfile::tempdir().unwrap();
    let destination = temporary.path().join("created-parent/artifact");
    tokio::time::timeout(
        Duration::from_secs(5),
        download_parallel(&format!("http://{address}/artifact"), &destination, Some(8)),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(tokio::fs::read(destination).await.unwrap(), b"artifact");
    server.await.unwrap();
}

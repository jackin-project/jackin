//! Tests for `net`.
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[test]
fn user_agent_includes_crate_version() {
    // The bug this guards against: an empty UA causes GitHub's API edge to
    // 403 ("Request forbidden by administrative rules") before any auth or
    // rate-limit logic runs. Concrete `jackin/<version>` is easier to
    // diagnose in remote access logs than a bare `jackin`.
    assert!(USER_AGENT.starts_with("jackin/"), "got: {USER_AGENT}");
    let version = USER_AGENT.strip_prefix("jackin/").unwrap();
    assert!(!version.is_empty(), "version segment empty: {USER_AGENT}");
}

/// Spin up a one-shot HTTP listener, fire a request from `http_client`, and
/// confirm `User-Agent` arrived. Guards against reqwest's "no default UA"
/// behaviour silently coming back after a refactor.
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

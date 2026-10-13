// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[expect(
    clippy::disallowed_methods,
    reason = "test-only delayed HTTP fixture runs on an owned OS helper thread"
)]
#[test]
fn openrouter_snapshot_carries_delayed_429_retry_after_to_broker_boundary() {
    use std::io::{Read as _, Write as _};
    use std::time::Duration;

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("429 fixture accept");
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).expect("429 fixture read");
        assert!(
            String::from_utf8_lossy(&request[..read]).contains("/key"),
            "fixture must receive the key request"
        );
        std::thread::sleep(Duration::from_millis(1_100));
        let body = "{\"error\":\"provider body mentions 429\"}";
        write!(
            stream,
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 37\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("429 fixture write");
    });

    let request_now = now_epoch().saturating_sub(120);
    let (view, rate_limit) = openrouter_snapshot_with_key_fetch(
        "opencode",
        Some("fixture-key"),
        &format!("http://{address}"),
        request_now,
        fetch_openrouter_key_usage,
    );
    server.join().expect("429 fixture server");

    assert_eq!(view.status, UsageSnapshotStatus::Error);
    assert!(
        view.last_error
            .as_deref()
            .is_some_and(|message| message.contains("429")),
        "typed HTTP status should remain visible in the snapshot message"
    );
    let retry_at = rate_limit
        .expect("429 must reach the rate-limit boundary")
        .retry_at_epoch
        .expect("numeric Retry-After must produce an absolute deadline");
    assert!(
        retry_at >= request_now + 100,
        "deadline must start from response receipt, not request start: retry_at={retry_at}, request_now={request_now}"
    );
}

#[test]
fn openrouter_actual_401_is_needs_login() {
    let (base_url, server) = one_shot_key_server(401, "unauthorized");
    let view =
        openrouter_snapshot_with_base("opencode", Some("fixture-key"), &base_url, 1_780_000_000);
    server.join().unwrap();

    assert_eq!(view.status, UsageSnapshotStatus::NeedsLogin);
    assert_eq!(
        view.last_error.as_deref(),
        Some("OpenRouter key HTTP 401 Unauthorized")
    );
}

#[test]
fn openrouter_malformed_key_payload_is_error() {
    let (base_url, server) = one_shot_key_server(200, "not-json");
    let view =
        openrouter_snapshot_with_base("opencode", Some("fixture-key"), &base_url, 1_780_000_000);
    server.join().unwrap();

    assert_eq!(view.status, UsageSnapshotStatus::Error);
    assert!(
        view.last_error
            .as_deref()
            .is_some_and(|error| error.starts_with("OpenRouter key decode failed:")),
        "malformed payload must retain decode failure"
    );
}

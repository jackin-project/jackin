//! Self-tests for the [`FixtureHttpServer`](super::FixtureHttpServer) harness.
//!
//! The client side speaks raw HTTP over `TcpStream`: the harness must stay
//! dependency-free, so its own tests cannot use an HTTP client crate either.

use super::{FixtureHttpServer, RecordedRequest, ScriptedResponse, redact_secrets};
use std::io::{Read, Result, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

fn get(addr: &std::net::SocketAddr, path: &str, extra_headers: &[(&str, &str)]) -> Result<String> {
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut request = format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n");
    for (name, value) in extra_headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

fn post(addr: &std::net::SocketAddr, path: &str, body: &str) -> Result<String> {
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

#[test]
fn replays_scripted_status_body_and_headers_in_order() -> Result<()> {
    let server = FixtureHttpServer::start(vec![
        ScriptedResponse::new(429, "slow down").with_retry_after_secs(2),
        ScriptedResponse::new(200, r#"{"ok":true}"#)
            .with_header("Content-Type", "application/json"),
    ])?;

    let first = get(&server.addr(), "/v1/models", &[])?;
    assert!(first.starts_with("HTTP/1.1 429"), "status line: {first}");
    assert!(first.contains("Retry-After: 2"), "headers: {first}");
    assert!(first.ends_with("slow down"), "body: {first}");

    let second = get(&server.addr(), "/v1/models", &[])?;
    assert!(second.starts_with("HTTP/1.1 200"), "status line: {second}");
    assert!(
        second.contains("Content-Type: application/json"),
        "headers: {second}"
    );
    assert!(second.ends_with(r#"{"ok":true}"#), "body: {second}");

    assert_eq!(server.request_count(), 2);
    assert_eq!(server.script_remaining(), 1, "last response sticks");
    Ok(())
}

#[test]
fn records_method_path_headers_and_body() -> Result<()> {
    let server = FixtureHttpServer::start(vec![ScriptedResponse::new(200, "ok")])?;
    let post_raw = post(&server.addr(), "/v1/chat?x=1", "hello")?;
    assert!(post_raw.starts_with("HTTP/1.1 200"), "raw: {post_raw}");
    let get_raw = get(
        &server.addr(),
        "/v1/models",
        &[("Authorization", "Bearer test-token-123")],
    )?;
    assert!(get_raw.starts_with("HTTP/1.1 200"), "raw: {get_raw}");

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/v1/chat?x=1");
    assert_eq!(requests[0].body_text(), "hello");
    assert_eq!(requests[1].method, "GET");
    assert_eq!(
        requests[1].header("authorization"),
        Some("Bearer test-token-123"),
        "header lookup is case-insensitive"
    );
    assert_eq!(
        requests[1].header("X-Missing"),
        None,
        "missing header yields None"
    );
    Ok(())
}

#[test]
fn unknown_status_uses_fallback_reason_phrase() -> Result<()> {
    let server = FixtureHttpServer::start(vec![ScriptedResponse::new(418, "teapot")])?;
    let raw = get(&server.addr(), "/", &[])?;
    assert!(raw.starts_with("HTTP/1.1 418 Unknown"), "raw: {raw}");
    Ok(())
}

#[test]
fn empty_script_serves_500_until_extended() -> Result<()> {
    let server = FixtureHttpServer::start(Vec::new())?;
    let raw = get(&server.addr(), "/", &[])?;
    assert!(raw.starts_with("HTTP/1.1 500"), "raw: {raw}");

    server.push_response(ScriptedResponse::new(200, "late"));
    let raw = get(&server.addr(), "/", &[])?;
    assert!(raw.starts_with("HTTP/1.1 200"), "raw: {raw}");
    assert!(raw.ends_with("late"), "raw: {raw}");
    Ok(())
}

#[test]
fn latency_injection_delays_response() -> Result<()> {
    let server = FixtureHttpServer::start(vec![
        ScriptedResponse::new(200, "slow").with_latency(Duration::from_millis(250)),
    ])?;
    let start = Instant::now();
    let raw = get(&server.addr(), "/", &[])?;
    let elapsed = start.elapsed();
    assert!(raw.ends_with("slow"), "raw: {raw}");
    assert!(
        elapsed >= Duration::from_millis(200),
        "latency too short: {elapsed:?}"
    );
    Ok(())
}

#[test]
fn url_points_at_bound_loopback_port() -> Result<()> {
    let server = FixtureHttpServer::start(vec![ScriptedResponse::new(200, "ok")])?;
    assert!(server.addr().ip().is_loopback());
    assert_ne!(server.addr().port(), 0);
    assert_eq!(server.url(), format!("http://{}", server.addr()));
    Ok(())
}

#[test]
fn redact_secrets_masks_every_occurrence() {
    let text = "token abc123 then abc123 again";
    assert_eq!(
        redact_secrets(text, &["abc123"]),
        "token <redacted> then <redacted> again"
    );
    assert_eq!(
        redact_secrets(text, &["", "missing"]),
        text,
        "empty and absent secrets are no-ops"
    );
}

#[test]
fn assert_no_secrets_passes_on_clean_request() {
    let request = RecordedRequest {
        method: "GET".to_owned(),
        path: "/v1/models".to_owned(),
        headers: vec![(
            "Authorization".to_owned(),
            "Bearer test-token-123".to_owned(),
        )],
        body: b"{}".to_vec(),
    };
    request.assert_no_secrets(&["real-secret", ""]);
}

#[test]
#[should_panic(expected = "forbidden value #1")]
fn assert_no_secrets_panics_without_echoing_secret() {
    let request = RecordedRequest {
        method: "GET".to_owned(),
        path: "/v1/models".to_owned(),
        headers: Vec::new(),
        body: b"key=hunter2-value".to_vec(),
    };
    request.assert_no_secrets(&["other", "hunter2-value"]);
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn codex_over_cap_keeps_raw_label_with_clamped_bar() {
    let window: CodexWindowSnapshot =
        serde_json::from_value(serde_json::json!({"used_percent": 142}))
            .expect("over-cap window decodes");
    assert_eq!(window.used_percent_raw(), Some(142.0));
    assert_eq!(window.used_percent_clamped(), Some(100));
    let mut buckets = Vec::new();
    push_codex_window(
        &mut buckets,
        "Session",
        Some(StatusSlot::Session),
        Some(&window),
        1_781_185_560,
    );
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].used_label.as_deref(), Some("142% used"));
    assert_eq!(buckets[0].remaining_percent, Some(0));

    // In-range labels are unchanged; negatives floor at 0, never "-3%".
    let normal: CodexWindowSnapshot =
        serde_json::from_value(serde_json::json!({"used_percent": 63.7}))
            .expect("fractional window decodes");
    let mut buckets = Vec::new();
    push_codex_window(&mut buckets, "Session", None, Some(&normal), 1_781_185_560);
    assert_eq!(buckets[0].used_label.as_deref(), Some("64% used"));
    let negative: CodexWindowSnapshot =
        serde_json::from_value(serde_json::json!({"used_percent": -3}))
            .expect("negative window decodes");
    let mut buckets = Vec::new();
    push_codex_window(
        &mut buckets,
        "Session",
        None,
        Some(&negative),
        1_781_185_560,
    );
    assert_eq!(buckets[0].used_label.as_deref(), Some("0% used"));
    assert_eq!(buckets[0].remaining_percent, Some(100));
}

#[expect(
    clippy::disallowed_methods,
    reason = "test-only poll backoff on an owned OS helper thread (a lint-sanctioned use); bounds the canned server so a missing reset-credits GET fails instead of hanging the suite"
)]
#[test]
fn profile_snapshot_surfaces_reset_credits_from_fixture() {
    use std::io::{Read as _, Write as _};

    // Mirrors the 2 live Full-reset grants.
    const USAGE_FIXTURE: &str = r#"{"plan_type": "pro", "rate_limit": {"primary_window": {"used_percent": 10, "reset_after_seconds": 3600, "limit_window_seconds": 18000}, "secondary_window": {"used_percent": 5, "reset_after_seconds": 604800, "limit_window_seconds": 604800}}}"#;
    const RESET_FIXTURE: &str = r#"{"credits": [{"status": "available", "expires_at": "2030-01-01T00:00:00Z"}, {"status": "available", "expires_at": "2030-02-01T00:00:00Z"}], "available_count": 2}"#;

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    // Bounded poll (not blocking accept): if the profile lane ever stops
    // issuing the reset-credits GET, this fails instead of hanging the suite.
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut served = 0;
        while served < 2 && Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(accepted) => accepted,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("canned codex server accept failed: {error}"),
            };
            stream.set_nonblocking(false).expect("canned stream blocks");
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).expect("canned read");
            let text = String::from_utf8_lossy(&request[..read]);
            let body = if text.contains("rate-limit-reset-credits") {
                RESET_FIXTURE
            } else {
                USAGE_FIXTURE
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("canned write");
            served += 1;
        }
        served
    });
    let home = tempfile::tempdir().unwrap();
    fs::write(
        home.path().join("config.toml"),
        format!("chatgpt_base_url = \"http://{address}\"\n"),
    )
    .unwrap();
    let credentials = CodexOAuthCredentials {
        access_token: "fixture-token".to_owned(),
        account_id: None,
        account_label: Some("codex@example.test".to_owned()),
        refresh_token: None,
    };
    let view = codex_profile_snapshot("codex", &credentials, home.path(), 1_781_728_000);
    assert_eq!(
        server.join().expect("canned server"),
        2,
        "profile lane must issue both the usage and reset-credits GETs"
    );

    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    let reset = view
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Limit Reset Credits")
        .expect("profile lane surfaces the reset-credits bucket");
    let detail = reset.pace_label.as_deref().unwrap_or_default();
    assert!(
        detail.starts_with("2 manual resets available"),
        "unexpected reset detail: {detail}"
    );
}

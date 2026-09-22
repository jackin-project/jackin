// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn cli_fallback_last_error_uses_normalized_scope_message() {
    let oauth_error = ProviderError::from(ProviderHttpError::HttpStatus {
        status: 403,
        message: "Claude OAuth usage HTTP 403 Forbidden".to_owned(),
        retry_after: None,
        response_received_at_epoch: 1_700_000_000,
    });
    let normalized =
        claude_provider_error_label(Some(&oauth_error), None).expect("normalized label");
    assert_eq!(
        claude_resolved_last_error(UsageSnapshotStatus::Fresh, Some(normalized), true).as_deref(),
        Some("Claude token lacks usage scope (inference-only); quota unavailable")
    );
    // Non-scope errors pass through verbatim; OAuth success has no error.
    assert_eq!(
        claude_resolved_last_error(
            UsageSnapshotStatus::Fresh,
            Some("oauth boom".to_owned()),
            true
        )
        .as_deref(),
        Some("oauth boom")
    );
    assert_eq!(
        claude_resolved_last_error(
            UsageSnapshotStatus::Fresh,
            Some("oauth boom".to_owned()),
            false
        ),
        None
    );
    assert_eq!(
        claude_resolved_last_error(UsageSnapshotStatus::Stale, None, false).as_deref(),
        Some("Claude provider usage unavailable; cached quota is stale")
    );
}

#[test]
fn scope_restriction_requires_typed_http_403() {
    let misleading = [
        ProviderError::from(ProviderHttpError::Transport(
            "Claude OAuth usage request failed: status 403".to_owned(),
        )),
        ProviderError::from(ProviderHttpError::Decode(
            "Claude OAuth usage decode failed: payload mentions 401".to_owned(),
        )),
        ProviderError::from("Claude CLI usage failed with HTTP 429".to_owned()),
    ];
    for error in &misleading {
        assert!(!claude_error_is_scope_restriction(error));
    }

    assert!(claude_error_is_scope_restriction(&ProviderError::from(
        ProviderHttpError::HttpStatus {
            status: 403,
            message: "message mentions HTTP 401".to_owned(),
            retry_after: None,
            response_received_at_epoch: 1_700_000_000,
        },
    )));
    for status in [401, 429] {
        assert!(!claude_error_is_scope_restriction(&ProviderError::from(
            ProviderHttpError::HttpStatus {
                status,
                message: if status == 401 {
                    "message mentions HTTP 403".to_owned()
                } else {
                    "message mentions HTTP 401".to_owned()
                },
                retry_after: None,
                response_received_at_epoch: 1_700_000_000,
            },
        )));
    }
}

#[test]
fn claude_actual_429_preserves_typed_retry_after_and_response_clock() {
    use std::io::{Read as _, Write as _};

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("Claude 429 fixture accept");
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).expect("Claude 429 fixture read");
        assert!(read > 0, "Claude fixture must receive the usage request");
        let body = "provider body mentions 429";
        write!(
            stream,
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 37\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("Claude 429 fixture write");
    });

    let response_epoch = 1_780_000_200;
    let error = fetch_claude_oauth_usage_with_response_clock(
        "fixture-token",
        &format!("http://{address}/usage"),
        || response_epoch,
    )
    .expect_err("fixture must return typed HTTP 429");
    server.join().expect("Claude 429 fixture server");

    let resolved = ClaudeResolved {
        access_token: "fixture-token".to_owned(),
        subscription_type: None,
        account_email: None,
        organization_type: None,
        credential_origin: "OAuth · test".to_owned(),
        is_anonymous: true,
    };
    let (view, rate_limit) = claude_view_from_wave_with_rate_limit_using(
        "claude",
        Some("Claude"),
        1_780_000_000,
        ClaudeWaveResolution::Resolved(Box::new(resolved)),
        move |_| Err(error),
        || Err(ProviderError::from("CLI fallback disabled".to_owned())),
    );

    assert_eq!(view.status, UsageSnapshotStatus::Stale);
    assert_eq!(
        rate_limit,
        Some(ProviderRateLimit {
            retry_at_epoch: Some(response_epoch + 37),
        })
    );
}

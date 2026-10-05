// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn cli_fallback_last_error_uses_normalized_scope_message() {
    let oauth_error = ProviderError::from(ProviderHttpError::HttpStatus {
        status: 403,
        message: "Claude OAuth usage HTTP 403 Forbidden".to_owned(),
        retry_after_seconds: None,
        response_received_at_epoch: None,
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
            retry_after_seconds: None,
            response_received_at_epoch: None,
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
                retry_after_seconds: None,
                response_received_at_epoch: None,
            },
        )));
    }
}

#[test]
fn captured_profile_oauth_failure_never_dispatches_ambient_cli() {
    for status in [401, 403, 429, 500] {
        let calls = std::cell::Cell::new(0);
        let error = ProviderError::from(ProviderHttpError::HttpStatus {
            status,
            message: "fixture OAuth failure".to_owned(),
            retry_after_seconds: None,
            response_received_at_epoch: None,
        });
        let (oauth, cli) = claude_fetch_for_authority(
            ClaudeUsageAuthority::CapturedProfile,
            Err(error),
            || {
                calls.set(calls.get() + 1);
                Err(ProviderError::from("fixture CLI failure".to_owned()))
            },
        );
        assert_eq!(oauth.err().and_then(|error| error.status()), Some(status));
        assert!(cli.is_none());
        assert_eq!(calls.get(), 0, "captured token failure cannot use host CLI");
    }
}

#[test]
fn standalone_ambient_oauth_failure_retains_cli_fallback() {
    let calls = std::cell::Cell::new(0);
    let (oauth, cli) = claude_fetch_for_authority(
        ClaudeUsageAuthority::Ambient,
        Err(ProviderError::from("fixture OAuth failure".to_owned())),
        || {
            calls.set(calls.get() + 1);
            Err(ProviderError::from("fixture CLI failure".to_owned()))
        },
    );
    assert!(oauth.is_err());
    assert!(cli.is_some_and(|result| result.is_err()));
    assert_eq!(calls.get(), 1);
}

#[test]
fn successful_oauth_preserves_its_quota_without_cli_dispatch() {
    for authority in [
        ClaudeUsageAuthority::CapturedProfile,
        ClaudeUsageAuthority::Ambient,
    ] {
        let calls = std::cell::Cell::new(0);
        let usage = serde_json::from_value(serde_json::json!({
            "five_hour": {"utilization": 17.0, "resets_at": "2026-10-03T00:00:00Z"}
        }))
        .expect("sanitized OAuth fixture");
        let (oauth, cli) = claude_fetch_for_authority(authority, Ok(usage), || {
            calls.set(calls.get() + 1);
            Err(ProviderError::from("fixture CLI failure".to_owned()))
        });
        let usage = oauth.expect("OAuth quota retained");
        assert_eq!(
            usage.five_hour.and_then(|window| window.utilization),
            Some(17.0)
        );
        assert!(cli.is_none());
        assert_eq!(calls.get(), 0);
    }
}

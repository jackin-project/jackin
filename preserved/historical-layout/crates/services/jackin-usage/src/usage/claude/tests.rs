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

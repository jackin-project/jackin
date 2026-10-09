// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn scope_restriction_requires_typed_http_403_and_keeps_status_in_error() {
    let misleading = [
        ProviderError::from(ProviderHttpError::Transport(
            "Claude OAuth usage request failed: status 403".to_owned(),
        )),
        ProviderError::from(ProviderHttpError::Decode(
            "Claude OAuth usage decode failed: payload mentions 401".to_owned(),
        )),
        ProviderError::from("ordinary error mentions HTTP 403".to_owned()),
    ];
    for error in &misleading {
        assert!(!claude_error_is_scope_restriction(error));
    }

    for status in [401, 403, 429] {
        let error = ProviderError::from(ProviderHttpError::HttpStatus {
            status,
            message: format!("Claude OAuth usage HTTP {status}"),
            retry_after_seconds: None,
            response_received_at_epoch: None,
        });
        assert_eq!(claude_error_is_scope_restriction(&error), status == 403);
        let label = claude_provider_error_label(Some(&error)).expect("typed error label");
        assert!(label.contains(&format!("HTTP {status}")));
        if status == 403 {
            assert!(label.contains("inference-only"));
        }
    }

    assert_eq!(claude_provider_error_label(None), None);
}

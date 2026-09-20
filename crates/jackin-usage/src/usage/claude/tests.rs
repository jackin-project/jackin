// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn cli_fallback_last_error_uses_normalized_scope_message() {
    let normalized =
        claude_provider_error_label(Some("Claude OAuth usage HTTP 403 Forbidden"), None)
            .expect("normalized label");
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

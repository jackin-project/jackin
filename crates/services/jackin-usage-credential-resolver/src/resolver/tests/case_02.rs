// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn claude_api_key_snapshot_does_not_use_oauth_adapter() {
    let view = provider_credential_snapshot(
        "claude",
        jackin_core::ANTHROPIC_API_KEY_ENV_NAME,
        "fixture-api-key",
    );

    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.account.account_label, "Claude API key");
    assert_eq!(
        view.account.credential_origin.as_deref(),
        Some("API key · env ANTHROPIC_API_KEY")
    );
    assert_eq!(
        view.last_error.as_deref(),
        Some("Claude API-key quota is unavailable; OAuth usage requires CLAUDE_CODE_OAUTH_TOKEN")
    );
}

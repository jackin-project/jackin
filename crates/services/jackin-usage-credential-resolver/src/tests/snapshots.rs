// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::UsageSnapshotStatus;

use crate::provider_credential_snapshot;

#[test]
fn unknown_surface_reports_unsupported_snapshot() {
    let view = provider_credential_snapshot("no-such-surface", "SOME_KEY", "secret");

    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.focused_agent.as_deref(), Some("no-such-surface"));
    assert!(view.last_error.is_some());
}

#[test]
fn opencode_surface_with_foreign_key_reports_unsupported_snapshot() {
    let view = provider_credential_snapshot("opencode", "FOREIGN_KEY", "secret");

    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
}

#[test]
fn claude_api_key_surface_reports_key_view_without_network() {
    let view = provider_credential_snapshot("claude", "ANTHROPIC_API_KEY", "secret");

    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.focused_agent.as_deref(), Some("claude"));
    assert_eq!(view.account.account_label, "Claude API key");
}

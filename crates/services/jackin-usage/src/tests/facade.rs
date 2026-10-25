// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::UsageSnapshotStatus;

use crate::usage::provider_credential_snapshot;

#[test]
fn facade_unknown_surface_reports_unsupported_snapshot() {
    let view = provider_credential_snapshot("no-such-surface", "SOME_KEY", "secret");

    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.focused_agent.as_deref(), Some("no-such-surface"));
    assert!(view.last_error.is_some());
}

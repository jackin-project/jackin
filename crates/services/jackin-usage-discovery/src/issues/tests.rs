// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn debug_redacts_scope_labels() {
    let diagnostic = UsageDiscoveryDiagnostic {
        surface_id: Some("claude".to_owned()),
        scope_label: "/private/config/private-account".to_owned(),
        unresolved_source: Some(UsageDiscoveryUnresolvedSource {
            capability_id: "opaque-source-id".to_owned(),
            configuration_count: 1,
        }),
        issue: UsageDiscoveryIssue::InteractionRequired,
    };
    let debug = format!("{diagnostic:?}");

    assert!(debug.contains("opaque-source-id"));
    assert!(!debug.contains("/private/config/private-account"));
}

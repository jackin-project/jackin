// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn account_diagnostic_keeps_only_opaque_declared_source_identity() {
    let account_id = "private-account-alias";
    let route_key = "ANTHROPIC_API_KEY";
    let diagnostic = account_diagnostic(
        HostSurfaceId::Claude,
        account_id,
        Some(route_key),
        2,
        UsageDiscoveryIssue::InteractionRequired,
    );
    let debug = format!("{diagnostic:?}");
    let source = diagnostic.unresolved_source.as_ref().unwrap();

    assert_eq!(diagnostic.scope_label, "account");
    assert_eq!(source.configuration_count, 2);
    assert!(!source.capability_id.contains(account_id));
    assert!(!source.capability_id.contains(route_key));
    assert!(!debug.contains(account_id));
    assert!(!debug.contains(route_key));
}

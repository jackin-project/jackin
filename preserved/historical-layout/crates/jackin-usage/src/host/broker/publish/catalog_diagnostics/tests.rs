// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::usage_broker::{UsageProjectionRefreshStateV1, UsageProjectionSchemaV1};

#[test]
fn cleared_diagnostic_removes_empty_provider_row() {
    let mut projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "fixture:0".to_owned(),
        generated_at_epoch: 0,
        discovery_revision: "fixture".to_owned(),
        broker_instance_id: "fixture".to_owned(),
        broker_generation: 0,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        issues: Vec::new(),
    };
    let mut diagnostics = CatalogDiagnostics::default();
    diagnostics.push_provider_issue(
        "claude",
        "Anthropic",
        CatalogDiagnosticCode::InteractionRequired,
    );
    diagnostics.push_unresolved("claude", "Anthropic", "opaque-candidate".to_owned(), 1);

    apply_catalog_diagnostics(&mut projection, &diagnostics).unwrap();
    assert_eq!(projection.providers.len(), 1);
    assert_eq!(projection.providers[0].provider_id, "anthropic");
    assert_eq!(projection.providers[0].display_name, "Anthropic");
    assert_eq!(
        projection.providers[0].issues[0].code,
        "interaction_required"
    );
    assert_eq!(projection.unresolved.len(), 1);
    assert_eq!(projection.unresolved[0].provider_id, "anthropic");

    apply_catalog_diagnostics(&mut projection, &CatalogDiagnostics::default()).unwrap();
    assert!(projection.providers.is_empty());
}

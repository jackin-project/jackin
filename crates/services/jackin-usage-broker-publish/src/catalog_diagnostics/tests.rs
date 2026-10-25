// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::usage_broker::{UsageProjectionRefreshStateV1, UsageProjectionSchemaV2};

#[test]
fn catalog_unresolved_states_preserve_unavailable_distinction() {
    for (codes, expected) in [
        (vec![], UsageLifecycleV1::Error),
        (
            vec![CatalogDiagnosticCode::ConfigUnreadable],
            UsageLifecycleV1::Error,
        ),
        (
            vec![CatalogDiagnosticCode::ConfigInvalid],
            UsageLifecycleV1::Error,
        ),
        (
            vec![CatalogDiagnosticCode::ConfigVersionUnsupported],
            UsageLifecycleV1::Unsupported,
        ),
        (
            vec![CatalogDiagnosticCode::ConfigTransientConflict],
            UsageLifecycleV1::Error,
        ),
        (
            vec![CatalogDiagnosticCode::CredentialMissing],
            UsageLifecycleV1::NeedsSecret,
        ),
        (
            vec![CatalogDiagnosticCode::CredentialDenied],
            UsageLifecycleV1::NeedsSecret,
        ),
        (
            vec![CatalogDiagnosticCode::KeychainAccessUnavailable],
            UsageLifecycleV1::Unavailable,
        ),
        (
            vec![CatalogDiagnosticCode::CredentialMalformed],
            UsageLifecycleV1::NeedsSecret,
        ),
        (
            vec![CatalogDiagnosticCode::InteractionRequired],
            UsageLifecycleV1::NeedsSecret,
        ),
    ] {
        assert_eq!(unresolved_state(codes.into_iter()), expected);
    }

    for codes in [
        vec![
            CatalogDiagnosticCode::KeychainAccessUnavailable,
            CatalogDiagnosticCode::CredentialMissing,
        ],
        vec![
            CatalogDiagnosticCode::CredentialMissing,
            CatalogDiagnosticCode::KeychainAccessUnavailable,
        ],
    ] {
        assert_eq!(
            unresolved_state(codes.into_iter()),
            UsageLifecycleV1::Unavailable
        );
    }

    for codes in [
        vec![
            CatalogDiagnosticCode::KeychainAccessUnavailable,
            CatalogDiagnosticCode::ConfigVersionUnsupported,
        ],
        vec![
            CatalogDiagnosticCode::ConfigVersionUnsupported,
            CatalogDiagnosticCode::KeychainAccessUnavailable,
        ],
    ] {
        assert_eq!(
            unresolved_state(codes.into_iter()),
            UsageLifecycleV1::Unsupported
        );
    }
}

#[test]
fn cleared_diagnostic_removes_empty_provider_row() {
    let mut projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV2,
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

    apply_catalog_diagnostics(&mut projection, &diagnostics);
    assert_eq!(projection.providers.len(), 1);
    assert_eq!(
        projection.providers[0].issues[0].code,
        "interaction_required"
    );

    apply_catalog_diagnostics(&mut projection, &CatalogDiagnostics::default());
    assert!(projection.providers.is_empty());
}

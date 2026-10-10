// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::host::UsageCredentialKind;
use crate::host::discovery::{
    UsageDiscoveryDiagnostic, UsageDiscoveryUnresolvedSource, UsageSourceCandidateDescriptor,
    ValidatedCredentialBinding, ValidatedCredentialSource,
};
use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject};
use std::collections::BTreeSet;

#[test]
fn interaction_diagnostics_are_typed_and_do_not_export_scope_text() {
    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("fixture-revision".to_owned()),
        accounts: Vec::new(),
        diagnostics: vec![UsageDiscoveryDiagnostic {
            surface_id: Some("claude".to_owned()),
            scope_label: "/private/token/path must not be projected".to_owned(),
            unresolved_source: Some(UsageDiscoveryUnresolvedSource {
                capability_id: "opaque-source-fixture".to_owned(),
                configuration_count: 1,
            }),
            issue: UsageDiscoveryIssue::InteractionRequired,
        }],
        candidates: vec![UsageSourceCandidateDescriptor {
            surface_id: "claude".to_owned(),
            credential_kind: UsageCredentialKind::ForwardedCapability,
            source_id: "fixture-source".to_owned(),
            capability_id: "opaque-candidate".to_owned(),
            provenance: vec!["fixture-scope".to_owned()],
        }],
        bindings: vec![ValidatedCredentialBinding {
            surface: HostSurfaceId::Claude,
            identity: Some(CanonicalAccountIdentity {
                surface: HostSurfaceId::Claude,
                subject: CanonicalAccountSubject::ProviderStableHandle(
                    "fixture-account".to_owned(),
                ),
            }),
            source_id: "fixture-source".to_owned(),
            capability_id: "opaque-candidate".to_owned(),
            credential_revision: "fixture-revision".to_owned(),
            provenance: BTreeSet::from(["fixture-scope".to_owned()]),
            source: ValidatedCredentialSource::Capability,
        }],
    };

    let mapped = from_discovery(&discovery);
    let debug = format!("{mapped:?}");
    assert!(debug.contains("InteractionRequired"));
    assert!(
        !debug.contains("opaque-candidate"),
        "a candidate with validated identity is not an unresolved source"
    );
    assert!(debug.contains("opaque-source-fixture"));
    assert!(!debug.contains("/private/token/path"));
}

#[test]
fn unknown_surface_diagnostics_are_dropped() {
    let discovery = ValidatedUsageDiscovery {
        config_generation: None,
        accounts: Vec::new(),
        diagnostics: vec![UsageDiscoveryDiagnostic {
            surface_id: Some("/private/provider/path".to_owned()),
            scope_label: "secret fixture value".to_owned(),
            unresolved_source: None,
            issue: UsageDiscoveryIssue::InteractionRequired,
        }],
        candidates: Vec::new(),
        bindings: Vec::new(),
    };

    assert_eq!(from_discovery(&discovery), CatalogDiagnostics::default());
}

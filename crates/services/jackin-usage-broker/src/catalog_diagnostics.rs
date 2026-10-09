// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Converts validated discovery diagnostics into the secret-free projection input.

use crate::publish::{CatalogDiagnosticCode, CatalogDiagnostics};
use jackin_usage_discovery::{UsageDiscoveryIssue, ValidatedUsageDiscovery};
use jackin_usage_host_presentation::HostSurfaceId;

pub(crate) fn from_discovery(discovery: &ValidatedUsageDiscovery) -> CatalogDiagnostics {
    let mut diagnostics = CatalogDiagnostics::default();
    for diagnostic in &discovery.diagnostics {
        let code = issue_code(diagnostic.issue);
        match diagnostic.surface_id.as_deref() {
            Some(surface_id) => {
                let Some(surface) = surface_by_id(surface_id) else {
                    continue;
                };
                diagnostics.push_provider_issue(surface.id(), surface.label(), code);
                if let Some(source) = &diagnostic.unresolved_source {
                    diagnostics.push_unresolved(
                        surface.id(),
                        surface.label(),
                        source.capability_id.clone(),
                        source.configuration_count,
                    );
                }
            }
            None => diagnostics.push_projection_issue(code),
        }
    }

    for candidate in discovery.unresolved_capabilities() {
        let Some(surface) = surface_by_id(&candidate.surface_id) else {
            continue;
        };
        diagnostics.push_unresolved(
            surface.id(),
            surface.label(),
            candidate.capability_id.clone(),
            u32::try_from(candidate.provenance.len()).unwrap_or(u32::MAX),
        );
    }
    diagnostics
}

fn surface_by_id(id: &str) -> Option<HostSurfaceId> {
    HostSurfaceId::ALL
        .iter()
        .copied()
        .find(|surface| surface.id() == id)
}

const fn issue_code(issue: UsageDiscoveryIssue) -> CatalogDiagnosticCode {
    match issue {
        UsageDiscoveryIssue::ConfigUnreadable => CatalogDiagnosticCode::ConfigUnreadable,
        UsageDiscoveryIssue::ConfigInvalid => CatalogDiagnosticCode::ConfigInvalid,
        UsageDiscoveryIssue::ConfigVersionUnsupported => {
            CatalogDiagnosticCode::ConfigVersionUnsupported
        }
        UsageDiscoveryIssue::ConfigTransientConflict => {
            CatalogDiagnosticCode::ConfigTransientConflict
        }
        UsageDiscoveryIssue::CredentialMissing => CatalogDiagnosticCode::CredentialMissing,
        UsageDiscoveryIssue::CredentialDenied => CatalogDiagnosticCode::CredentialDenied,
        UsageDiscoveryIssue::KeychainConsentRequired => {
            CatalogDiagnosticCode::KeychainConsentRequired
        }
        UsageDiscoveryIssue::CredentialMalformed => CatalogDiagnosticCode::CredentialMalformed,
        UsageDiscoveryIssue::InteractionRequired => CatalogDiagnosticCode::InteractionRequired,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jackin_usage_discovery::{
        UsageDiscoveryDiagnostic, UsageSourceCandidateDescriptor, ValidatedCredentialBinding,
        ValidatedCredentialSource,
    };
    use jackin_usage_host_credentials::UsageCredentialKind;
    use std::collections::BTreeSet;

    #[test]
    fn interaction_required_maps_to_typed_codes_without_scope_text() {
        let discovery = ValidatedUsageDiscovery {
            config_generation: Some("fixture-revision".to_owned()),
            accounts: Vec::new(),
            diagnostics: vec![
                UsageDiscoveryDiagnostic {
                    surface_id: Some("claude".to_owned()),
                    scope_label: "/private/token/path must not be projected".to_owned(),
                    unresolved_source: Some(
                        jackin_usage_discovery::UsageDiscoveryUnresolvedSource {
                            capability_id: "opaque-source-fixture".to_owned(),
                            configuration_count: 1,
                        },
                    ),
                    issue: UsageDiscoveryIssue::InteractionRequired,
                },
                UsageDiscoveryDiagnostic {
                    surface_id: None,
                    scope_label: "secret fixture value".to_owned(),
                    unresolved_source: None,
                    issue: UsageDiscoveryIssue::ConfigTransientConflict,
                },
            ],
            candidates: vec![UsageSourceCandidateDescriptor {
                surface_id: "claude".to_owned(),
                credential_kind: UsageCredentialKind::ForwardedCapability,
                source_id: "fixture-source".to_owned(),
                capability_id: "opaque-candidate".to_owned(),
                provenance: vec!["fixture-scope".to_owned()],
            }],
            bindings: vec![ValidatedCredentialBinding {
                surface: HostSurfaceId::Claude,
                identity: None,
                source_id: "fixture-source".to_owned(),
                capability_id: "opaque-candidate".to_owned(),
                credential_revision: "fixture-credential-revision".to_owned(),
                provenance: BTreeSet::from(["fixture-scope".to_owned()]),
                source: ValidatedCredentialSource::Capability,
            }],
        };

        let mapped = from_discovery(&discovery);
        let debug = format!("{mapped:?}");
        assert!(debug.contains("InteractionRequired"));
        assert!(debug.contains("ConfigTransientConflict"));
        assert!(debug.contains("opaque-candidate"));
        assert!(debug.contains("opaque-source-fixture"));
        assert!(!debug.contains("/private/token/path"));
        assert!(!debug.contains("secret fixture value"));
    }

    #[test]
    fn unknown_surface_diagnostic_is_dropped_instead_of_exported() {
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
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Converts validated discovery diagnostics into secret-free projection data.

use crate::host::{HostSurfaceId, UsageDiscoveryIssue, ValidatedUsageDiscovery};

use super::publish::{CatalogDiagnosticCode, CatalogDiagnostics};

pub(super) fn from_discovery(discovery: &ValidatedUsageDiscovery) -> CatalogDiagnostics {
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
mod tests;

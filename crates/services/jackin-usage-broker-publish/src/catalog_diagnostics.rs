// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Typed, secret-free diagnostics from one broker catalog scan.

use std::collections::{BTreeMap, BTreeSet};

use jackin_protocol::usage_broker::{
    UsageFreshnessPhaseV1, UsageFreshnessV1, UsageIssueRecoverabilityV1, UsageIssueScopeV1,
    UsageIssueV1, UsageLifecycleV1, UsageMembershipStateV1, UsageProjectionV1, UsageProviderV1,
    UsageUnresolvedV1,
};

/// Closed set of discovery issue categories exported through the projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CatalogDiagnosticCode {
    /// The config source could not be read.
    ConfigUnreadable,
    /// The config source was malformed or invalid.
    ConfigInvalid,
    /// The config schema is newer than the supported schema.
    ConfigVersionUnsupported,
    /// Repeated reads observed a changing config generation.
    ConfigTransientConflict,
    /// A required credential source is absent.
    CredentialMissing,
    /// Protected credential access was denied or unavailable.
    CredentialDenied,
    /// The operator must approve protected credential access.
    KeychainConsentRequired,
    /// The credential source is malformed.
    CredentialMalformed,
    /// Credential access requires operator interaction.
    InteractionRequired,
}

impl CatalogDiagnosticCode {
    const fn id(self) -> &'static str {
        match self {
            Self::ConfigUnreadable => "config_unreadable",
            Self::ConfigInvalid => "config_invalid",
            Self::ConfigVersionUnsupported => "config_version_unsupported",
            Self::ConfigTransientConflict => "config_transient_conflict",
            Self::CredentialMissing => "credential_missing",
            Self::CredentialDenied => "credential_denied",
            Self::KeychainConsentRequired => "keychain_consent_required",
            Self::CredentialMalformed => "credential_malformed",
            Self::InteractionRequired => "interaction_required",
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::ConfigUnreadable => "Configuration could not be read",
            Self::ConfigInvalid => "Configuration is invalid",
            Self::ConfigVersionUnsupported => "Configuration version is not supported",
            Self::ConfigTransientConflict => "Configuration changed while it was being read",
            Self::CredentialMissing => "Credentials are missing",
            Self::CredentialDenied => "Credential access was denied",
            Self::KeychainConsentRequired => {
                "Keychain consent required; approve jackin in Keychain Access"
            }
            Self::CredentialMalformed => "Credentials are malformed",
            Self::InteractionRequired => "Credential access requires interaction",
        }
    }

    const fn recoverability(self) -> UsageIssueRecoverabilityV1 {
        match self {
            Self::ConfigUnreadable | Self::ConfigTransientConflict => {
                UsageIssueRecoverabilityV1::Retryable
            }
            Self::ConfigVersionUnsupported => UsageIssueRecoverabilityV1::Unsupported,
            Self::ConfigInvalid
            | Self::CredentialMissing
            | Self::CredentialDenied
            | Self::KeychainConsentRequired
            | Self::CredentialMalformed
            | Self::InteractionRequired => UsageIssueRecoverabilityV1::ActionRequired,
        }
    }

    const fn needs_secret(self) -> bool {
        matches!(
            self,
            Self::CredentialMissing
                | Self::CredentialDenied
                | Self::KeychainConsentRequired
                | Self::CredentialMalformed
                | Self::InteractionRequired
        )
    }

    fn from_id(id: &str) -> Option<Self> {
        match id {
            "config_unreadable" => Some(Self::ConfigUnreadable),
            "config_invalid" => Some(Self::ConfigInvalid),
            "config_version_unsupported" => Some(Self::ConfigVersionUnsupported),
            "config_transient_conflict" => Some(Self::ConfigTransientConflict),
            "credential_missing" => Some(Self::CredentialMissing),
            "credential_denied" => Some(Self::CredentialDenied),
            "keychain_consent_required" => Some(Self::KeychainConsentRequired),
            "credential_malformed" => Some(Self::CredentialMalformed),
            "interaction_required" => Some(Self::InteractionRequired),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CatalogProvider {
    id: &'static str,
    display_name: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct UnresolvedCatalogSource {
    provider: CatalogProvider,
    capability_id: String,
}

/// Secret-free projection of one discovery scan's diagnostics.
///
/// Fields are private and builder inputs are closed issue codes plus Rust-owned
/// provider identifiers/names. Discovery scope labels and arbitrary messages
/// cannot enter this value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogDiagnostics {
    projection_issues: BTreeSet<CatalogDiagnosticCode>,
    provider_issues: BTreeMap<CatalogProvider, BTreeSet<CatalogDiagnosticCode>>,
    unresolved: BTreeMap<UnresolvedCatalogSource, u32>,
}

impl CatalogDiagnostics {
    /// Add one whole-catalog discovery issue.
    pub fn push_projection_issue(&mut self, code: CatalogDiagnosticCode) {
        self.projection_issues.insert(code);
    }

    /// Add one issue for a provider id and label owned by Rust code.
    pub fn push_provider_issue(
        &mut self,
        provider_id: &'static str,
        display_name: &'static str,
        code: CatalogDiagnosticCode,
    ) {
        self.provider_issues
            .entry(CatalogProvider {
                id: provider_id,
                display_name,
            })
            .or_default()
            .insert(code);
    }

    /// Add a configured source that could not resolve to canonical identity.
    /// `capability_id` is the discovery-owned opaque candidate identifier.
    pub fn push_unresolved(
        &mut self,
        provider_id: &'static str,
        display_name: &'static str,
        capability_id: String,
        configuration_count: u32,
    ) {
        let source = UnresolvedCatalogSource {
            provider: CatalogProvider {
                id: provider_id,
                display_name,
            },
            capability_id,
        };
        self.unresolved
            .entry(source)
            .and_modify(|count| *count = (*count).max(configuration_count))
            .or_insert(configuration_count);
    }
}

pub(crate) fn apply_catalog_diagnostics(
    projection: &mut UsageProjectionV1,
    diagnostics: &CatalogDiagnostics,
) {
    projection
        .issues
        .retain(|issue| CatalogDiagnosticCode::from_id(&issue.code).is_none());
    for provider in &mut projection.providers {
        provider
            .issues
            .retain(|issue| CatalogDiagnosticCode::from_id(&issue.code).is_none());
    }
    projection.unresolved.clear();
    projection
        .providers
        .retain(|provider| !provider.accounts.is_empty() || !provider.issues.is_empty());

    projection.issues.extend(
        diagnostics
            .projection_issues
            .iter()
            .copied()
            .map(|code| issue(code, UsageIssueScopeV1::Projection)),
    );

    let mut current_providers = BTreeSet::new();
    current_providers.extend(diagnostics.provider_issues.keys().copied());
    current_providers.extend(diagnostics.unresolved.keys().map(|source| source.provider));

    for provider in current_providers {
        let issues = diagnostics
            .provider_issues
            .get(&provider)
            .into_iter()
            .flat_map(|codes| codes.iter().copied())
            .map(|code| issue(code, UsageIssueScopeV1::Provider))
            .collect::<Vec<_>>();
        if let Some(row) = projection
            .providers
            .iter_mut()
            .find(|row| row.provider_id == provider.id)
        {
            row.issues.extend(issues.clone());
        } else {
            projection.providers.push(UsageProviderV1 {
                provider_id: provider.id.to_owned(),
                display_name: provider.display_name.to_owned(),
                rank: 0,
                membership_state: UsageMembershipStateV1::Current,
                freshness: UsageFreshnessV1 {
                    generation: 0,
                    phase: UsageFreshnessPhaseV1::Failed,
                    last_good_at_epoch: None,
                    retry_at_epoch: None,
                    is_stale: false,
                },
                accounts: Vec::new(),
                issues,
            });
        }
    }

    projection.providers.sort_by(|left, right| {
        left.provider_id
            .cmp(&right.provider_id)
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    for (rank, provider) in projection.providers.iter_mut().enumerate() {
        provider.rank = u32::try_from(rank).unwrap_or(u32::MAX);
    }

    projection.unresolved = diagnostics
        .unresolved
        .iter()
        .map(|(source, configuration_count)| UsageUnresolvedV1 {
            provider_id: source.provider.id.to_owned(),
            capability_id: source.capability_id.clone(),
            configuration_count: *configuration_count,
            state: unresolved_state(
                diagnostics
                    .provider_issues
                    .get(&source.provider)
                    .into_iter()
                    .flat_map(|codes| codes.iter().copied()),
            ),
            issues: diagnostics
                .provider_issues
                .get(&source.provider)
                .into_iter()
                .flat_map(|codes| codes.iter().copied())
                .map(|code| issue(code, UsageIssueScopeV1::Provider))
                .collect(),
        })
        .collect();
}

fn issue(code: CatalogDiagnosticCode, scope: UsageIssueScopeV1) -> UsageIssueV1 {
    UsageIssueV1 {
        code: code.id().to_owned(),
        scope,
        recoverability: code.recoverability(),
        message: code.message().to_owned(),
        retry_at_epoch: None,
    }
}

fn unresolved_state(codes: impl Iterator<Item = CatalogDiagnosticCode>) -> UsageLifecycleV1 {
    let codes = codes.collect::<BTreeSet<_>>();
    if codes.contains(&CatalogDiagnosticCode::ConfigVersionUnsupported) {
        UsageLifecycleV1::Unsupported
    } else if codes.iter().any(|code| code.needs_secret()) {
        UsageLifecycleV1::NeedsSecret
    } else {
        // Discovery failure is not a temporary provider outage. `Error` keeps
        // unresolved identity from being typed as provider unavailability.
        UsageLifecycleV1::Error
    }
}

#[cfg(test)]
mod tests {
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

        apply_catalog_diagnostics(&mut projection, &diagnostics);
        assert_eq!(projection.providers.len(), 1);
        assert_eq!(
            projection.providers[0].issues[0].code,
            "interaction_required"
        );

        apply_catalog_diagnostics(&mut projection, &CatalogDiagnostics::default());
        assert!(projection.providers.is_empty());
    }
}

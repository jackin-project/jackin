// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Per-source validation and diagnostics.

use crate::{
    DiscoveredCredentialSource, ProfileCredentialReader, ProfileValidation,
    UsageDiscoveryDiagnostic, UsageDiscoveryIssue, ValidatedCredentialSource, ValidatedSourceParts,
    opaque_credential_revision, profile_credential_revision, profile_identity,
};
use jackin_usage_host_credentials::{
    ProviderCredentialEnvResolver, ProviderCredentialIdentityOutcome,
};
use std::collections::BTreeSet;

use jackin_usage_host_presentation::HostSurfaceId;

pub(crate) fn validate_source(
    source: DiscoveredCredentialSource,
    env_resolver: &dyn ProviderCredentialEnvResolver,
    profile_reader: &dyn ProfileCredentialReader,
) -> ValidatedSourceParts {
    match source {
        DiscoveredCredentialSource::Profile {
            surface,
            agent,
            root,
            operator_home,
            account_label,
            source_id,
            capability_id,
            provenance,
        } => {
            let outcome = profile_identity(profile_reader, agent, &root, &operator_home);
            let credential_revision =
                profile_credential_revision(profile_reader, agent, &root, &operator_home);
            let source = match &outcome {
                ProfileValidation::Authenticated { material, .. }
                | ProfileValidation::Anonymous(material) => material.clone().map_or(
                    // A material-less local profile (Muse identity, omp/hermes
                    // attribution) is unpollable by design — never a forwarded
                    // trust-domain token, so never `Capability`.
                    ValidatedCredentialSource::Unpollable,
                    |material| ValidatedCredentialSource::Profile(*material),
                ),
                _ => ValidatedCredentialSource::Capability,
            };
            let outcome = match outcome {
                ProfileValidation::Authenticated {
                    provider_id,
                    account_label: auth_label,
                    material,
                } => ProfileValidation::Authenticated {
                    provider_id,
                    account_label: auth_label.or(account_label),
                    material,
                },
                ProfileValidation::Anonymous(_) => {
                    if let Some(label) = account_label {
                        ProfileValidation::Authenticated {
                            account_label: Some(label),
                            provider_id: None,
                            material: None,
                        }
                    } else {
                        outcome
                    }
                }
                other => other,
            };
            (
                surface,
                source_id,
                capability_id,
                credential_revision,
                provenance,
                source,
                outcome,
            )
        }
        DiscoveredCredentialSource::Env {
            surface,
            handle,
            key,
            dispatch_key,
            launch_keys,
            kind: _,
            account_label,
            source_id,
            capability_id,
            provenance,
        } => {
            let material = env_resolver.source_material(surface, &key, &handle);
            let outcome = match env_resolver.identify_provider_credential(surface, &handle) {
                ProviderCredentialIdentityOutcome::Authenticated {
                    provider_id,
                    account_label: auth_label,
                } => ProfileValidation::Authenticated {
                    provider_id,
                    account_label: auth_label.or(account_label),
                    material: None,
                },
                ProviderCredentialIdentityOutcome::Anonymous => {
                    if let Some(label) = account_label {
                        ProfileValidation::Authenticated {
                            account_label: Some(label),
                            provider_id: None,
                            material: None,
                        }
                    } else {
                        ProfileValidation::Anonymous(None)
                    }
                }
                ProviderCredentialIdentityOutcome::Missing => ProfileValidation::Missing,
                ProviderCredentialIdentityOutcome::Denied => ProfileValidation::Denied,
                ProviderCredentialIdentityOutcome::Malformed => ProfileValidation::Malformed,
            };
            let credential_revision = opaque_credential_revision(&format!(
                "env:{}:{}:{}:{}",
                surface.id(),
                key,
                dispatch_key,
                handle.0
            ));
            (
                surface,
                source_id,
                capability_id,
                credential_revision,
                provenance,
                ValidatedCredentialSource::Env {
                    handle,
                    key,
                    dispatch_key,
                    launch_keys,
                    material,
                },
                outcome,
            )
        }
        DiscoveredCredentialSource::Capability {
            surface,
            account_label,
            source_id,
            capability_id,
        } => {
            let provenance = BTreeSet::from(["forwarded to Capsule".to_owned()]);
            let outcome = account_label.map_or(ProfileValidation::Anonymous(None), |label| {
                ProfileValidation::Authenticated {
                    provider_id: None,
                    account_label: Some(label),
                    material: None,
                }
            });
            (
                surface,
                source_id,
                capability_id.clone(),
                opaque_credential_revision(&format!("capability:{capability_id}")),
                provenance,
                ValidatedCredentialSource::Capability,
                outcome,
            )
        }
    }
}

pub(crate) fn source_diagnostic(
    surface: HostSurfaceId,
    provenance: &BTreeSet<String>,
    issue: UsageDiscoveryIssue,
) -> UsageDiscoveryDiagnostic {
    UsageDiscoveryDiagnostic {
        surface_id: Some(surface.id().to_owned()),
        scope_label: provenance.iter().cloned().collect::<Vec<_>>().join(", "),
        issue,
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Validated source accumulation.

use super::{
    AccountAccumulator, ProfileValidation, UsageDiscoveryDiagnostic, UsageDiscoveryIssue,
    ValidatedCredentialBinding, ValidatedCredentialSource, source_diagnostic,
};
use std::collections::{BTreeMap, BTreeSet};

use super::super::CanonicalAccountIdentity;
use super::super::CanonicalAccountSubject;
use super::super::HostSurfaceId;

pub(crate) fn accumulate_validated_source(
    parts: ValidatedSourceParts,
    attach_to: Option<CanonicalAccountIdentity>,
    diagnostics: &mut Vec<UsageDiscoveryDiagnostic>,
    bindings: &mut Vec<ValidatedCredentialBinding>,
    accounts: &mut BTreeMap<CanonicalAccountIdentity, AccountAccumulator>,
) {
    let (surface, source_id, capability_id, credential_revision, provenance, source, outcome) =
        parts;
    if let Some(identity) = attach_to {
        let label = match &outcome {
            ProfileValidation::Authenticated {
                provider_id,
                account_label,
                ..
            } => account_label
                .as_deref()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_owned)
                .or_else(|| provider_id.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        let entry = accounts.entry(identity.clone()).or_insert_with(|| {
            // Unreachable: the strong target accumulates first and always
            // mints its account. The fallback keeps the merge total.
            AccountAccumulator {
                label,
                provenance: BTreeSet::new(),
                source_ids: BTreeSet::new(),
            }
        });
        entry.provenance.extend(provenance.iter().cloned());
        entry.source_ids.insert(source_id.clone());
        bindings.push(ValidatedCredentialBinding {
            surface,
            identity: Some(identity),
            source_id,
            capability_id,
            credential_revision,
            provenance,
            source,
        });
        return;
    }

    match outcome {
        ProfileValidation::Authenticated {
            provider_id,
            account_label,
            material: _,
        } => {
            let subject = provider_id
                .as_ref()
                .filter(|id| !id.trim().is_empty())
                .map(|id| CanonicalAccountSubject::ProviderId(id.trim().to_owned()))
                .or_else(|| {
                    account_label
                        .as_ref()
                        .filter(|label| !label.trim().is_empty())
                        .map(|_| {
                            // A label is presentation evidence only. Keep
                            // source identity when the provider did not
                            // return a stronger canonical subject.
                            CanonicalAccountSubject::SourceCapability(capability_id.clone())
                        })
                });
            let Some(subject) = subject else {
                bindings.push(ValidatedCredentialBinding {
                    surface,
                    identity: None,
                    source_id,
                    capability_id,
                    credential_revision,
                    provenance,
                    source,
                });
                return;
            };
            let identity = CanonicalAccountIdentity { surface, subject };
            let label = account_label
                .as_deref()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_owned)
                .or_else(|| provider_id.clone())
                .unwrap_or_default();
            let entry = accounts
                .entry(identity.clone())
                .or_insert_with(|| AccountAccumulator {
                    label,
                    provenance: BTreeSet::new(),
                    source_ids: BTreeSet::new(),
                });
            entry.provenance.extend(provenance.iter().cloned());
            entry.source_ids.insert(source_id.clone());
            bindings.push(ValidatedCredentialBinding {
                surface,
                identity: Some(identity),
                source_id,
                capability_id,
                credential_revision,
                provenance,
                source,
            });
        }
        ProfileValidation::Anonymous(_) => bindings.push(ValidatedCredentialBinding {
            surface,
            identity: None,
            source_id,
            capability_id,
            credential_revision,
            provenance,
            source,
        }),
        ProfileValidation::Missing => diagnostics.push(source_diagnostic(
            surface,
            &provenance,
            UsageDiscoveryIssue::CredentialMissing,
        )),
        ProfileValidation::Denied => diagnostics.push(source_diagnostic(
            surface,
            &provenance,
            UsageDiscoveryIssue::CredentialDenied,
        )),
        ProfileValidation::ConsentRequired => diagnostics.push(source_diagnostic(
            surface,
            &provenance,
            UsageDiscoveryIssue::KeychainConsentRequired,
        )),
        ProfileValidation::Malformed => diagnostics.push(source_diagnostic(
            surface,
            &provenance,
            UsageDiscoveryIssue::CredentialMalformed,
        )),
    }
}

pub(crate) type ValidatedSourceParts = (
    HostSurfaceId,
    String,
    String,
    String,
    BTreeSet<String>,
    ValidatedCredentialSource,
    ProfileValidation,
);

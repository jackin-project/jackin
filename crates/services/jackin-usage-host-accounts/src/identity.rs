// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::account_key_hash;
use jackin_protocol::control::{FocusedUsageView, UsageConfidence};

use super::{stable_account_label, surface_for_view};
use jackin_usage_host_presentation::HostSurfaceId;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CanonicalAccountSubject {
    ProviderId(String),
    ProviderStableHandle(String),
    SourceCapability(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanonicalAccountIdentity {
    pub surface: HostSurfaceId,
    pub subject: CanonicalAccountSubject,
}

#[derive(Debug, Default, Clone)]
pub struct CanonicalIdentityGraph {
    evidence_by_id: BTreeMap<String, CanonicalAccountIdentity>,
    aliases: BTreeMap<String, String>,
}

impl CanonicalIdentityGraph {
    pub fn resolve_alias(
        &mut self,
        capability_id: &str,
        identity: &CanonicalAccountIdentity,
    ) -> Result<String, String> {
        let canonical_id = identity.canonical_id_v1();
        if self
            .evidence_by_id
            .get(&canonical_id)
            .is_some_and(|existing| existing != identity)
        {
            return Err("canonical account identity collision".to_owned());
        }
        if self
            .aliases
            .get(capability_id)
            .is_some_and(|existing| existing != &canonical_id)
        {
            return Err("canonical account alias collision".to_owned());
        }

        self.evidence_by_id
            .insert(canonical_id.clone(), identity.clone());
        self.aliases
            .insert(capability_id.to_owned(), canonical_id.clone());
        Ok(canonical_id)
    }
}

impl CanonicalAccountIdentity {
    pub fn source_capability(surface: HostSurfaceId, capability_id: &str) -> Self {
        Self {
            surface,
            subject: CanonicalAccountSubject::SourceCapability(capability_id.to_owned()),
        }
    }

    pub fn from_view(surface: HostSurfaceId, view: &FocusedUsageView) -> Option<Self> {
        if surface_for_view(view) != Some(surface)
            || matches!(view.confidence, UsageConfidence::PresenceOnly)
        {
            return None;
        }
        let label = stable_account_label(&view.account.account_label)?;
        Some(Self {
            surface,
            subject: CanonicalAccountSubject::ProviderStableHandle(label.to_owned()),
        })
    }

    pub fn account_key(&self) -> String {
        let evidence = match &self.subject {
            CanonicalAccountSubject::ProviderId(id) => {
                format!("account-key-v1:provider-id:{}", id.trim())
            }
            CanonicalAccountSubject::ProviderStableHandle(handle) => format!(
                "account-key-v1:stable-handle:{}",
                normalize_stable_handle(handle)
            ),
            CanonicalAccountSubject::SourceCapability(capability_id) => format!(
                "account-key-v1:source-capability:{}:{}",
                capability_id.len(),
                capability_id
            ),
        };
        account_key_hash(self.surface.provider_id(), &evidence)
    }

    pub fn canonical_id_v1(&self) -> String {
        let evidence = match &self.subject {
            CanonicalAccountSubject::ProviderId(id) => {
                format!("canonical-account-v1:provider-id:{}", id.trim())
            }
            CanonicalAccountSubject::ProviderStableHandle(handle) => format!(
                "canonical-account-v1:stable-handle:{}",
                normalize_stable_handle(handle)
            ),
            CanonicalAccountSubject::SourceCapability(capability_id) => format!(
                "canonical-account-v1:source-capability:{}:{}",
                capability_id.len(),
                capability_id
            ),
        };
        account_key_hash(self.surface.provider_id(), &evidence)
    }
}

pub(crate) fn normalize_stable_handle(handle: &str) -> String {
    handle.trim().to_lowercase()
}

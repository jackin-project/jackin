// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_protocol::control::{CanonicalRemainingPercent, PercentObservationOrigin};

pub(super) fn quota_view() -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("claude", chrono::Utc::now().timestamp());
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.account.provider_label = "Claude".to_owned();
    view.account.account_label = "account@example.test".to_owned();
    view.buckets = vec![QuotaBucketView {
        label: "Weekly".to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: Some(
            CanonicalRemainingPercent::new(75.0).expect("fixture remaining percentage is valid"),
        ),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        count_quota: None,
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];
    view
}

pub(super) fn env_material(source_name: &str, material: &str) -> ProviderCredentialSourceMaterial {
    ProviderCredentialSourceMaterial {
        source: UsageCredentialSourceIdentity::HostEnv {
            name: source_name.to_owned(),
        },
        material_fingerprint: usage_credential_material_fingerprint(material),
    }
}

pub(super) fn env_scope(
    account_id: &str,
    surface_id: &str,
    key: &str,
    material: &ProviderCredentialSourceMaterial,
) -> UsageCredentialScope {
    UsageCredentialScope {
        sources: BTreeSet::from([UsageCredentialSourceProof {
            account_id: account_id.to_owned(),
            surface_id: surface_id.to_owned(),
            key: key.to_owned(),
            source: material.source.clone(),
            material_fingerprint: material.material_fingerprint.clone(),
        }]),
    }
}

#[derive(Default)]
pub(super) struct RecordingRefreshResolver {
    pub(super) calls: Mutex<Vec<(String, OpaqueCredentialHandle)>>,
}

impl ProviderCredentialEnvResolver for RecordingRefreshResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        _keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        Vec::new()
    }

    fn refresh_provider_credential(
        &self,
        _surface: HostSurfaceId,
        key: &str,
        handle: &OpaqueCredentialHandle,
    ) -> ProviderCredentialRefreshOutcome {
        self.calls
            .lock()
            .unwrap()
            .push((key.to_owned(), handle.clone()));
        ProviderCredentialRefreshOutcome::Snapshot {
            view: Box::new(quota_view()),
            rate_limit: None,
            provider_error: None,
        }
    }
}

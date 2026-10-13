// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use std::collections::BTreeSet;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageFreshnessPhaseV1, UsageGenerationView, UsageIssueRecoverabilityV1,
    UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1, UsageProjectionRefreshStateV1,
};
use jackin_usage_discovery::{
    DiscoveredAccountDescriptor, UsageDiscoveryDiagnostic, UsageDiscoveryIssue,
    ValidatedCredentialBinding, ValidatedCredentialSource, ValidatedUsageDiscovery,
};

fn bucket(label: &str) -> QuotaBucketView {
    QuotaBucketView {
        label: label.into(),
        used_label: None,
        limit_label: None,
        remaining_percent: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

fn view_with_buckets(
    status: UsageSnapshotStatus,
    buckets: Vec<QuotaBucketView>,
) -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: None,
        focused_provider: None,
        account: FocusedAccountHeader {
            provider_label: "Codex".into(),
            account_label: "work@example.test".into(),
            username: None,
            plan_label: None,
            credential_origin: None,
        },
        buckets,
        status,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_800_000_000,
        updated_label: "now".into(),
        status_bar_label: "ok".into(),
        tabs: Vec::new(),
        last_error: None,
    }
}

fn production_projection_runtime() -> (tempfile::TempDir, HostUsageRuntime, UsageAccountCapability)
{
    let temp = tempfile::tempdir().unwrap();
    let identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Codex,
        subject: CanonicalAccountSubject::ProviderId("projection-account".to_owned()),
    };
    let binding = ValidatedCredentialBinding {
        surface: HostSurfaceId::Codex,
        identity: Some(identity.clone()),
        source_id: "projection-source".to_owned(),
        capability_id: "projection-capability".to_owned(),
        credential_revision: "revision".to_owned(),
        provenance: BTreeSet::from(["account work".to_owned()]),
        source: ValidatedCredentialSource::Capability,
    };
    let capability =
        jackin_usage_discovery::capability_for_binding(&binding, Some("projection-revision"));
    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("projection-revision".to_owned()),
        accounts: vec![DiscoveredAccountDescriptor {
            surface_id: "codex".to_owned(),
            account_key: identity.account_key(),
            account_label: "work@example.test".to_owned(),
            provenance: vec!["account work".to_owned()],
            source_ids: vec!["projection-source".to_owned()],
            identity,
        }],
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![binding],
    };
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open_with_validated_discovery(HostRuntimeConfig::under_data_dir(temp.path()), discovery)
        .unwrap();
    (temp, runtime, capability)
}

#[test]
fn canonical_runtime_preserves_broker_failure_retry_and_recovery() {
    use jackin_protocol::usage_broker::{
        UsageCoordinationError, UsageCoordinationErrorKind, UsageRefreshPhase,
    };
    let (_temp, mut runtime, capability) = production_projection_runtime();
    let snapshot = view_with_buckets(UsageSnapshotStatus::Fresh, vec![bucket("Weekly")]);
    runtime
        .apply_broker_generation(UsageGenerationView {
            capability: capability.clone(),
            generation: 41,
            phase: UsageRefreshPhase::Failed,
            snapshot: Some(snapshot.clone()),
            error: Some(UsageCoordinationError {
                kind: UsageCoordinationErrorKind::RateLimited,
                message: "Provider request quota exceeded".to_owned(),
            }),
            retry_at_epoch: Some(1_800_000_123),
        })
        .unwrap();
    let failed = runtime.canonical_projection("en").unwrap();
    let account = &failed.providers[0].accounts[0];
    assert_eq!(account.freshness.generation, 41);
    assert_eq!(account.freshness.phase, UsageFreshnessPhaseV1::Stale);
    assert_eq!(account.freshness.last_good_at_epoch, Some(1_800_000_000));
    assert_eq!(account.freshness.retry_at_epoch, Some(1_800_000_123));
    assert_eq!(
        failed.providers[0].freshness.retry_at_epoch,
        Some(1_800_000_123)
    );
    assert_eq!(
        account.issues,
        vec![UsageIssueV1 {
            code: "rate_limited".to_owned(),
            scope: UsageIssueScopeV1::Account,
            recoverability: UsageIssueRecoverabilityV1::Retryable,
            message: "Provider request quota exceeded".to_owned(),
            retry_at_epoch: Some(1_800_000_123),
        }]
    );
    runtime
        .apply_broker_generation(UsageGenerationView {
            capability,
            generation: 42,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(snapshot),
            error: None,
            retry_at_epoch: None,
        })
        .unwrap();
    let recovered = runtime.canonical_projection("en").unwrap();
    let account = &recovered.providers[0].accounts[0];
    assert_ne!(failed.projection_id, recovered.projection_id);
    assert_eq!(account.freshness.phase, UsageFreshnessPhaseV1::Current);
    assert_eq!(account.freshness.retry_at_epoch, None);
    assert!(account.issues.is_empty());
}
#[test]
fn canonical_runtime_preserves_action_required_failure_without_snapshot() {
    use jackin_protocol::usage_broker::{
        UsageCoordinationError, UsageCoordinationErrorKind, UsageRefreshPhase,
    };
    let (_temp, mut runtime, capability) = production_projection_runtime();
    runtime
        .apply_broker_generation(UsageGenerationView {
            capability,
            generation: 9,
            phase: UsageRefreshPhase::Failed,
            snapshot: None,
            error: Some(UsageCoordinationError {
                kind: UsageCoordinationErrorKind::NeedsSecret,
                message: "Approve credential access".to_owned(),
            }),
            retry_at_epoch: None,
        })
        .unwrap();
    let projection = runtime.canonical_projection("en").unwrap();
    let account = &projection.providers[0].accounts[0];
    assert_eq!(account.lifecycle, UsageLifecycleV1::NeedsSecret);
    assert_eq!(account.freshness.phase, UsageFreshnessPhaseV1::Failed);
    assert_eq!(account.freshness.last_good_at_epoch, None);
    assert_eq!(account.issues[0].code, "needs_secret");
    assert_eq!(account.issues[0].message, "Approve credential access");
    assert_eq!(
        account.issues[0].recoverability,
        UsageIssueRecoverabilityV1::ActionRequired
    );
}

#[test]
fn canonical_runtime_projects_discovery_diagnostics_without_account_rows() {
    let (_temp, mut runtime, _capability) = production_projection_runtime();
    let discovery = runtime.discovery.as_mut().unwrap();
    discovery.accounts.clear();
    discovery.bindings.clear();
    discovery.diagnostics = vec![
        UsageDiscoveryDiagnostic {
            surface_id: Some("codex".to_owned()),
            scope_label: "account work".to_owned(),
            issue: UsageDiscoveryIssue::KeychainConsentRequired,
        },
        UsageDiscoveryDiagnostic {
            surface_id: None,
            scope_label: "workspace work".to_owned(),
            issue: UsageDiscoveryIssue::ConfigInvalid,
        },
    ];
    let projection = runtime.canonical_projection("en").unwrap();
    assert_eq!(projection.providers.len(), 1);
    assert!(projection.providers[0].accounts.is_empty());
    assert_eq!(
        projection.providers[0].issues[0].code,
        "keychain_consent_required"
    );
    assert_eq!(
        projection.providers[0].issues[0].scope,
        UsageIssueScopeV1::Provider
    );
    assert_eq!(
        projection.providers[0].issues[0].recoverability,
        UsageIssueRecoverabilityV1::ActionRequired
    );
    assert_eq!(projection.issues[0].code, "config_invalid");
    assert_eq!(projection.issues[0].scope, UsageIssueScopeV1::Projection);
}
#[test]
fn canonical_runtime_projects_coordination_failure_and_retains_last_good() {
    use jackin_protocol::usage_broker::{
        UsageCoordinationError, UsageCoordinationErrorKind, UsageRefreshPhase,
    };
    let (_temp, mut runtime, capability) = production_projection_runtime();
    let snapshot = view_with_buckets(UsageSnapshotStatus::Fresh, vec![bucket("Weekly")]);
    runtime
        .apply_broker_generation(UsageGenerationView {
            capability: capability.clone(),
            generation: 41,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(snapshot.clone()),
            error: None,
            retry_at_epoch: None,
        })
        .unwrap();
    let before = runtime.canonical_projection("en").unwrap();
    runtime
        .record_broker_error(
            &capability,
            &UsageCoordinationError {
                kind: UsageCoordinationErrorKind::Unavailable,
                message: "Usage broker connection failed".to_owned(),
            },
        )
        .unwrap();
    let failed = runtime.canonical_projection("en").unwrap();
    let account = &failed.providers[0].accounts[0];
    assert_ne!(before.projection_id, failed.projection_id);
    assert_eq!(account.freshness.generation, 41);
    assert_eq!(account.freshness.phase, UsageFreshnessPhaseV1::Stale);
    assert_eq!(account.freshness.last_good_at_epoch, Some(1_800_000_000));
    assert_eq!(account.freshness.retry_at_epoch, None);
    assert_eq!(account.windows[0].label, "Weekly");
    assert_eq!(
        account.issues,
        vec![UsageIssueV1 {
            code: "unavailable".to_owned(),
            scope: UsageIssueScopeV1::Account,
            recoverability: UsageIssueRecoverabilityV1::Retryable,
            message: "Usage broker connection failed".to_owned(),
            retry_at_epoch: None,
        }]
    );
    runtime
        .apply_broker_generation(UsageGenerationView {
            capability: capability.clone(),
            generation: 43,
            phase: UsageRefreshPhase::Updating,
            snapshot: Some(snapshot.clone()),
            error: None,
            retry_at_epoch: Some(1_800_000_120),
        })
        .unwrap();
    runtime
        .record_broker_error(
            &capability,
            &UsageCoordinationError {
                kind: UsageCoordinationErrorKind::Unavailable,
                message: "Usage broker connection failed during refresh".to_owned(),
            },
        )
        .unwrap();
    let active_failure = runtime.canonical_projection("en").unwrap();
    let account = &active_failure.providers[0].accounts[0];
    assert_eq!(
        active_failure.refresh_state,
        UsageProjectionRefreshStateV1::Refreshing
    );
    assert_eq!(account.freshness.generation, 43);
    assert_eq!(account.freshness.phase, UsageFreshnessPhaseV1::Refreshing);
    assert!(account.freshness.is_stale);
    assert_eq!(account.freshness.last_good_at_epoch, Some(1_800_000_000));
    assert_eq!(account.freshness.retry_at_epoch, Some(1_800_000_120));
    assert_eq!(account.windows[0].label, "Weekly");
    assert_eq!(account.issues[0].code, "unavailable");
    assert_eq!(
        account.issues[0].message,
        "Usage broker connection failed during refresh"
    );
    assert_eq!(account.issues[0].retry_at_epoch, Some(1_800_000_120));
    assert_eq!(
        active_failure.providers[0].freshness.retry_at_epoch,
        Some(1_800_000_120)
    );
    runtime
        .apply_broker_generation(UsageGenerationView {
            capability,
            generation: 44,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(snapshot),
            error: None,
            retry_at_epoch: None,
        })
        .unwrap();
    let recovered = runtime.canonical_projection("en").unwrap();
    assert_eq!(
        recovered.providers[0].accounts[0].freshness.phase,
        UsageFreshnessPhaseV1::Current
    );
    assert!(recovered.providers[0].accounts[0].issues.is_empty());
}

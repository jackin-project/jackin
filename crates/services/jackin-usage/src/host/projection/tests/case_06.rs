// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn account_projection_preserves_error_text_without_inventing_retry_policy() {
    let mut view = view_with_buckets(UsageSnapshotStatus::Stale, vec![bucket("Weekly")]);
    view.last_error = Some("Provider message mentions HTTP 429 retry at 999999".to_owned());
    let account = project_account(&catalog_entry(view, None), 0, 1).unwrap();
    assert_eq!(account.issues[0].code, "provider_unavailable");
    assert_eq!(
        account.issues[0].message,
        "Provider message mentions HTTP 429 retry at 999999"
    );
    assert_eq!(account.issues[0].retry_at_epoch, None);
    assert_eq!(account.freshness.retry_at_epoch, None);
}

#[test]
fn canonical_runtime_projects_discovery_diagnostics_without_account_rows() {
    use crate::host::discovery::{UsageDiscoveryDiagnostic, UsageDiscoveryIssue};
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

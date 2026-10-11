// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_protocol::usage_broker::{UsageIdentityKindV1, UsagePercent, UsageProjectionV1};
use jackin_protocol::usage_monitor::{
    MonitorStatus, StatuslineObservation, StatuslineQuotaWindow, StatuslineRateLimits,
    USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
};

fn local_source_projection(
    capability: &UsageAccountCapability,
    remaining_percent: i32,
    now_epoch: i64,
) -> UsageProjectionV1 {
    let mut projection: UsageProjectionV1 = serde_json::from_str(include_str!(
        "../../../../../jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
    ))
    .expect("valid shared usage projection fixture");
    projection.generated_at_epoch = now_epoch;
    let provider = &mut projection.providers[0];
    provider.provider_id = "claude".to_owned();
    provider.display_name = "Claude".to_owned();
    provider.freshness.last_good_at_epoch = Some(now_epoch);
    let account = &mut provider.accounts[0];
    account.canonical_account_id = capability.account_id.clone();
    account.identity_kind = UsageIdentityKindV1::LocalSourceHandle;
    account.freshness.last_good_at_epoch = Some(now_epoch);
    for window in &mut account.windows {
        window.remaining_raw_percent = Some(remaining_percent);
        window.remaining_percent = Some(UsagePercent::clamp_raw(remaining_percent));
        window.reset_at_epoch = Some(now_epoch + 24 * 60 * 60);
    }
    projection
}

fn bind_projection_observer(
    store: &MonitorStore,
    account_id: &str,
    source_id: &str,
    collector_approved: bool,
    idempotency_key: &str,
) -> (MonitorAccountBinding, String) {
    let binding = match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: account_id.to_owned(),
                    operator_label: "projection-routing-test".to_owned(),
                    operator_confirmed: true,
                    provider_account_id: Some(source_id.to_owned()),
                    experimental_collector_approved: collector_approved,
                },
            },
            NOW,
        )
        .expect("bind projection observer account")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account-bound reply, got {other:?}"),
    };
    let status = match store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    purpose: MonitorPurpose::ObserveOnly,
                    scope: MonitorScope::BoundAccount {
                        binding_id: binding.binding_id.clone(),
                        binding_revision: binding.revision,
                        session_id: None,
                    },
                    goal_id: None,
                    expected_model: None,
                    policy_revision: None,
                    experimental_collector: false,
                },
                idempotency_key: idempotency_key.to_owned(),
            },
            NOW,
        )
        .expect("start projection observer")
    {
        MonitorReply::Started { status } => status,
        other => panic!("expected started reply, got {other:?}"),
    };
    (binding, status.monitor_id)
}

fn monitor_status(store: &MonitorStore, monitor_id: &str, now_epoch: i64) -> MonitorStatus {
    match store
        .operate(
            MonitorOperation::Status {
                monitor_id: monitor_id.to_owned(),
            },
            now_epoch,
        )
        .expect("read monitor status")
    {
        MonitorReply::Status { status } => *status,
        other => panic!("expected monitor status reply, got {other:?}"),
    }
}

#[test]
fn local_source_projection_routes_only_to_current_approved_local_partitions() {
    let temp = tempfile::tempdir().expect("temporary monitor directory");
    let store = MonitorStore::open(temp.path()).expect("open monitor store");
    let source_a = source_id();
    let source_b = "b".repeat(64);
    store.set_experimental_collector_source(Some(source_a.clone()));
    let capability_a = crate::source_identity::claude_usage_capability_for_source_id(&source_a);
    let capability_b = crate::source_identity::claude_usage_capability_for_source_id(&source_b);

    let (_binding_a, monitor_a) =
        bind_projection_observer(&store, "local-alpha", &source_a, true, "route-alpha");
    let (binding_b, monitor_b) =
        bind_projection_observer(&store, "local-beta", &source_a, true, "route-beta");
    let (_binding_c, monitor_c) =
        bind_projection_observer(&store, "local-gamma", &source_b, true, "route-gamma");
    let (_binding_d, monitor_d) = bind_projection_observer(
        &store,
        "local-unapproved",
        &source_b,
        false,
        "route-unapproved",
    );

    let projection_a = local_source_projection(&capability_a, 57, NOW);
    store
        .observe_projection(&projection_a, NOW)
        .expect("observe current projection for source A");
    assert_eq!(
        monitor_status(&store, &monitor_a, NOW)
            .seven_day
            .used_percentage_basis_points,
        Some(4_300)
    );
    assert_eq!(
        monitor_status(&store, &monitor_b, NOW)
            .seven_day
            .used_percentage_basis_points,
        Some(4_300),
        "one source fans out only to its explicitly approved local partitions"
    );
    assert_eq!(
        monitor_status(&store, &monitor_c, NOW)
            .seven_day
            .used_percentage_basis_points,
        None,
        "another source cannot consume source A evidence"
    );
    assert_eq!(
        monitor_status(&store, &monitor_d, NOW)
            .seven_day
            .used_percentage_basis_points,
        None,
        "a binding without collector consent cannot consume source evidence"
    );

    let mut unverified_projection = local_source_projection(&capability_a, 1, NOW);
    let unverified_account = &mut unverified_projection.providers[0].accounts[0];
    unverified_account.canonical_account_id = "local-alpha".to_owned();
    unverified_account.identity_kind = UsageIdentityKindV1::UnverifiedHandle;
    store
        .observe_projection(&unverified_projection, NOW)
        .expect("observe unverified projection without routing it to a local partition");
    assert_eq!(
        monitor_status(&store, &monitor_a, NOW)
            .seven_day
            .used_percentage_basis_points,
        Some(4_300),
        "an unverified canonical ID cannot stand in for a local source capability"
    );

    store
        .operate(
            MonitorOperation::Ingest {
                scope: MonitorScope::BoundAccount {
                    binding_id: binding_b.binding_id.clone(),
                    binding_revision: binding_b.revision,
                    session_id: None,
                },
                observation: StatuslineObservation {
                    schema_version: USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
                    session_id: "local-beta-session".to_owned(),
                    model: None,
                    claude_code_version: Some("2.1.80".to_owned()),
                    rate_limits: StatuslineRateLimits {
                        five_hour: None,
                        seven_day: Some(StatuslineQuotaWindow {
                            used_percentage_basis_points: Some(1_700),
                            reset_at_epoch: Some(NOW + 24 * 60 * 60),
                        }),
                    },
                },
            },
            NOW,
        )
        .expect("record independent statusline evidence for local beta");

    let rebound = match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: "local-beta".to_owned(),
                    operator_label: "projection-routing-test".to_owned(),
                    operator_confirmed: true,
                    provider_account_id: Some(source_b.clone()),
                    experimental_collector_approved: true,
                },
            },
            NOW + 1,
        )
        .expect("remap local beta to source B")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected rebound account, got {other:?}"),
    };
    assert_eq!(rebound.revision, binding_b.revision + 1);
    let after_rebind = monitor_status(&store, &monitor_b, NOW + 1);
    assert_eq!(
        after_rebind.seven_day.used_percentage_basis_points,
        Some(1_700),
        "remapping drops old broker-source quota but keeps local statusline evidence"
    );
    assert!(
        after_rebind
            .evidence
            .iter()
            .any(|evidence| evidence.source == MonitorEvidenceSource::Statusline)
    );
    assert!(
        !after_rebind
            .evidence
            .iter()
            .any(|evidence| evidence.source == MonitorEvidenceSource::BrokerProjection)
    );

    store
        .observe_projection(&projection_a, NOW + 1)
        .expect("late source-A projection cannot follow beta's newer binding revision");
    assert_eq!(
        monitor_status(&store, &monitor_b, NOW + 1)
            .seven_day
            .used_percentage_basis_points,
        Some(1_700)
    );

    store.set_experimental_collector_source(Some(source_b));
    let stale_source_a_projection = local_source_projection(&capability_a, 1, NOW + 2);
    store
        .observe_projection(&stale_source_a_projection, NOW + 2)
        .expect("source-A identity cannot match source B's foreground lease");
    let projection_b = local_source_projection(&capability_b, 20, NOW + 3);
    store
        .observe_projection(&projection_b, NOW + 3)
        .expect("observe current projection for source B");

    assert_eq!(
        monitor_status(&store, &monitor_a, NOW + 3)
            .seven_day
            .used_percentage_basis_points,
        Some(4_300),
        "source B cannot overwrite the unchanged source-A partition"
    );
    assert_eq!(
        monitor_status(&store, &monitor_b, NOW + 3)
            .seven_day
            .used_percentage_basis_points,
        Some(8_000)
    );
    assert_eq!(
        monitor_status(&store, &monitor_c, NOW + 3)
            .seven_day
            .used_percentage_basis_points,
        Some(8_000)
    );
    assert_eq!(
        monitor_status(&store, &monitor_d, NOW + 3)
            .seven_day
            .used_percentage_basis_points,
        None
    );

    let revoked = match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: "local-beta".to_owned(),
                    operator_label: "projection-routing-test".to_owned(),
                    operator_confirmed: true,
                    provider_account_id: Some("b".repeat(64)),
                    experimental_collector_approved: false,
                },
            },
            NOW + 4,
        )
        .expect("revoke local beta's collector consent")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected consent revocation, got {other:?}"),
    };
    assert_eq!(revoked.revision, rebound.revision + 1);
    let after_revoke = monitor_status(&store, &monitor_b, NOW + 4);
    assert_eq!(
        after_revoke.seven_day.used_percentage_basis_points,
        Some(1_700),
        "revoking consent clears source-derived quota but preserves independent statusline evidence"
    );
    assert!(
        after_revoke
            .evidence
            .iter()
            .all(|evidence| evidence.source == MonitorEvidenceSource::Statusline)
    );

    store
        .observe_projection(&projection_b, NOW + 5)
        .expect("current source projection cannot route after consent revocation");
    assert_eq!(
        monitor_status(&store, &monitor_b, NOW + 5)
            .seven_day
            .used_percentage_basis_points,
        Some(1_700)
    );
    assert_eq!(
        monitor_status(&store, &monitor_c, NOW + 5)
            .seven_day
            .used_percentage_basis_points,
        Some(8_000),
        "revoking one local partition does not remove consent from another partition"
    );
}

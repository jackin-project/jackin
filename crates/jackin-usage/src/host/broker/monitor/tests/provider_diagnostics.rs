// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn rate_limited_provider_preserves_retry_and_fresh_statusline_quota() {
    let (_directory, store) = open_store();
    let (binding, monitor_id) = start_approved_projection_observer(
        &store,
        "operator-local-rate-limited-diagnostics",
        "acct-provider-freshness",
        NOW,
    );
    store
        .observe_projection(&provider_projection(NOW, 1, 1), NOW)
        .expect("publish initial provider success");

    let retry_at_epoch = NOW + 900;
    let mut failed = provider_projection(NOW + 10, 2, 2);
    let account = &mut failed.providers[0].accounts[0];
    account.freshness = UsageFreshnessV1 {
        generation: 2,
        phase: UsageFreshnessPhaseV1::Failed,
        last_good_at_epoch: Some(NOW),
        retry_at_epoch: Some(retry_at_epoch),
        is_stale: true,
    };
    account.issues = vec![UsageIssueV1 {
        code: "rate_limited".to_owned(),
        scope: UsageIssueScopeV1::Account,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: "rate limited".to_owned(),
        retry_at_epoch: Some(retry_at_epoch),
    }];
    for group in &mut account.metric_groups {
        group.phase = UsageFreshnessPhaseV1::Failed;
        group.is_stale = true;
        group.observed_at_epoch = None;
        group.fetched_at_epoch = NOW + 10;
        group.last_success_at_epoch = Some(NOW);
    }
    store
        .observe_projection(&failed, NOW + 10)
        .expect("publish rate-limited last-good state");

    let expired_at = NOW + MONITOR_EVIDENCE_TTL_SECS + 1;
    let expired = descriptor_status(&store, &monitor_id, expired_at);
    assert_eq!(
        expired.readiness.provider,
        MonitorProviderReadiness::RateLimited
    );
    assert_eq!(expired.readiness.quota, MonitorQuotaReadiness::Stale);
    assert_eq!(expired.five_hour.used_percentage_basis_points, Some(2_000));
    assert_eq!(expired.seven_day.used_percentage_basis_points, Some(3_000));
    for window in [&expired.five_hour, &expired.seven_day] {
        let used = window.used_evidence.as_ref().expect("retained quota value");
        assert_eq!(used.evidence_at_epoch, Some(NOW));
        assert_eq!(used.evidence_received_at_epoch, NOW);
        assert_eq!(used.age_seconds, 301);
        assert_eq!(used.freshness, MonitorEvidenceFreshness::Stale);
    }
    let rate_issue = expired
        .issues
        .iter()
        .find(|issue| issue.code == MonitorIssueCode::ProviderRateLimited)
        .expect("provider rate-limit issue is visible");
    assert_eq!(rate_issue.retry_at_epoch, Some(retry_at_epoch));

    store
        .operate(
            MonitorOperation::Ingest {
                scope: MonitorScope::BoundAccount {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    session_id: None,
                },
                observation: observation_with_windows(
                    "fresh-statusline-after-provider-failure",
                    Some(42),
                    Some(NOW + 3_600),
                    Some(34),
                    Some(NOW + 86_400),
                ),
            },
            expired_at + 1,
        )
        .expect("ingest independent fresh statusline quota");
    let recovered = descriptor_status(&store, &monitor_id, expired_at + 1);
    assert_eq!(
        recovered.readiness.provider,
        MonitorProviderReadiness::RateLimited
    );
    assert_eq!(recovered.readiness.quota, MonitorQuotaReadiness::Ready);
    assert!(recovered.issues.iter().any(|issue| {
        issue.code == MonitorIssueCode::ProviderRateLimited
            && issue.retry_at_epoch == Some(retry_at_epoch)
    }));
    assert!(
        !recovered
            .issues
            .iter()
            .any(|issue| issue.code == MonitorIssueCode::QuotaStale)
    );
}

#[expect(
    clippy::too_many_lines,
    reason = "The fixture builds a complete V4 policy, spend, quota, and event history for migration verification."
)]
fn v4_history_fixture() -> (tempfile::TempDir, serde_json::Value, String, String, String) {
    let (directory, store) = open_store();
    let account_id = "acct-v4-provider-history";
    let goal_id = "goal-v4-provider-history";
    seed_zero_sgd_spend(&store, account_id, NOW);
    let (binding, _) = start_approved_projection_observer(&store, account_id, account_id, NOW);
    let policy = approve_strict_policy(
        &store,
        &binding,
        goal_id,
        &Money::new(5_000, "SGD", 2),
        NOW + 1,
    );
    let monitor_id = match store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    purpose: MonitorPurpose::DispatchGuard,
                    scope: binding_scope(&binding),
                    goal_id: Some(goal_id.to_owned()),
                    expected_model: None,
                    policy_revision: Some(policy.revision),
                    experimental_collector: false,
                },
                idempotency_key: "v4-strict-history-monitor".to_owned(),
            },
            NOW + 2,
        )
        .expect("start strict history monitor")
    {
        MonitorReply::Started { status } => status.monitor_id.clone(),
        other => panic!("expected started reply, got {other:?}"),
    };
    let statusline_session_reset = NOW + 1_800;
    let statusline_weekly_reset = NOW + 80_000;
    store
        .operate(
            MonitorOperation::Ingest {
                scope: binding_scope(&binding),
                observation: observation_with_windows(
                    "v4-statusline-reset-watermark",
                    None,
                    Some(statusline_session_reset),
                    None,
                    Some(statusline_weekly_reset),
                ),
            },
            NOW + 3,
        )
        .expect("retain statusline reset watermark alongside provider data");
    record_spend(
        &store,
        spend_input(account_id, NOW - 100, NOW + 100_000, 123, NOW + 4, "SGD"),
        NOW + 4,
    );
    let mut projection = provider_projection(NOW + 5, 1, 1);
    projection.providers[0].accounts[0].canonical_account_id = account_id.to_owned();
    for window in &mut projection.providers[0].accounts[0].windows {
        window.used_percent = Some(UsagePercent::clamp_raw(100));
        window.used_raw_percent = Some(100);
    }
    for group in &mut projection.providers[0].accounts[0].metric_groups {
        if let UsageMetricValueV1::Window {
            used_percent,
            used_raw_percent,
            ..
        } = &mut group.value
        {
            *used_percent = Some(UsagePercent::clamp_raw(100));
            *used_raw_percent = Some(100);
        }
    }
    store
        .observe_projection(&projection, NOW + 5)
        .expect("publish provider quota under the legacy unscoped account key");

    let before_migration = store.lock();
    assert!(
        before_migration.accounts[account_id]
            .reset_barriers
            .iter()
            .flatten()
            .any(|barrier| barrier.source == MonitorEvidenceSource::BrokerProjection)
    );
    assert!(
        before_migration.monitors[&monitor_id]
            .evidence
            .iter()
            .any(|evidence| {
                evidence.source == MonitorEvidenceSource::BrokerProjection
                    && matches!(
                        evidence.value,
                        MonitorEvidenceValue::QuotaUsedPercentage {
                            used_percentage_basis_points: 10_000,
                            ..
                        }
                    )
            })
    );
    drop(before_migration);

    let mut snapshot = serde_json::to_value(store.lock().clone()).expect("serialize V4 fixture");
    snapshot["schema_version"] = serde_json::json!(4);
    for account in snapshot["accounts"]
        .as_object_mut()
        .expect("persisted accounts")
        .values_mut()
    {
        account
            .as_object_mut()
            .expect("persisted account")
            .remove("provider_observation");
    }
    for monitor in snapshot["monitors"]
        .as_object_mut()
        .expect("persisted monitors")
        .values_mut()
    {
        let fingerprints = monitor["evidence_fingerprints"]
            .as_object_mut()
            .expect("persisted evidence fingerprints");
        let current_fingerprints = std::mem::take(fingerprints);
        for (key, value) in current_fingerprints {
            let parts = key.split(':').collect::<Vec<_>>();
            let legacy_key = match parts.as_slice() {
                [source, field @ ("used" | "reset"), window, "account"]
                    if matches!(
                        *source,
                        "broker_projection"
                            | "statusline"
                            | "provider_spend"
                            | "operator"
                            | "local_session_log"
                    ) =>
                {
                    Some(format!("{field}:{window}:account"))
                }
                [
                    source,
                    field @ ("used" | "reset"),
                    window,
                    "session",
                    session_id,
                ] if matches!(
                    *source,
                    "broker_projection"
                        | "statusline"
                        | "provider_spend"
                        | "operator"
                        | "local_session_log"
                ) =>
                {
                    Some(format!("{field}:{window}:{session_id}"))
                }
                _ => None,
            };
            fingerprints.insert(legacy_key.unwrap_or(key), value);
        }
        for event in monitor["events"].as_array_mut().expect("monitor events") {
            assert!(
                event["status"]["readiness"]
                    .as_object_mut()
                    .expect("V5 event readiness")
                    .remove("provider")
                    .is_some(),
                "V4 event status does not contain provider readiness"
            );
            event["status"]["schema_version"] = serde_json::json!(4);
        }
    }
    assert_ne!(
        snapshot["accounts"][account_id]["broker_windows"][0]["used"],
        serde_json::Value::Null,
        "fixture includes legacy provider quota without source attribution"
    );
    drop(store);
    overwrite_persisted_state(&directory, &snapshot);
    (
        directory,
        snapshot,
        account_id.to_owned(),
        goal_id.to_owned(),
        monitor_id,
    )
}

#[test]
fn v4_migration_preserves_strict_history_and_drops_unscoped_provider_quota() {
    let (directory, legacy, account_id, goal_id, monitor_id) = v4_history_fixture();
    let migrated = MonitorStore::open(directory.path()).expect("migrate V4 store");
    let state = migrated.lock();
    let snapshot = serde_json::to_value(&*state).expect("serialize migrated store");

    assert_eq!(snapshot["schema_version"], serde_json::json!(5));
    assert_eq!(
        snapshot["goals"][goal_id.as_str()],
        legacy["goals"][goal_id.as_str()]
    );
    assert_eq!(
        snapshot["policy_records"][goal_id.as_str()],
        legacy["policy_records"][goal_id.as_str()]
    );
    assert_eq!(
        snapshot["accounts"][account_id.as_str()]["spend"],
        legacy["accounts"][account_id.as_str()]["spend"]
    );

    let mut expected_monitor = legacy["monitors"][monitor_id.as_str()].clone();
    assert_eq!(
        expected_monitor["config"]["goal_id"],
        serde_json::json!(goal_id)
    );
    assert_ne!(expected_monitor["latest_decision"], serde_json::Value::Null);
    assert_ne!(expected_monitor["spend_state"], serde_json::Value::Null);
    assert!(
        !expected_monitor["events"]
            .as_array()
            .expect("legacy events")
            .is_empty()
    );
    for event in expected_monitor["events"]
        .as_array_mut()
        .expect("legacy events")
    {
        event["status"]["schema_version"] = serde_json::json!(5);
        event["status"]["readiness"]["provider"] = serde_json::json!("unknown");
    }
    assert_eq!(
        snapshot["monitors"][monitor_id.as_str()]["config"],
        expected_monitor["config"]
    );
    assert_eq!(
        snapshot["monitors"][monitor_id.as_str()]["latest_decision"],
        expected_monitor["latest_decision"]
    );
    assert_eq!(
        snapshot["monitors"][monitor_id.as_str()]["spend_state"],
        expected_monitor["spend_state"]
    );
    assert_eq!(
        snapshot["monitors"][monitor_id.as_str()]["events"],
        expected_monitor["events"]
    );

    let account = &state.accounts[account_id.as_str()];
    assert!(account.provider_observation.is_none());
    assert!(account.broker_windows.iter().all(|window| {
        window.used.is_none() && window.reset.is_none() && window.paired.is_none()
    }));
    assert_eq!(
        account.latest_reset_epochs,
        [Some(NOW + 1_800), Some(NOW + 80_000)],
        "dropping unscoped provider quota retains reset watermarks sourced from statusline"
    );
    let migrated_monitor = &state.monitors[monitor_id.as_str()];
    assert!(
        migrated_monitor
            .evidence
            .iter()
            .all(|evidence| evidence.source != MonitorEvidenceSource::BrokerProjection)
    );
    assert!(migrated_monitor.reset_barriers.iter().all(Option::is_none));
    drop(state);

    let current = descriptor_status(&migrated, &monitor_id, NOW + 6);
    assert_eq!(
        current.readiness.provider,
        MonitorProviderReadiness::Unknown
    );
    assert_eq!(current.readiness.quota, MonitorQuotaReadiness::Unknown);
    assert_eq!(current.five_hour.used_percentage_basis_points, None);
    assert_eq!(current.seven_day.used_percentage_basis_points, None);
    assert!(!current.runnable);
}

#[test]
fn v4_migration_rejects_malformed_event_schema_without_rewriting_source() {
    let (directory, mut snapshot, _, _, _) = v4_history_fixture();
    let monitor = snapshot["monitors"]
        .as_object_mut()
        .expect("persisted monitors")
        .values_mut()
        .find(|monitor| {
            monitor["events"]
                .as_array()
                .is_some_and(|events| !events.is_empty())
        })
        .expect("fixture has a monitor with events");
    monitor["events"][0]["status"]["schema_version"] = serde_json::json!(5);
    let source_bytes = serde_json::to_vec(&snapshot).expect("serialize malformed V4 source");
    std::fs::write(persisted_state_path(&directory), &source_bytes)
        .expect("write malformed V4 source");

    let error = MonitorStore::open(directory.path()).expect_err("reject malformed V4 event schema");
    assert_eq!(error.code, MonitorIssueCode::MonitorStoreUnavailable);
    assert_eq!(
        std::fs::read(persisted_state_path(&directory)).expect("read rejected source"),
        source_bytes,
        "a rejected migration must not rewrite its source"
    );
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn legacy_quota_fingerprint_key(key: &str) -> Option<(String, &str)> {
    let mut parts = key.splitn(4, ':');
    let source = parts.next()?;
    if !matches!(
        source,
        "broker_projection" | "statusline" | "provider_spend" | "operator" | "local_session_log"
    ) {
        return None;
    }
    let field = parts.next()?;
    let window = parts.next()?;
    let scope = parts.next()?;
    if !matches!(field, "used" | "reset") || !matches!(window, "five_hour" | "seven_day") {
        return None;
    }
    let legacy_scope = scope.strip_prefix("session:").unwrap_or(scope);
    Some((format!("{field}:{window}:{legacy_scope}"), source))
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "The V4 fixture must verify the legacy collision, migration, and unchanged replay together."
)]
fn v4_fingerprint_migration_preserves_account_named_statusline_session_and_drops_provider_data() {
    let (directory, store) = open_store();
    let (binding, monitor_id) = start_approved_projection_observer(
        &store,
        "operator-local-fingerprint-migration",
        "acct-provider-freshness",
        NOW,
    );
    let observation = observation_with_windows("account", Some(42), Some(NOW + 3_600), None, None);
    store
        .operate(
            MonitorOperation::Ingest {
                scope: MonitorScope::BoundAccount {
                    binding_id: binding.binding_id.clone(),
                    binding_revision: binding.revision,
                    session_id: None,
                },
                observation: observation.clone(),
            },
            NOW,
        )
        .expect("ingest statusline session literally named account");
    store
        .observe_projection(&provider_projection(NOW + 1, 1, 1), NOW + 1)
        .expect("publish broker quota for the account");

    let (original_sequence, original_received_at, original_statusline_fingerprint) = {
        let state = store.lock();
        let monitor = &state.monitors[&monitor_id];
        let statusline_used = monitor
            .evidence
            .iter()
            .find(|evidence| {
                evidence.source == MonitorEvidenceSource::Statusline
                    && evidence.session_id.as_deref() == Some("account")
                    && matches!(
                        &evidence.value,
                        MonitorEvidenceValue::QuotaUsedPercentage {
                            window: MonitorQuotaWindow::FiveHour,
                            ..
                        }
                    )
            })
            .expect("monitor has five-hour statusline used evidence");
        assert_eq!(statusline_used.evidence_received_at_epoch, NOW);
        let fingerprint = monitor
            .evidence_fingerprints
            .get("statusline:used:five_hour:session:account")
            .expect("current statusline fingerprint uses a source and session scope")
            .clone();
        assert!(
            monitor
                .evidence_fingerprints
                .contains_key("broker_projection:reset:five_hour:account")
        );
        (
            statusline_used.sequence,
            statusline_used.evidence_received_at_epoch,
            fingerprint,
        )
    };

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

    let fingerprints = snapshot["monitors"][monitor_id.as_str()]["evidence_fingerprints"]
        .as_object_mut()
        .expect("persisted evidence fingerprints");
    let current_fingerprints = std::mem::take(fingerprints);
    let mut legacy_fingerprints = serde_json::Map::new();
    let mut legacy_sources = BTreeMap::<String, String>::new();
    for (key, value) in current_fingerprints {
        let Some((legacy_key, source)) = legacy_quota_fingerprint_key(&key) else {
            legacy_fingerprints.insert(key, value);
            continue;
        };
        let Some(previous_source) = legacy_sources.get(&legacy_key) else {
            legacy_fingerprints.insert(legacy_key.clone(), value);
            legacy_sources.insert(legacy_key, source.to_owned());
            continue;
        };
        let new_source_wins = match legacy_key.as_str() {
            "used:five_hour:account" => source == "broker_projection",
            "reset:five_hour:account" => source == "broker_projection",
            _ => panic!("unexpected duplicate legacy fingerprint key: {legacy_key}"),
        };
        if new_source_wins {
            legacy_fingerprints.insert(legacy_key.clone(), value);
            legacy_sources.insert(legacy_key, source.to_owned());
        } else {
            assert!(matches!(
                (legacy_key.as_str(), previous_source.as_str()),
                (
                    "used:five_hour:account" | "reset:five_hour:account",
                    "broker_projection"
                )
            ));
        }
    }
    assert_eq!(
        legacy_sources["used:five_hour:account"],
        "broker_projection"
    );
    assert_eq!(
        legacy_sources["reset:five_hour:account"],
        "broker_projection"
    );
    assert!(
        legacy_fingerprints
            .keys()
            .all(|key| legacy_quota_fingerprint_key(key).is_none())
    );
    *fingerprints = legacy_fingerprints;
    drop(store);
    overwrite_persisted_state(&directory, &snapshot);

    let migrated = MonitorStore::open(directory.path()).expect("migrate V4 fingerprint state");
    let next_input_sequence = {
        let state = migrated.lock();
        let monitor = &state.monitors[&monitor_id];
        let statusline_used = monitor
            .evidence
            .iter()
            .find(|evidence| {
                evidence.source == MonitorEvidenceSource::Statusline
                    && evidence.session_id.as_deref() == Some("account")
                    && matches!(
                        &evidence.value,
                        MonitorEvidenceValue::QuotaUsedPercentage {
                            window: MonitorQuotaWindow::FiveHour,
                            ..
                        }
                    )
            })
            .expect("migrated statusline evidence remains available");
        assert_eq!(statusline_used.sequence, original_sequence);
        assert_eq!(
            statusline_used.evidence_received_at_epoch,
            original_received_at
        );
        assert!(
            monitor
                .evidence_fingerprints
                .contains_key("statusline:used:five_hour:session:account")
        );
        assert_eq!(
            monitor.evidence_fingerprints["statusline:used:five_hour:session:account"],
            original_statusline_fingerprint
        );
        assert!(
            !monitor
                .evidence_fingerprints
                .contains_key("used:five_hour:account")
        );
        assert!(
            !monitor
                .evidence_fingerprints
                .contains_key("reset:five_hour:account")
        );
        assert!(
            !monitor
                .evidence_fingerprints
                .keys()
                .any(|key| key.starts_with("broker_projection:"))
        );
        assert!(
            !monitor
                .evidence
                .iter()
                .any(|evidence| evidence.source == MonitorEvidenceSource::BrokerProjection)
        );
        state.next_input_sequence
    };

    let before_replay = descriptor_status(&migrated, &monitor_id, NOW + 100);
    let used_before_replay = before_replay
        .evidence
        .iter()
        .find(|evidence| {
            evidence.source == MonitorEvidenceSource::Statusline
                && evidence.session_id.as_deref() == Some("account")
                && matches!(
                    &evidence.value,
                    MonitorEvidenceValue::QuotaUsedPercentage {
                        window: MonitorQuotaWindow::FiveHour,
                        ..
                    }
                )
        })
        .expect("status retains statusline evidence");
    assert_eq!(used_before_replay.sequence, original_sequence);
    assert_eq!(
        used_before_replay.evidence_received_at_epoch,
        original_received_at
    );
    assert_eq!(used_before_replay.age_seconds, 100);

    migrated
        .operate(
            MonitorOperation::Ingest {
                scope: MonitorScope::BoundAccount {
                    binding_id: binding.binding_id,
                    binding_revision: binding.revision,
                    session_id: None,
                },
                observation,
            },
            NOW + 200,
        )
        .expect("replay identical statusline input");
    let after_replay = descriptor_status(&migrated, &monitor_id, NOW + 200);
    let used_after_replay = after_replay
        .evidence
        .iter()
        .find(|evidence| {
            evidence.source == MonitorEvidenceSource::Statusline
                && evidence.session_id.as_deref() == Some("account")
                && matches!(
                    &evidence.value,
                    MonitorEvidenceValue::QuotaUsedPercentage {
                        window: MonitorQuotaWindow::FiveHour,
                        ..
                    }
                )
        })
        .expect("replayed statusline evidence remains available");
    assert_eq!(used_after_replay.sequence, original_sequence);
    assert_eq!(
        used_after_replay.evidence_received_at_epoch,
        original_received_at
    );
    assert_eq!(used_after_replay.age_seconds, 200);
    let state = migrated.lock();
    assert_eq!(state.next_input_sequence, next_input_sequence);
    assert_eq!(
        state.monitors[&monitor_id].evidence_fingerprints["statusline:used:five_hour:session:account"],
        original_statusline_fingerprint
    );
}

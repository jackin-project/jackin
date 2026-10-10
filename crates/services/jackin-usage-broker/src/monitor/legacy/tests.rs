// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

mod binding_union_tests {
    use super::*;

    fn empty_spend() -> V1SpendState {
        V1SpendState {
            baseline: None,
            period_anchor: None,
            cumulative_goal_spend: None,
            rollover_unknown: false,
            cumulative_complete: false,
        }
    }

    fn empty_account() -> V1AccountObservations {
        V1AccountObservations {
            sessions: BTreeMap::new(),
            broker_windows: std::array::from_fn(|_| V1ObservedWindow {
                used: None,
                reset: None,
                paired: None,
            }),
            latest_reset_epochs: [None, None],
            reset_barriers: [None, None],
            input_sequence: 0,
            spend: V1SpendAccountState {
                latest_record: None,
                current_period_record: None,
                previous_period_record: None,
            },
        }
    }

    fn empty_monitor(account_id: String, goal_id: String) -> V1DurableMonitor {
        V1DurableMonitor {
            config: V1MonitorConfig {
                provider: V1MonitorProvider::Claude,
                account_id,
                goal_id,
                session_id: None,
                expected_model: None,
                budget: None,
            },
            created_at_epoch: 0,
            stopped_at_epoch: None,
            updated_at_epoch: 0,
            last_reconciled_at_epoch: 0,
            next_evidence_sequence: 0,
            next_decision_sequence: 0,
            next_event_sequence: 0,
            evidence: Vec::new(),
            evidence_fingerprints: BTreeMap::new(),
            reset_barriers: [None, None],
            latest_decision: None,
            decision_fingerprint: None,
            events: Vec::new(),
            spend_state: empty_spend(),
        }
    }

    #[test]
    fn migration_binding_bound_covers_full_v1_account_goal_monitor_union() {
        let accounts = (0..MAX_ACCOUNTS)
            .map(|index| (format!("acct-account-{index}"), empty_account()))
            .collect();
        let goals = (0..MAX_GOALS)
            .map(|index| {
                (
                    format!("goal-{index}"),
                    V1DurableGoalSpend {
                        account_id: format!("acct-goal-{index}"),
                        budget: None,
                        spend_state: empty_spend(),
                    },
                )
            })
            .collect();
        let monitors = (0..MAX_MONITORS)
            .map(|index| {
                (
                    format!("monitor-{:08}", index + 1),
                    empty_monitor(format!("acct-monitor-{index}"), "goal-0".to_owned()),
                )
            })
            .collect();
        let state = V1StoreState {
            schema_version: V1_SCHEMA_VERSION,
            next_monitor_id: MAX_MONITORS as u64 + 1,
            next_input_sequence: 0,
            last_now_epoch: 0,
            accounts,
            monitors,
            goals,
        };

        let ids = binding_ids(&state).expect("allocate every bounded legacy account identity");
        assert_eq!(ids.len(), MAX_ACCOUNTS + MAX_GOALS + MAX_MONITORS);
        let bindings = build_bindings(&ids).expect("build migrated unconfirmed bindings");
        assert_eq!(bindings.len(), super::MAX_BINDINGS);
    }

    #[test]
    fn migration_drops_only_fingerprints_for_pruned_legacy_sessions() {
        let fingerprints = BTreeMap::from([
            ("model:active-session".to_owned(), "model-active".to_owned()),
            ("model:old-session".to_owned(), "model-old".to_owned()),
            (
                "used:five_hour:active-session".to_owned(),
                "used-active".to_owned(),
            ),
            (
                "reset:seven_day:old-session".to_owned(),
                "reset-old".to_owned(),
            ),
            (
                "used:five_hour:account".to_owned(),
                "account-used".to_owned(),
            ),
            ("spend:account".to_owned(), "account-spend".to_owned()),
        ]);
        let retained = retain_migrated_evidence_fingerprints(
            fingerprints,
            &BTreeSet::from(["active-session".to_owned()]),
        );
        assert_eq!(retained.len(), 4);
        assert!(retained.contains_key("model:active-session"));
        assert!(retained.contains_key("used:five_hour:active-session"));
        assert!(retained.contains_key("used:five_hour:account"));
        assert!(retained.contains_key("spend:account"));
        assert!(!retained.contains_key("model:old-session"));
        assert!(!retained.contains_key("reset:seven_day:old-session"));
    }

    #[test]
    fn v1_event_status_evidence_requires_unique_sequences_within_the_snapshot() {
        let config = V1MonitorConfig {
            provider: V1MonitorProvider::Claude,
            account_id: "acct-v1-event-evidence".to_owned(),
            goal_id: "goal-v1-event-evidence".to_owned(),
            session_id: Some("session-v1-event-evidence".to_owned()),
            expected_model: None,
            budget: None,
        };
        let evidence = || V1MonitorEvidence {
            sequence: 1,
            account_id: config.account_id.clone(),
            session_id: config.session_id.clone(),
            source: V1MonitorEvidenceSource::Statusline,
            evidence_at_epoch: Some(0),
            evidence_received_at_epoch: 0,
            age_seconds: 0,
            value: V1MonitorEvidenceValue::Model {
                model: "claude-sonnet".to_owned(),
            },
        };
        let mut status = V1MonitorStatus {
            schema_version: V1_SCHEMA_VERSION,
            monitor_id: "monitor-00000001".to_owned(),
            provider: V1MonitorProvider::Claude,
            account_id: config.account_id.clone(),
            goal_id: config.goal_id.clone(),
            session_id: config.session_id.clone(),
            model: Some("claude-sonnet".to_owned()),
            model_evidence: None,
            lifecycle: V1MonitorLifecycle::Active,
            runnable: false,
            five_hour: V1MonitorQuotaWindowStatus {
                used_percentage_basis_points: None,
                used_evidence: None,
                reset_at_epoch: None,
                reset_evidence: None,
            },
            seven_day: V1MonitorQuotaWindowStatus {
                used_percentage_basis_points: None,
                used_evidence: None,
                reset_at_epoch: None,
                reset_evidence: None,
            },
            budget: None,
            cumulative_goal_spend: None,
            spend_period_baseline: None,
            evidence: vec![evidence(), evidence()],
            latest_decision: None,
            updated_at_epoch: 0,
            issues: Vec::new(),
        };
        assert!(!status.is_valid_for("monitor-00000001", &config, 1, 0, 0));
        status.evidence.pop();
        assert!(status.is_valid_for("monitor-00000001", &config, 1, 0, 0));
        status.evidence[0].sequence = 2;
        assert!(!status.is_valid_for("monitor-00000001", &config, 1, 0, 0));
    }
}

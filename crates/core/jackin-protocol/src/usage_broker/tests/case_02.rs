// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn assert_literal_value_payload(group: &UsageMetricGroupV1, wire: &serde_json::Value) {
    let value = &wire["value"];
    match &group.value {
        UsageMetricValueV1::Window { .. } => {
            assert_eq!(value["remaining_percent"], 57.0);
            assert_eq!(value["meter_percent"], 57.0);
            assert_eq!(value["meter_side"], "remaining");
            assert_eq!(
                value["period"],
                serde_json::json!({"period": "calendar", "granularity": "weekly"})
            );
            assert!(value.get("count_quota").is_none());
        }
        UsageMetricValueV1::Balance { .. } => {
            assert_eq!(
                value["amount"],
                serde_json::json!({
                    "amount_minor": 1_250,
                    "currency": "USD",
                    "exponent": 2
                })
            );
            assert_eq!(value["expires_at_epoch"], 1_900_000_000);
        }
        UsageMetricValueV1::SpendCap { .. } => {
            assert_eq!(value["cap"]["amount_minor"], 30_000);
            assert_eq!(value["spent"]["amount_minor"], 2_700);
            assert_eq!(value["remaining"]["amount_minor"], 300);
            assert_eq!(value["cap"]["currency"], "USD");
            assert_eq!(value["cap"]["exponent"], 2);
        }
        UsageMetricValueV1::TokenTotals { .. } => {
            assert_eq!(value["input"], 1_000);
            assert_eq!(value["output"], 2_000);
            assert_eq!(value["reasoning"], 50);
            assert_eq!(value["interval_label"], "current period");
            assert!(value.get("cached").is_none());
        }
        UsageMetricValueV1::RateLimit { .. } => {
            assert_eq!(value["limit"], 60);
            assert_eq!(value["remaining"], 59);
            assert_eq!(value["window_label"], "per minute");
        }
        UsageMetricValueV1::Plan { .. } => {
            assert_eq!(value["plan_label"], "Pro");
            assert!(value.get("tier").is_none());
            assert_eq!(wire["renews_at_epoch"], 1_900_000_000);
        }
    }
}

#[test]
fn metric_groups_round_trip_all_six_wire_values() {
    let groups = [
        ("window", window_group("g-window", 0)),
        (
            "balance",
            UsageMetricGroupV1 {
                group_id: "g-balance".into(),
                rank: 1,
                kind: UsageMetricGroupKindV1::Balance,
                label: "Credits".into(),
                quota_state: UsageQuotaStateV1::Available,
                value: UsageMetricValueV1::Balance {
                    amount: Money::new(1_250, "USD", 2),
                    expires_at_epoch: Some(1_900_000_000),
                },
                reset_at_epoch: None,
                ..window_group("g-balance", 1)
            },
        ),
        (
            "spend_cap",
            UsageMetricGroupV1 {
                group_id: "g-spend".into(),
                rank: 2,
                kind: UsageMetricGroupKindV1::SpendCap,
                label: "Extra usage".into(),
                quota_state: UsageQuotaStateV1::Warning,
                value: UsageMetricValueV1::SpendCap {
                    cap: Some(Money::new(30_000, "USD", 2)),
                    spent: Some(Money::new(2_700, "USD", 2)),
                    remaining: Some(Money::new(300, "USD", 2)),
                },
                ..window_group("g-spend", 2)
            },
        ),
        (
            "token_totals",
            UsageMetricGroupV1 {
                group_id: "g-tokens".into(),
                rank: 3,
                kind: UsageMetricGroupKindV1::TokenTotals,
                label: "Token totals".into(),
                quota_state: UsageQuotaStateV1::NotApplicable,
                value: UsageMetricValueV1::TokenTotals {
                    input: Some(1_000),
                    output: Some(2_000),
                    cached: None,
                    reasoning: Some(50),
                    interval_label: Some("current period".into()),
                },
                reset_at_epoch: None,
                ..window_group("g-tokens", 3)
            },
        ),
        (
            "rate_limit",
            UsageMetricGroupV1 {
                group_id: "g-rate".into(),
                rank: 4,
                kind: UsageMetricGroupKindV1::RateLimit,
                label: "Requests per minute".into(),
                quota_state: UsageQuotaStateV1::Available,
                value: UsageMetricValueV1::RateLimit {
                    limit: Some(60),
                    remaining: Some(59),
                    window_label: Some("per minute".into()),
                },
                ..window_group("g-rate", 4)
            },
        ),
        (
            "plan",
            UsageMetricGroupV1 {
                group_id: "g-plan".into(),
                rank: 5,
                kind: UsageMetricGroupKindV1::Plan,
                label: "Plan".into(),
                quota_state: UsageQuotaStateV1::NotApplicable,
                value: UsageMetricValueV1::Plan {
                    plan_label: Some("Pro".into()),
                    tier: None,
                },
                reset_at_epoch: None,
                renews_at_epoch: Some(1_900_000_000),
                ..window_group("g-plan", 5)
            },
        ),
    ];

    for (rank, (expected_tag, group)) in groups.iter().enumerate() {
        group.validate(rank).unwrap();
        let wire = serde_json::to_value(group).unwrap();
        assert_eq!(wire["kind"], *expected_tag);
        assert_eq!(wire["value"]["type"], *expected_tag);
        assert_literal_value_payload(group, &wire);
        let decoded = serde_json::from_value::<UsageMetricGroupV1>(wire).unwrap();
        assert_eq!(&decoded, group);
    }
}

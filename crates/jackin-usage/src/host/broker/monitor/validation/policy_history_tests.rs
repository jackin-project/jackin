// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{
    MonitorAccountBinding, MonitorPolicy, MonitorPolicyOrigin, MonitorPolicyRecord, StoreState,
    validate_store_state,
};
use jackin_protocol::control::Money;
use jackin_protocol::usage_monitor::{MonitorProvider, USAGE_MONITOR_SCHEMA_VERSION};

const BINDING_ID: &str = "binding-00000001";
const OTHER_BINDING_ID: &str = "binding-00000002";
const GOAL_ID: &str = "goal-history-test";

fn valid_state() -> StoreState {
    let mut state = StoreState {
        schema_version: USAGE_MONITOR_SCHEMA_VERSION,
        next_monitor_id: 1,
        next_input_sequence: 0,
        next_binding_id: 2,
        last_now_epoch: 100,
        accounts: Default::default(),
        unbound_sessions: Default::default(),
        bindings: Default::default(),
        policy_records: Default::default(),
        monitors: Default::default(),
        goals: Default::default(),
    };
    state.bindings.insert(
        BINDING_ID.to_owned(),
        vec![MonitorAccountBinding {
            binding_id: BINDING_ID.to_owned(),
            provider: MonitorProvider::Claude,
            account_id: "account-history-test".to_owned(),
            provider_account_id: None,
            experimental_collector_approved: false,
            operator_label: "test operator".to_owned(),
            revision: 1,
            operator_confirmed: true,
            confirmed_at_epoch: Some(10),
        }],
    );
    state
}

fn policy_record(
    revision: u64,
    previous_policy: Option<MonitorPolicy>,
    new_policy: MonitorPolicy,
    budget: Option<Money>,
) -> MonitorPolicyRecord {
    MonitorPolicyRecord {
        provider: MonitorProvider::Claude,
        account_id: "account-history-test".to_owned(),
        binding_id: Some(BINDING_ID.to_owned()),
        binding_revision: Some(1),
        goal_id: GOAL_ID.to_owned(),
        previous_policy,
        new_policy,
        budget,
        operator_label: Some("test operator".to_owned()),
        operator_confirmed: true,
        acknowledge_no_sgd_cap: new_policy == MonitorPolicy::QuotaOnly,
        recorded_at_epoch: Some(20 + i64::try_from(revision).expect("small test revision")),
        revision,
        origin: MonitorPolicyOrigin::Operator,
    }
}

fn strict_budget(amount_minor: i64) -> Money {
    Money::new(amount_minor, "SGD", 2)
}

fn assert_store_unavailable(state: &StoreState) {
    let error = validate_store_state(state).expect_err("corrupt policy history must fail closed");
    assert_eq!(
        error.code,
        super::super::MonitorIssueCode::MonitorStoreUnavailable
    );
}

#[test]
fn persisted_strict_policy_cannot_be_downgraded_to_quota_only() {
    let mut state = valid_state();
    state.policy_records.insert(
        GOAL_ID.to_owned(),
        vec![
            policy_record(1, None, MonitorPolicy::StrictSgd, Some(strict_budget(5_000))),
            policy_record(
                2,
                Some(MonitorPolicy::StrictSgd),
                MonitorPolicy::QuotaOnly,
                None,
            ),
        ],
    );

    assert_store_unavailable(&state);
}

#[test]
fn persisted_strict_budget_cannot_increase() {
    let mut state = valid_state();
    state.policy_records.insert(
        GOAL_ID.to_owned(),
        vec![
            policy_record(1, None, MonitorPolicy::StrictSgd, Some(strict_budget(5_000))),
            policy_record(
                2,
                Some(MonitorPolicy::StrictSgd),
                MonitorPolicy::StrictSgd,
                Some(strict_budget(5_001)),
            ),
        ],
    );

    assert_store_unavailable(&state);
}

#[test]
fn persisted_strict_budget_can_stay_equal_or_tighten() {
    for next_budget in [5_000, 4_999] {
        let mut state = valid_state();
        state.policy_records.insert(
            GOAL_ID.to_owned(),
            vec![
                policy_record(1, None, MonitorPolicy::StrictSgd, Some(strict_budget(5_000))),
                policy_record(
                    2,
                    Some(MonitorPolicy::StrictSgd),
                    MonitorPolicy::StrictSgd,
                    Some(strict_budget(next_budget)),
                ),
            ],
        );

        validate_store_state(&state).expect("equal or tighter policy history is valid");
    }
}

#[test]
fn persisted_unactivated_quota_only_policy_can_be_replaced_with_strict() {
    let mut state = valid_state();
    state.policy_records.insert(
        GOAL_ID.to_owned(),
        vec![
            policy_record(1, None, MonitorPolicy::QuotaOnly, None),
            policy_record(
                2,
                Some(MonitorPolicy::QuotaOnly),
                MonitorPolicy::StrictSgd,
                Some(strict_budget(5_000)),
            ),
        ],
    );

    validate_store_state(&state).expect("an unactivated quota-only policy can be replaced");
}

#[test]
fn persisted_migrated_zero_budget_repair_remains_valid() {
    let mut state = valid_state();
    let mut migrated = policy_record(1, None, MonitorPolicy::StrictSgd, Some(strict_budget(0)));
    migrated.binding_id = None;
    migrated.binding_revision = None;
    migrated.operator_label = None;
    migrated.operator_confirmed = false;
    migrated.acknowledge_no_sgd_cap = false;
    migrated.recorded_at_epoch = None;
    migrated.origin = MonitorPolicyOrigin::MigratedV1;

    state.policy_records.insert(
        GOAL_ID.to_owned(),
        vec![
            migrated,
            policy_record(
                2,
                Some(MonitorPolicy::StrictSgd),
                MonitorPolicy::StrictSgd,
                Some(strict_budget(1)),
            ),
        ],
    );

    validate_store_state(&state).expect("the explicit migration repair remains valid");
}

#[test]
fn persisted_policy_history_cannot_move_between_accounts() {
    let mut state = valid_state();
    state.next_binding_id = 3;
    state.bindings.insert(
        OTHER_BINDING_ID.to_owned(),
        vec![MonitorAccountBinding {
            binding_id: OTHER_BINDING_ID.to_owned(),
            provider: MonitorProvider::Claude,
            account_id: "different-account".to_owned(),
            provider_account_id: None,
            experimental_collector_approved: false,
            operator_label: "test operator".to_owned(),
            revision: 1,
            operator_confirmed: true,
            confirmed_at_epoch: Some(10),
        }],
    );
    let mut second = policy_record(
        2,
        Some(MonitorPolicy::StrictSgd),
        MonitorPolicy::StrictSgd,
        Some(strict_budget(4_999)),
    );
    second.account_id = "different-account".to_owned();
    second.binding_id = Some(OTHER_BINDING_ID.to_owned());
    state.policy_records.insert(
        GOAL_ID.to_owned(),
        vec![
            policy_record(1, None, MonitorPolicy::StrictSgd, Some(strict_budget(5_000))),
            second,
        ],
    );

    assert_store_unavailable(&state);
}

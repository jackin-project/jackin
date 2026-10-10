// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn broker_policy_freezes_cadence_matrix() {
    assert_eq!(
        cadence(UsageActivity::DirectInteraction, false),
        Duration::from_mins(2)
    );
    assert_eq!(
        cadence(UsageActivity::Recent, false),
        Duration::from_mins(5)
    );
    assert_eq!(cadence(UsageActivity::Idle, false), Duration::from_mins(15));
    assert_eq!(
        cadence(UsageActivity::LongIdle, false),
        Duration::from_mins(30)
    );
    assert_eq!(
        cadence(UsageActivity::DirectInteraction, true),
        Duration::from_mins(30)
    );
}

#[test]
fn broker_policy_retry_is_positive_exponential_stable_and_provider_wins() {
    let capability = UsageAccountCapability {
        account_id: "account".to_owned(),
        surface_id: "openai".to_owned(),
    };
    let policy = UsagePolicy::default();
    let first = retry_deadline(policy, &capability, 4, 2, None, 1000).expect("deadline");
    let second = retry_deadline(policy, &capability, 4, 2, None, 1000).expect("deadline");
    assert_eq!(first, second);
    assert!((1_060..=1_075).contains(&first));
    let earlier_failure =
        retry_deadline(policy, &capability, 4, 1, None, 1000).expect("first failure deadline");
    assert!((1_030..=1_037).contains(&earlier_failure));
    assert!(first > earlier_failure);
    assert_eq!(
        retry_deadline(policy, &capability, 4, 2, Some(5000), 1000),
        Some(5000)
    );
}

#[test]
fn claude_minimum_attempt_deadline_uses_persisted_invocation() {
    let capability = UsageAccountCapability {
        account_id: "account".to_owned(),
        surface_id: "claude".to_owned(),
    };
    assert_eq!(
        minimum_attempt_deadline(&capability, Some(1_000)),
        Some(1_300)
    );
    assert_eq!(minimum_attempt_deadline(&capability, None), None);

    let other = UsageAccountCapability {
        surface_id: "openai".to_owned(),
        ..capability
    };
    assert_eq!(minimum_attempt_deadline(&other, Some(1_000)), None);
}

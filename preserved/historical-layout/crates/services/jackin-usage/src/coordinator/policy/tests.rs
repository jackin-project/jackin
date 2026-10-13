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
fn broker_policy_retry_jitter_is_stable_bounded_and_provider_wins() {
    let capability = UsageAccountCapability {
        account_id: "account".to_owned(),
        surface_id: "openai".to_owned(),
    };
    let policy = UsagePolicy::default();
    let first = retry_deadline(policy, &capability, 4, 2, None, 1000).expect("deadline");
    let second = retry_deadline(policy, &capability, 4, 2, None, 1000).expect("deadline");
    assert_eq!(first, second);
    assert!((1000..=1060).contains(&first));
    assert_eq!(
        retry_deadline(policy, &capability, 4, 2, Some(5000), 1000),
        Some(5000)
    );
}

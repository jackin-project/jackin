// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::push_amp_agent_dollar_bucket;
use jackin_protocol::control::{Money, QuotaBucketView};

#[test]
fn amp_agent_bucket_skips_unrepresentable_checked_subtraction() {
    let mut buckets: Vec<QuotaBucketView> = Vec::new();
    push_amp_agent_dollar_bucket(
        &mut buckets,
        &Money::new(i64::MIN, "USD", 0),
        &Money::new(i64::MAX, "USD", 0),
        None,
        0,
        None,
    );
    assert!(buckets.is_empty());
}

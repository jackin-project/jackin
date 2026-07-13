// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `pull_request`.
use super::PullRequestChecks;

#[test]
fn pull_request_checks_from_buckets_keeps_sum_equals_total_for_known_inputs() {
    let checks =
        PullRequestChecks::from_buckets(["pass", "pass", "fail", "pending", "skipping", "cancel"]);
    assert_eq!(checks.total(), 6);
    assert_eq!(checks.passing(), 2);
    assert_eq!(checks.failing(), 1);
    assert_eq!(checks.pending(), 1);
    assert_eq!(checks.skipped(), 1);
    assert_eq!(checks.cancelled(), 1);
}

#[test]
fn pull_request_checks_from_buckets_routes_unknown_into_skipped() {
    let checks = PullRequestChecks::from_buckets(["pass", "unknown-bucket", "another-bucket"]);
    assert_eq!(checks.total(), 3);
    assert_eq!(checks.passing(), 1);
    assert_eq!(checks.skipped(), 2, "unknown buckets fall into skipped");
    assert_eq!(
        checks.passing()
            + checks.failing()
            + checks.pending()
            + checks.skipped()
            + checks.cancelled(),
        checks.total(),
        "counters must always sum to total"
    );
}

#[test]
fn pull_request_checks_from_buckets_empty_yields_zero_total() {
    let checks = PullRequestChecks::from_buckets(std::iter::empty::<&str>());
    assert_eq!(checks.total(), 0);
    assert_eq!(checks.summary(), "(none)");
}

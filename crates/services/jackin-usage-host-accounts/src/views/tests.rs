// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalRemainingPercent, CanonicalUsedPercent, CountQuota, CountQuotaPeriod,
    CountQuotaProvenance, CountQuotaUnit, FocusedUsageView, PercentObservationOrigin,
    ProviderQuotaState, QuotaBucketView, UsageSeverity, UsageSnapshotStatus,
};

use super::{min_meter_fill, summary_bucket};

fn empty_bucket(label: &str) -> QuotaBucketView {
    QuotaBucketView {
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: None,
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

#[test]
fn minimum_fill_preserves_fractional_used_source_order() {
    let mut lower_used = empty_bucket("lower used");
    lower_used.used_percent = Some(CanonicalUsedPercent::new(43.374).expect("valid used"));
    lower_used.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    let mut higher_used = empty_bucket("higher used");
    higher_used.used_percent = Some(CanonicalUsedPercent::new(43.375).expect("valid used"));
    higher_used.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    let mut view = FocusedUsageView::unavailable("fixture", 1_800_000_000);

    for buckets in [
        vec![lower_used.clone(), higher_used.clone()],
        vec![higher_used.clone(), lower_used.clone()],
    ] {
        view.buckets = buckets;
        assert_eq!(
            min_meter_fill(&view).map(jackin_protocol::control::MeterPercentPoints::get),
            Some(56.625)
        );
    }
}

#[test]
fn incomplete_requests_count_does_not_rank_from_conflicting_raw_percent() {
    let mut incomplete_count = empty_bucket("incomplete count");
    incomplete_count.remaining_percent =
        Some(CanonicalRemainingPercent::new(0.0).expect("valid provider remaining"));
    incomplete_count.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    incomplete_count.count_quota = Some(CountQuota::new(
        Some(9),
        Some(0),
        None,
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    let mut complete = empty_bucket("complete percent");
    complete.remaining_percent =
        Some(CanonicalRemainingPercent::new(50.0).expect("valid provider remaining"));
    complete.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    let mut view = FocusedUsageView::unavailable("fixture", 1_800_000_000);
    view.buckets = vec![incomplete_count, complete];

    assert_eq!(
        min_meter_fill(&view).map(jackin_protocol::control::MeterPercentPoints::get),
        Some(50.0)
    );
}

#[test]
fn state_only_bucket_remains_eligible_for_account_summary_without_geometry() {
    let mut state_only = empty_bucket("explicit state");
    state_only.provider_quota_state = Some(ProviderQuotaState::Unlimited);
    let mut view = FocusedUsageView::unavailable("fixture", 1_800_000_000);
    view.buckets = vec![state_only];

    assert_eq!(
        summary_bucket(&view).and_then(|bucket| bucket.provider_quota_state),
        Some(ProviderQuotaState::Unlimited)
    );
    assert_eq!(min_meter_fill(&view), None);
}

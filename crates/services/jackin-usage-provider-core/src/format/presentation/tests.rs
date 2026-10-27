// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{
    CanonicalRemainingPercent, CanonicalUsedPercent, CountQuota, CountQuotaPeriod,
    CountQuotaProvenance, CountQuotaUnit, FocusedAccountHeader, FocusedUsageView,
    ProviderQuotaState, QuotaBucketView, StatusSlot, UsageConfidence, UsageMeterSide,
    UsageSeverity, UsageSnapshotStatus, UsageSource,
};
use jackin_protocol::usage_broker::UsageAccountCapability;

fn count_quota(
    used: Option<u64>,
    limit: Option<u64>,
    remaining: Option<u64>,
    unit: CountQuotaUnit,
    period: CountQuotaPeriod,
) -> CountQuota {
    CountQuota::new(
        used,
        limit,
        remaining,
        unit,
        period,
        CountQuotaProvenance::ProviderReported,
    )
}

fn count_bucket(label: &str, count_quota: Option<CountQuota>) -> QuotaBucketView {
    QuotaBucketView {
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota,
        provider_quota_state: None,
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Daily),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

fn focused_view(buckets: Vec<QuotaBucketView>) -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: Some("openrouter".to_owned()),
        focused_provider: Some("openrouter".to_owned()),
        account: FocusedAccountHeader {
            provider_label: "OpenRouter".to_owned(),
            account_label: "Count fixture".to_owned(),
            account_capability: Some(UsageAccountCapability {
                account_id: "count_fixture".to_owned(),
                surface_id: "openrouter".to_owned(),
            }),
            username: None,
            plan_label: None,
            credential_origin: None,
        },
        buckets,
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_798_000_000,
        updated_label: "Updated just now".to_owned(),
        status_bar_label: "1 requests left".to_owned(),
        tabs: Vec::new(),
        last_error: None,
    }
}

#[test]
fn public_detail_lists_independent_count_observations_and_keeps_compact_summary() {
    let mut preserved = count_bucket("provider state", None);
    preserved.provider_quota_state = Some(ProviderQuotaState::Exhausted);
    preserved.used_percent = CanonicalUsedPercent::new(25.0).ok();
    preserved.used_percent_origin =
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported);
    preserved.remaining_percent = CanonicalRemainingPercent::new(75.0).ok();
    preserved.remaining_percent_origin =
        Some(jackin_protocol::control::PercentObservationOrigin::Derived);

    let observed = count_bucket(
        "observed",
        Some(count_quota(
            Some(9),
            Some(10),
            Some(1),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
        )),
    );
    let nonadditive = count_bucket(
        "nonadditive",
        Some(count_quota(
            Some(2),
            Some(10),
            Some(7),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
        )),
    );
    let view = focused_view(vec![preserved, observed.clone(), nonadditive]);
    let detail = usage_detail_presentation(&view);

    assert_eq!(detail.rows.len(), 3);
    assert_eq!(detail.rows[0].row_id, "bucket:0");
    assert_eq!(
        detail.rows[0].display_label,
        "Exhausted · Reported used: 25% · Derived remaining: 75%"
    );
    assert_eq!(
        detail.rows[0]
            .meter_percent
            .map(jackin_protocol::control::MeterPercentPoints::get),
        Some(0.0)
    );
    assert_eq!(detail.rows[0].meter_side, Some(UsageMeterSide::Remaining));
    assert_eq!(detail.rows[1].row_id, "bucket:1");
    assert_eq!(detail.rows[2].row_id, "bucket:2");

    assert_eq!(
        usage_bucket_presentation(&observed).display_label,
        "1 requests left"
    );

    assert_eq!(
        detail.rows[1].display_label,
        "Reported count (requests/utc_daily): used 9, limit 10, remaining 1"
    );
    assert_eq!(
        detail.rows[2].display_label,
        "Reported count (requests/utc_daily): used 2, limit 10, remaining 7"
    );
}

#[test]
fn public_detail_distinguishes_zero_from_each_missing_count_observation() {
    let cases = [
        (
            count_quota(
                Some(0),
                None,
                None,
                CountQuotaUnit::Requests,
                CountQuotaPeriod::UtcDaily,
            ),
            "Reported count (requests/utc_daily): used 0, limit unknown, remaining unknown",
        ),
        (
            count_quota(
                None,
                Some(0),
                None,
                CountQuotaUnit::Requests,
                CountQuotaPeriod::UtcDaily,
            ),
            "Reported count (requests/utc_daily): used unknown, limit 0, remaining unknown",
        ),
        (
            count_quota(
                None,
                None,
                Some(0),
                CountQuotaUnit::Requests,
                CountQuotaPeriod::UtcDaily,
            ),
            "Reported count (requests/utc_daily): used unknown, limit unknown, remaining 0",
        ),
        (
            count_quota(
                None,
                None,
                None,
                CountQuotaUnit::Requests,
                CountQuotaPeriod::UtcDaily,
            ),
            "Reported count (requests/utc_daily): used unknown, limit unknown, remaining unknown",
        ),
    ];
    let detail = usage_detail_presentation(&focused_view(
        cases
            .iter()
            .enumerate()
            .map(|(index, (count, _))| count_bucket(&format!("zero-{index}"), Some(*count)))
            .collect(),
    ));

    assert_eq!(detail.rows.len(), cases.len());
    for (index, (row, (_, expected))) in detail.rows.iter().zip(cases).enumerate() {
        assert_eq!(row.row_id, format!("bucket:{index}"));
        assert_eq!(row.display_label, expected);
    }
}

#[test]
fn public_detail_keeps_unknown_count_vocabulary_and_absent_quota_control() {
    let unknown = count_bucket(
        "unknown-vocabulary",
        Some(count_quota(
            Some(9),
            Some(10),
            Some(7),
            CountQuotaUnit::Unknown,
            CountQuotaPeriod::Unknown,
        )),
    );
    let no_count = count_bucket("no-count", None);
    let detail = usage_detail_presentation(&focused_view(vec![unknown, no_count]));

    assert_eq!(detail.rows.len(), 2);
    assert_eq!(detail.rows[0].row_id, "bucket:0");
    assert_eq!(detail.rows[1].row_id, "bucket:1");
    assert_eq!(detail.rows[1].display_label, "fresh");
    assert_eq!(
        detail.rows[0].display_label,
        "Reported count (unknown/unknown): used 9, limit 10, remaining 7"
    );
}

#[test]
fn public_detail_preserves_count_meter_and_existing_bucket_decoration() {
    let mut bucket = count_bucket(
        "decorated",
        Some(count_quota(
            Some(9),
            Some(10),
            Some(1),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
        )),
    );
    bucket.pace_label = Some("On pace".to_owned());
    bucket.reset_label = Some("Reset at fixture boundary".to_owned());
    bucket.status = UsageSnapshotStatus::Stale;
    bucket.severity = UsageSeverity::Warn;
    let detail = usage_detail_presentation(&focused_view(vec![bucket]));
    let row = &detail.rows[0];

    assert_eq!(detail.rows.len(), 1);
    assert_eq!(row.row_id, "bucket:0");
    assert_eq!(
        row.meter_percent
            .map(jackin_protocol::control::MeterPercentPoints::get),
        Some(10.0)
    );
    assert_eq!(row.meter_side, Some(UsageMeterSide::Remaining));
    assert_eq!(row.severity, UsageSeverity::Warn);
    assert_eq!(
        row.layout_lines[2].trailing.as_deref(),
        Some("Reset at fixture boundary")
    );
    assert_eq!(
        row.display_label,
        "Reported count (requests/utc_daily): used 9, limit 10, remaining 1 · On pace · Reset at fixture boundary · stale"
    );
}

#[test]
fn public_detail_preserves_count_state_origins_meter_and_reset() {
    let mut bucket = count_bucket(
        "count state",
        Some(count_quota(
            Some(2),
            Some(10),
            Some(7),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
        )),
    );
    bucket.provider_quota_state = Some(ProviderQuotaState::Exhausted);
    bucket.used_percent = CanonicalUsedPercent::new(25.0).ok();
    bucket.used_percent_origin =
        Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported);
    bucket.remaining_percent = CanonicalRemainingPercent::new(75.0).ok();
    bucket.remaining_percent_origin =
        Some(jackin_protocol::control::PercentObservationOrigin::Derived);
    bucket.pace_label = Some("On pace".to_owned());
    bucket.reset_label = Some("Reset at fixture boundary".to_owned());
    bucket.status = UsageSnapshotStatus::Stale;
    bucket.severity = UsageSeverity::Warn;

    let detail = usage_detail_presentation(&focused_view(vec![bucket.clone()]));
    let row = &detail.rows[0];

    assert_eq!(detail.rows.len(), 1);
    assert_eq!(row.row_id, "bucket:0");
    assert_eq!(row.label, "count state");
    assert_eq!(
        row.meter_percent
            .map(jackin_protocol::control::MeterPercentPoints::get),
        Some(0.0)
    );
    assert_eq!(row.meter_side, Some(UsageMeterSide::Remaining));
    assert_eq!(row.severity, UsageSeverity::Warn);
    assert_eq!(
        row.layout_lines[5].trailing.as_deref(),
        Some("Reset at fixture boundary")
    );
    assert_eq!(
        usage_bucket_presentation(&bucket).display_label,
        "Exhausted · Reported used: 25% · Derived remaining: 75% · 7 requests left · On pace · Reset at fixture boundary · stale"
    );
    assert_eq!(
        row.display_label,
        "Exhausted · Reported used: 25% · Derived remaining: 75% · Reported count (requests/utc_daily): used 2, limit 10, remaining 7 · On pace · Reset at fixture boundary · stale"
    );
}

#[test]
fn public_detail_preserves_unlimited_count_without_meter() {
    let mut bucket = count_bucket(
        "unlimited with count",
        Some(count_quota(
            Some(2),
            Some(10),
            Some(7),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
        )),
    );
    bucket.provider_quota_state = Some(ProviderQuotaState::Unlimited);

    let detail = usage_detail_presentation(&focused_view(vec![bucket.clone()]));
    let row = &detail.rows[0];

    assert_eq!(detail.rows.len(), 1);
    assert_eq!(row.row_id, "bucket:0");
    assert_eq!(row.label, "unlimited with count");
    assert_eq!(row.meter_percent, None);
    assert_eq!(row.meter_side, None);
    assert_eq!(
        usage_bucket_presentation(&bucket).display_label,
        "Unlimited · 7 requests left"
    );
    assert_eq!(
        row.display_label,
        "Unlimited · Reported count (requests/utc_daily): used 2, limit 10, remaining 7"
    );
}

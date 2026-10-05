// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn count_quota_dto_preserves_unsigned_boundaries_and_unknowns() {
    for value in [
        None,
        Some(0),
        Some(1),
        Some((1_u64 << 53) + 1),
        Some(u64::MAX),
    ] {
        for period in [CountQuotaPeriod::UtcDaily, CountQuotaPeriod::Unknown] {
            let dto = count_quota_dto(CountQuota {
                used: value,
                limit: value,
                remaining: value,
                unit: CountQuotaUnit::Requests,
                period,
                provenance: CountQuotaProvenance::ProviderReported,
            });
            assert_eq!((dto.used, dto.limit, dto.remaining), (value, value, value));
            assert_eq!(dto.unit, "requests");
            assert_eq!(
                dto.period,
                if period == CountQuotaPeriod::UtcDaily {
                    "utc_daily"
                } else {
                    "unknown"
                }
            );
            assert_eq!(dto.provenance, "provider_reported");
        }
    }
}

#[test]
fn count_quota_dto_preserves_independently_missing_fields() {
    let dto = count_quota_dto(CountQuota {
        used: Some(0),
        limit: Some(u64::MAX),
        remaining: None,
        unit: CountQuotaUnit::Requests,
        period: CountQuotaPeriod::Unknown,
        provenance: CountQuotaProvenance::ProviderReported,
    });
    assert_eq!(dto.used, Some(0));
    assert_eq!(dto.limit, Some(u64::MAX));
    assert_eq!(dto.remaining, None);
}

#[test]
fn bucket_dto_keeps_raw_counts_and_rust_owned_literal_presentation() {
    let quota = CountQuota {
        used: Some((1_u64 << 53) + 1),
        limit: Some(u64::MAX),
        remaining: Some(2),
        unit: CountQuotaUnit::Requests,
        period: CountQuotaPeriod::Unknown,
        provenance: CountQuotaProvenance::ProviderReported,
    };
    let dto = bucket_dto(QuotaBucketView {
        label: "Credits".to_owned(),
        used_label: Some("$1.00".to_owned()),
        limit_label: Some("9000 tokens".to_owned()),
        remaining_percent: None,
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        remaining_money: None,
        count_quota: Some(quota),
        severity: jackin_protocol::control::UsageSeverity::Normal,
    });
    let counts = dto
        .count_quota
        .expect("typed quota must survive the FFI boundary");
    assert_eq!(counts.used, Some(9_007_199_254_740_993));
    assert_eq!(counts.limit, Some(u64::MAX));
    assert_eq!(counts.remaining, Some(2));
    assert_eq!(
        dto.display_segments,
        [
            "9007199254740993 / 18446744073709551615 requests used",
            "2 requests left",
        ]
    );
    assert_eq!(dto.resets_at, None);
    assert!(dto.used_money.is_none());
    assert!(dto.limit_money.is_none());
}

#[test]
fn account_dto_keeps_independent_counts_and_reset_epoch() {
    let dto = account_dto(HostAccountDescriptor {
        surface_id: "openrouter".to_owned(),
        provider_column_label: String::new(),
        account_key: "count-account".to_owned(),
        account_label: "count-account".to_owned(),
        plan_label: None,
        selected: true,
        lifecycle: "current".to_owned(),
        lifecycle_label: "Current".to_owned(),
        provenance: Vec::new(),
        provenance_label: "Provider reported".to_owned(),
        plan_or_status_label: "Ready".to_owned(),
        remaining_percent: None,
        remaining_label: "9007199254740993 requests left".to_owned(),
        headline: "9007199254740993 requests left".to_owned(),
        reset_label: None,
        reset_display_label: "—".to_owned(),
        exact_reset: None,
        status_word: "fresh".to_owned(),
        status_label: "Ready".to_owned(),
        severity: "normal".to_owned(),
        updated_label: "Updated now".to_owned(),
        last_error: None,
        dimmed: false,
        accessibility_label: "count-account".to_owned(),
        count_quota: Some(CountQuota {
            used: None,
            limit: Some(u64::MAX),
            remaining: Some(9_007_199_254_740_993),
            unit: CountQuotaUnit::Requests,
            period: CountQuotaPeriod::Unknown,
            provenance: CountQuotaProvenance::ProviderReported,
        }),
        resets_at: Some(0),
        used_money: None,
        limit_money: None,
        remaining_money: None,
    });
    let counts = dto.count_quota.expect("summary counts survive FFI");
    assert_eq!(counts.used, None);
    assert_eq!(counts.limit, Some(u64::MAX));
    assert_eq!(counts.remaining, Some(9_007_199_254_740_993));
    assert_eq!(dto.resets_at, Some(0));
    assert_eq!(dto.remaining_label, "9007199254740993 requests left");
}

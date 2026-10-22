// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

#[test]
fn count_quota_preserves_independent_unsigned_values_through_json() {
    let values = [
        None,
        Some(0),
        Some(1),
        Some((1_u64 << 53) + 1),
        Some(u64::MAX),
    ];
    for used in values {
        for limit in values {
            for remaining in values {
                let quota = count_quota(
                    used,
                    limit,
                    remaining,
                    CountQuotaUnit::Unknown,
                    CountQuotaPeriod::Unknown,
                );
                let encoded = serde_json::to_string(&quota).expect("quota serializes");
                let decoded: CountQuota =
                    serde_json::from_str(&encoded).expect("quota fields deserialize exactly");
                assert_eq!(decoded, quota);
                assert!(encoded.contains("\"unit\":\"unknown\""));
                assert!(encoded.contains("\"period\":\"unknown\""));
                assert!(encoded.contains("\"provenance\":\"provider_reported\""));
            }
        }
    }

    let raw_json = r#"{"used":9007199254740993,"limit":18446744073709551615,"remaining":0,"unit":"requests","period":"utc_daily","provenance":"provider_reported"}"#;
    let decoded: CountQuota = serde_json::from_str(raw_json).expect("u64 JSON integers parse");
    assert_eq!(decoded.used, Some((1_u64 << 53) + 1));
    assert_eq!(decoded.limit, Some(u64::MAX));
    assert_eq!(decoded.remaining, Some(0));
    assert_eq!(decoded.unit.machine_unit(), Some("requests"));
}

#[test]
fn count_quota_rejects_signed_fractional_and_out_of_range_values() {
    for (field, invalid) in [
        ("used", "-1"),
        ("limit", "1.5"),
        ("remaining", "18446744073709551616"),
    ] {
        let mut value = serde_json::json!({
            "used": null,
            "limit": null,
            "remaining": null,
            "unit": "unknown",
            "period": "unknown",
            "provenance": "provider_reported",
        });
        value[field] = serde_json::from_str(invalid).expect("invalid input is a JSON number");
        let json = value.to_string();
        assert!(
            serde_json::from_str::<CountQuota>(&json).is_err(),
            "{field}={invalid}"
        );
    }
}

#[test]
fn percentage_origin_and_meter_side_have_stable_snake_case_labels() {
    for (origin, label) in [
        (
            PercentObservationOrigin::ProviderReported,
            "provider_reported",
        ),
        (PercentObservationOrigin::Derived, "derived"),
        (
            PercentObservationOrigin::HistoricalUnknown,
            "historical_unknown",
        ),
    ] {
        let encoded = serde_json::to_value(origin).expect("origin serializes");
        assert_eq!(encoded, label);
        assert_eq!(origin.wire_label(), label);
        assert_eq!(
            serde_json::from_value::<PercentObservationOrigin>(encoded)
                .expect("origin label deserializes"),
            origin
        );
    }

    for (side, label) in [
        (UsageMeterSide::Used, "used"),
        (UsageMeterSide::Remaining, "remaining"),
    ] {
        let encoded = serde_json::to_value(side).expect("meter side serializes");
        assert_eq!(encoded, label);
        assert_eq!(side.wire_label(), label);
        assert_eq!(
            serde_json::from_value::<UsageMeterSide>(encoded)
                .expect("meter-side label deserializes"),
            side
        );
    }
}

#[test]
fn count_geometry_requires_verified_requests_and_preserves_reported_unknown_percent() {
    let reported = Some(CanonicalRemainingPercent::new(57.0).expect("valid percent"));
    let unknown = count_quota(
        Some(2),
        Some(10),
        Some(1),
        CountQuotaUnit::Unknown,
        CountQuotaPeriod::Unknown,
    );
    assert_eq!(unknown.remaining_meter_percent(), None);
    assert_eq!(
        unknown
            .display_remaining_meter_percent(reported)
            .map(MeterPercentPoints::get),
        Some(57.0)
    );
    assert_eq!(unknown.display_remaining_meter_percent(None), None);

    let requests = count_quota(
        Some(2),
        Some(10),
        Some(1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
    );
    assert_eq!(
        requests
            .display_remaining_meter_percent(reported)
            .map(MeterPercentPoints::get),
        Some(10.0),
        "verified exact counts take precedence over reported geometry"
    );

    for (limit, remaining) in [(Some(0), Some(1)), (None, Some(1)), (Some(1), None)] {
        let incomplete_requests = count_quota(
            None,
            limit,
            remaining,
            CountQuotaUnit::Requests,
            CountQuotaPeriod::Unknown,
        );
        assert_eq!(
            incomplete_requests.display_remaining_meter_percent(reported),
            None,
            "reported geometry must not become a fallback for verified request counts"
        );
    }
}

#[test]
fn request_geometry_uses_u128_for_exact_large_count_arithmetic() {
    for (remaining, limit, expected) in [
        (0, 1, 0.0),
        (1, 3, 33.0),
        ((u64::MAX / 2) + 1, u64::MAX, 50.0),
        (u64::MAX - 1, u64::MAX, 99.0),
        (u64::MAX, u64::MAX - 1, 100.0),
        (u64::MAX, 1, 100.0),
    ] {
        let quota = count_quota(
            None,
            Some(limit),
            Some(remaining),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::Unknown,
        );
        assert_eq!(
            quota.remaining_meter_percent().map(MeterPercentPoints::get),
            Some(expected),
            "remaining={remaining}, limit={limit}"
        );
    }

    for quota in [
        count_quota(
            None,
            Some(0),
            Some(1),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
        ),
        count_quota(
            None,
            None,
            Some(1),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::Unknown,
        ),
        count_quota(
            None,
            Some(1),
            None,
            CountQuotaUnit::Requests,
            CountQuotaPeriod::Unknown,
        ),
        count_quota(
            None,
            Some(1),
            Some(1),
            CountQuotaUnit::Unknown,
            CountQuotaPeriod::UtcDaily,
        ),
    ] {
        assert_eq!(quota.remaining_meter_percent(), None);
    }
}

#[test]
fn exact_remaining_alone_establishes_exhaustion() {
    for (unit, period) in [
        (CountQuotaUnit::Requests, CountQuotaPeriod::UtcDaily),
        (CountQuotaUnit::Unknown, CountQuotaPeriod::Unknown),
    ] {
        for (remaining, expected) in [
            (None, None),
            (Some(0), Some(true)),
            (Some(u64::MAX), Some(false)),
        ] {
            let quota = count_quota(None, None, remaining, unit, period);
            assert_eq!(quota.is_exhausted(), expected);
        }
    }
}

#[test]
fn source_percentages_preserve_raw_points_and_round_only_for_labels() {
    for (value, expected_label) in [
        (0.0, "0% used"),
        (-0.0, "0% used"),
        (0.375, "0.38% used"),
        (1.0, "1% used"),
        (43.375, "43.38% used"),
        (100.0, "100% used"),
        (100.125, "100.13% used"),
        (150.0, "150% used"),
    ] {
        let percent = CanonicalUsedPercent::new(value).expect("finite non-negative source points");
        assert_eq!(percent.label(), expected_label);
        let expected_raw = if value == 0.0 { 0.0 } else { value };
        assert_eq!(percent.get(), expected_raw);
        if value == -0.0 {
            assert_eq!(percent.get().to_bits(), 0.0_f64.to_bits());
        }
    }

    let raw = CanonicalUsedPercent::new(43.375).expect("valid percentage");
    assert_eq!(raw.get(), 43.375);
    assert_eq!(raw.meter_geometry().get(), 43.375);
    assert_eq!(raw.meter_fill(), 0.43375);
    assert_eq!(
        CanonicalUsedPercent::new(150.0)
            .unwrap()
            .meter_geometry()
            .get(),
        100.0
    );
    assert_eq!(CanonicalUsedPercent::new(150.0).unwrap().get(), 150.0);
    assert_eq!(
        CanonicalUsedPercent::new(150.0).unwrap().remaining_points(),
        0.0
    );

    let remaining = CanonicalRemainingPercent::new(100.125).expect("overage remains raw");
    assert_eq!(remaining.get(), 100.125);
    assert_eq!(remaining.meter_geometry().get(), 100.0);
    assert_eq!(remaining.used_points(), 0.0);
    assert_eq!(remaining.label(), "100.13% left");
}

#[test]
fn canonical_source_percentages_reject_negative_and_non_finite_values() {
    for value in [-0.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            CanonicalUsedPercent::new(value).expect_err("invalid used percent must be rejected"),
            "used percentage points must be finite and non-negative"
        );
        assert_eq!(
            CanonicalRemainingPercent::new(value)
                .expect_err("invalid remaining percent must be rejected"),
            "remaining percentage points must be finite and non-negative"
        );
    }

    let wire = serde_json::to_string(&CanonicalUsedPercent::new(0.375).unwrap())
        .expect("finite source percent serializes");
    assert_eq!(wire, "0.375");
    assert_eq!(
        serde_json::from_str::<CanonicalUsedPercent>(&wire)
            .unwrap()
            .get(),
        0.375
    );
    for invalid in ["-0.5", "\"0.5\"", "1e999"] {
        serde_json::from_str::<CanonicalUsedPercent>(invalid)
            .expect_err("invalid JSON source percent must be rejected");
    }
}

#[test]
fn signed_raw_percentages_preserve_negative_money_derived_points() {
    let raw = RawPercentPoints::new(-43.375).expect("finite signed money ratio");
    assert_eq!(raw.get(), -43.375);
    assert_eq!(raw.display_value(), "-43.38");
    assert_eq!(raw.meter_geometry().get(), 0.0);
    let wire = serde_json::to_string(&raw).expect("signed raw percentage serializes");
    assert_eq!(wire, "-43.375");
    assert_eq!(
        serde_json::from_str::<RawPercentPoints>(&wire).unwrap(),
        raw
    );
    assert_eq!(
        RawPercentPoints::new(f64::NAN).expect_err("NaN raw points must be rejected"),
        "raw percentage points must be finite"
    );
    assert_eq!(
        RawPercentPoints::new(f64::INFINITY).expect_err("infinite raw points must be rejected"),
        "raw percentage points must be finite"
    );
}

#[test]
fn meter_geometry_is_finite_continuous_and_bounded_to_percentage_points() {
    for (value, expected) in [
        (0.0, 0.0),
        (-0.0, 0.0),
        (0.375, 0.375),
        (43.375, 43.375),
        (100.0, 100.0),
    ] {
        assert_eq!(MeterPercentPoints::new(value).unwrap().get(), expected);
    }
    for value in [-0.1, 100.0001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            MeterPercentPoints::new(value).expect_err("invalid meter geometry must be rejected"),
            "meter percentage points must be finite and within 0..=100"
        );
    }
    for (raw, expected) in [
        (-1.0, 0.0),
        (43.375, 43.375),
        (100.125, 100.0),
        (150.0, 100.0),
    ] {
        assert_eq!(
            MeterPercentPoints::from_raw(RawPercentPoints::new(raw).unwrap()).get(),
            expected
        );
    }
    let geometry = MeterPercentPoints::new(43.375).unwrap();
    assert_eq!(geometry.fill(), 0.43375);
    assert_eq!(
        serde_json::from_str::<MeterPercentPoints>(&serde_json::to_string(&geometry).unwrap())
            .unwrap(),
        geometry
    );
}

#[test]
fn meter_complement_preserves_exact_normalized_geometry() {
    for value in [
        0.0,
        100.0,
        57.125,
        f64::from_bits(1),
        f64::from_bits(100.0_f64.to_bits() - 1),
    ] {
        let meter = MeterPercentPoints::new(value).expect("valid meter geometry");
        let expected = MeterPercentPoints::new(100.0 - value).expect("valid complement");
        assert_eq!(
            meter.complement().get().to_bits(),
            expected.get().to_bits(),
            "complement should match normalized 100 - {value:?}"
        );
    }
}

#[test]
fn canonical_source_percentages_convert_to_raw_points_without_precision_loss() {
    for value in [0.0, 57.125, 150.0, f64::MAX] {
        let used = CanonicalUsedPercent::new(value).expect("finite non-negative used value");
        let raw_used = RawPercentPoints::from(used);
        assert_eq!(raw_used.get().to_bits(), value.to_bits());

        let remaining =
            CanonicalRemainingPercent::new(value).expect("finite non-negative remaining value");
        let raw_remaining = RawPercentPoints::from(remaining);
        assert_eq!(raw_remaining.get().to_bits(), value.to_bits());
    }
}

#[test]
fn zero_meter_constant_is_validated_geometry_and_round_trips() {
    assert_eq!(MeterPercentPoints::ZERO.get(), 0.0);
    let encoded = serde_json::to_string(&MeterPercentPoints::ZERO).expect("zero serializes");
    let decoded: MeterPercentPoints = serde_json::from_str(&encoded).expect("zero deserializes");
    assert_eq!(decoded, MeterPercentPoints::ZERO);
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Lossless quota observations and percentage-point domain types.
//!
//! These types describe source observations and meter geometry. They do not
//! change the fields of the current usage bucket, monitor, or broker contracts.

use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Verified source unit for an observed count.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CountQuotaUnit {
    /// The source explicitly reports request counts.
    Requests,
    /// The source reports a count without verifying its unit.
    Unknown,
}

impl CountQuotaUnit {
    /// Stable transport label. An unknown unit is never promoted to a label.
    #[must_use]
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::Requests => "requests",
            Self::Unknown => "unknown",
        }
    }

    /// Verified machine-readable unit, if the source established one.
    #[must_use]
    pub const fn machine_unit(self) -> Option<&'static str> {
        match self {
            Self::Requests => Some("requests"),
            Self::Unknown => None,
        }
    }
}

/// Calendar interpretation supplied for a discrete quota.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CountQuotaPeriod {
    /// A UTC daily allowance reported by the provider.
    UtcDaily,
    /// No calendar period is verified.
    Unknown,
}

impl CountQuotaPeriod {
    /// Stable transport label for the independently observed period.
    #[must_use]
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::UtcDaily => "utc_daily",
            Self::Unknown => "unknown",
        }
    }
}

/// Source authority for a count quota.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CountQuotaProvenance {
    /// The provider response supplied these observations.
    ProviderReported,
}

impl CountQuotaProvenance {
    /// Stable transport label for this observation authority.
    #[must_use]
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::ProviderReported => "provider_reported",
        }
    }
}

/// Exact discrete quota observations.
///
/// Each value is independent. A missing field stays unknown; no value or
/// percentage is inferred from another field, a reset timestamp, or a label.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct CountQuota {
    /// Provider-reported consumed count.
    pub used: Option<u64>,
    /// Provider-reported count limit.
    pub limit: Option<u64>,
    /// Provider-reported exact remaining count.
    pub remaining: Option<u64>,
    /// Independently verified count unit.
    pub unit: CountQuotaUnit,
    /// Independently verified calendar period.
    pub period: CountQuotaPeriod,
    /// Source authority for the observation.
    pub provenance: CountQuotaProvenance,
}

impl CountQuota {
    /// Create a quota while preserving all independently reported fields.
    #[must_use]
    pub const fn new(
        used: Option<u64>,
        limit: Option<u64>,
        remaining: Option<u64>,
        unit: CountQuotaUnit,
        period: CountQuotaPeriod,
        provenance: CountQuotaProvenance,
    ) -> Self {
        Self {
            used,
            limit,
            remaining,
            unit,
            period,
            provenance,
        }
    }

    /// Exact remaining meter geometry for a verified request quota.
    ///
    /// Integer multiplication and division use `u128` so every `u64` value is
    /// safe. The existing quota geometry policy floors whole percentage points
    /// and clamps only the meter at 100. An unknown unit, missing count, or zero
    /// limit has no count-derived geometry.
    #[must_use]
    pub fn remaining_meter_percent(&self) -> Option<MeterPercentPoints> {
        if self.unit != CountQuotaUnit::Requests {
            return None;
        }

        let limit = self.limit.filter(|limit| *limit > 0)?;
        let remaining = self.remaining?;
        let percent = (u128::from(remaining) * 100 / u128::from(limit)).min(100);
        let percent = u8::try_from(percent).ok()?;
        Some(MeterPercentPoints(f64::from(percent)))
    }

    /// Select source-preserving geometry for display.
    ///
    /// Verified requests derive geometry from exact `remaining` and `limit`.
    /// Unknown units retain only an independently reported remaining percent.
    /// This selects meter geometry only. Callers must retain an independently
    /// reported raw percentage separately, including when this method returns
    /// `None` for incomplete verified request counts; it must not be overwritten
    /// or discarded based on this result. Only selected meter geometry is
    /// clamped to `0..=100`.
    #[must_use]
    pub fn display_remaining_meter_percent(
        &self,
        reported: Option<CanonicalRemainingPercent>,
    ) -> Option<MeterPercentPoints> {
        match self.unit {
            CountQuotaUnit::Requests => self.remaining_meter_percent(),
            CountQuotaUnit::Unknown => reported.map(CanonicalRemainingPercent::meter_geometry),
        }
    }

    /// Exhaustion follows exact remaining only, regardless of unit or period.
    #[must_use]
    pub fn is_exhausted(&self) -> Option<bool> {
        self.remaining.map(|remaining| remaining == 0)
    }
}

/// Exact, finite, signed percentage points from a generic broker source.
///
/// This type accepts negative values so signed monetary ratios remain
/// representable. It does not clamp or round its stored value.
#[derive(Debug, Clone, Copy, PartialOrd)]
pub struct RawPercentPoints(f64);

impl RawPercentPoints {
    /// Validate finite signed percentage points.
    pub fn new(value: f64) -> Result<Self, String> {
        if !value.is_finite() {
            return Err("raw percentage points must be finite".to_owned());
        }
        Ok(Self(normalize_zero(value)))
    }

    /// Exact canonical percentage points.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// Clamped continuous meter geometry. The stored raw value is unchanged.
    #[must_use]
    pub fn meter_geometry(self) -> MeterPercentPoints {
        MeterPercentPoints(self.0.clamp(0.0, 100.0))
    }

    /// Text value rounded at the final presentation edge.
    #[must_use]
    pub fn display_value(self) -> String {
        format_percent_points(self.0)
    }
}

impl PartialEq for RawPercentPoints {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for RawPercentPoints {}

impl Serialize for RawPercentPoints {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(self.0)
    }
}

impl<'de> Deserialize<'de> for RawPercentPoints {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

/// Canonical provider-reported used percentage points.
///
/// Values are finite and non-negative. Overage above 100 remains raw, and
/// negative zero is normalized to positive zero.
#[derive(Debug, Clone, Copy, PartialOrd)]
pub struct CanonicalUsedPercent(f64);

impl CanonicalUsedPercent {
    /// Validate provider-reported used percentage points.
    pub fn new(value: f64) -> Result<Self, String> {
        if !value.is_finite() || value < 0.0 {
            return Err("used percentage points must be finite and non-negative".to_owned());
        }
        Ok(Self(normalize_zero(value)))
    }

    /// Exact canonical percentage points.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// Continuous used-side meter geometry, clamped to the visible range.
    #[must_use]
    pub fn meter_geometry(self) -> MeterPercentPoints {
        MeterPercentPoints(self.0.clamp(0.0, 100.0))
    }

    /// Continuous fill in the unit interval for renderers that use fractions.
    #[must_use]
    pub fn meter_fill(self) -> f64 {
        self.meter_geometry().get() / 100.0
    }

    /// Remaining percentage points derived from this used-side source.
    #[must_use]
    pub fn remaining_points(self) -> f64 {
        (100.0 - self.0).max(0.0)
    }

    /// Text value rounded at the final presentation edge.
    #[must_use]
    pub fn display_value(self) -> String {
        format_percent_points(self.0)
    }

    /// Display label. The raw source value remains unchanged in `get()`.
    #[must_use]
    pub fn label(self) -> String {
        format!("{}% used", self.display_value())
    }
}

impl PartialEq for CanonicalUsedPercent {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for CanonicalUsedPercent {}

impl Serialize for CanonicalUsedPercent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(self.0)
    }
}

impl<'de> Deserialize<'de> for CanonicalUsedPercent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

/// Canonical provider-reported remaining percentage points.
///
/// Values are finite and non-negative. Raw overage remains above 100; only
/// meter geometry is bounded.
#[derive(Debug, Clone, Copy, PartialOrd)]
pub struct CanonicalRemainingPercent(f64);

impl CanonicalRemainingPercent {
    /// Validate provider-reported remaining percentage points.
    pub fn new(value: f64) -> Result<Self, String> {
        if !value.is_finite() || value < 0.0 {
            return Err("remaining percentage points must be finite and non-negative".to_owned());
        }
        Ok(Self(normalize_zero(value)))
    }

    /// Exact canonical percentage points.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// Continuous remaining-side meter geometry, clamped to the visible range.
    #[must_use]
    pub fn meter_geometry(self) -> MeterPercentPoints {
        MeterPercentPoints(self.0.clamp(0.0, 100.0))
    }

    /// Continuous fill in the unit interval for renderers that use fractions.
    #[must_use]
    pub fn meter_fill(self) -> f64 {
        self.meter_geometry().get() / 100.0
    }

    /// Used percentage points derived from this remaining-side source.
    #[must_use]
    pub fn used_points(self) -> f64 {
        (100.0 - self.0).max(0.0)
    }

    /// Derived used-side percentage points, kept non-negative and finite.
    #[must_use]
    pub fn derived_used_percent(self) -> CanonicalUsedPercent {
        CanonicalUsedPercent(self.used_points())
    }

    /// Text value rounded at the final presentation edge.
    #[must_use]
    pub fn display_value(self) -> String {
        format_percent_points(self.0)
    }

    /// Display label. The raw source value remains unchanged in `get()`.
    #[must_use]
    pub fn label(self) -> String {
        format!("{}% left", self.display_value())
    }
}

impl PartialEq for CanonicalRemainingPercent {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for CanonicalRemainingPercent {}

impl Serialize for CanonicalRemainingPercent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(self.0)
    }
}

impl<'de> Deserialize<'de> for CanonicalRemainingPercent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

/// Validated continuous meter geometry in percentage points `0..=100`.
///
/// Source values use [`CanonicalUsedPercent`] or [`CanonicalRemainingPercent`]
/// and may exceed this range. Only this geometry value is clamped.
#[derive(Debug, Clone, Copy, PartialOrd)]
pub struct MeterPercentPoints(f64);

impl MeterPercentPoints {
    /// Validate continuous meter geometry.
    pub fn new(value: f64) -> Result<Self, String> {
        if !value.is_finite() || !(0.0..=100.0).contains(&value) {
            return Err("meter percentage points must be finite and within 0..=100".to_owned());
        }
        Ok(Self(normalize_zero(value)))
    }

    /// Continuous geometry clamped from a finite signed raw point value.
    #[must_use]
    pub fn from_raw(value: RawPercentPoints) -> Self {
        value.meter_geometry()
    }

    /// Exact continuous meter geometry in percentage points.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// Continuous fill in the unit interval for renderers that use fractions.
    #[must_use]
    pub fn fill(self) -> f64 {
        self.0 / 100.0
    }
}

impl PartialEq for MeterPercentPoints {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for MeterPercentPoints {}

impl Serialize for MeterPercentPoints {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(self.0)
    }
}

impl<'de> Deserialize<'de> for MeterPercentPoints {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

fn normalize_zero(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value }
}

/// Round percentage points to the nearest hundredth only when formatting text.
fn format_percent_points(value: f64) -> String {
    let rounded = if value.abs() <= f64::MAX / 100.0 {
        (value * 100.0).round() / 100.0
    } else {
        value
    };
    rounded.to_string()
}

#[cfg(test)]
mod tests {
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
            let percent =
                CanonicalUsedPercent::new(value).expect("finite non-negative source points");
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
            assert!(CanonicalUsedPercent::new(value).is_err());
            assert!(CanonicalRemainingPercent::new(value).is_err());
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
            assert!(serde_json::from_str::<CanonicalUsedPercent>(invalid).is_err());
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
        assert!(RawPercentPoints::new(f64::NAN).is_err());
        assert!(RawPercentPoints::new(f64::INFINITY).is_err());
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
            assert!(MeterPercentPoints::new(value).is_err());
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
}

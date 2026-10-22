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

/// Authority for an independently observed percentage value.
///
/// `Derived` marks a locally calculated value. `HistoricalUnknown` is reserved
/// for private migrations that cannot prove whether an old value was reported
/// or calculated. Live writers must choose one of the first two explicitly.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PercentObservationOrigin {
    /// The upstream source explicitly reported this side.
    ProviderReported,
    /// A local calculation produced this side.
    Derived,
    /// A private migration retained a value with unknown original authority.
    HistoricalUnknown,
}

impl PercentObservationOrigin {
    /// Stable transport label for this observation authority.
    #[must_use]
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::ProviderReported => "provider_reported",
            Self::Derived => "derived",
            Self::HistoricalUnknown => "historical_unknown",
        }
    }
}

/// An affirmative provider-reported quota regime.
///
/// Absence means that the provider did not establish an explicit exhausted or
/// unlimited claim. This source observation is independent of any reported
/// counts or percentages.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderQuotaState {
    /// The provider explicitly reports that the quota is exhausted.
    Exhausted,
    /// The provider explicitly reports that the quota is unlimited.
    Unlimited,
}

impl ProviderQuotaState {
    /// Stable transport label for this explicit provider claim.
    #[must_use]
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::Exhausted => "exhausted",
            Self::Unlimited => "unlimited",
        }
    }
}

/// Side represented by selected meter geometry.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageMeterSide {
    /// Geometry represents percentage used.
    Used,
    /// Geometry represents percentage remaining.
    Remaining,
}

impl UsageMeterSide {
    /// Stable transport label for the selected percentage side.
    #[must_use]
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::Used => "used",
            Self::Remaining => "remaining",
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

impl From<CanonicalUsedPercent> for RawPercentPoints {
    fn from(value: CanonicalUsedPercent) -> Self {
        Self(value.get())
    }
}

impl From<CanonicalRemainingPercent> for RawPercentPoints {
    fn from(value: CanonicalRemainingPercent) -> Self {
        Self(value.get())
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
    /// Validated zero geometry for an explicitly exhausted quota.
    pub const ZERO: Self = Self(0.0);

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

    /// Reorient geometry to the opposite side of a 100-point meter.
    ///
    /// This transforms validated meter geometry only; it does not create or
    /// change a provider-reported quota observation.
    #[must_use]
    pub fn complement(self) -> Self {
        Self(normalize_zero(100.0 - self.0))
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
mod tests;

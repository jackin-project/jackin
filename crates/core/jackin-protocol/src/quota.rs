// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CountQuotaUnit {
    Requests,

    Unknown,
}

impl CountQuotaUnit {
    #[must_use]
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::Requests => "requests",
            Self::Unknown => "unknown",
        }
    }

    #[must_use]
    pub const fn machine_unit(self) -> Option<&'static str> {
        match self {
            Self::Requests => Some("requests"),
            Self::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CountQuotaPeriod {
    UtcDaily,

    Unknown,
}

impl CountQuotaPeriod {
    #[must_use]
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::UtcDaily => "utc_daily",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CountQuotaProvenance {
    ProviderReported,
}

impl CountQuotaProvenance {
    #[must_use]
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::ProviderReported => "provider_reported",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PercentObservationOrigin {
    ProviderReported,

    Derived,

    HistoricalUnknown,
}

impl PercentObservationOrigin {
    #[must_use]
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::ProviderReported => "provider_reported",
            Self::Derived => "derived",
            Self::HistoricalUnknown => "historical_unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderQuotaState {
    Exhausted,

    Unlimited,
}

impl ProviderQuotaState {
    #[must_use]
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::Exhausted => "exhausted",
            Self::Unlimited => "unlimited",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageMeterSide {
    Used,

    Remaining,
}

impl UsageMeterSide {
    #[must_use]
    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::Used => "used",
            Self::Remaining => "remaining",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct CountQuota {
    pub used: Option<u64>,

    pub limit: Option<u64>,

    pub remaining: Option<u64>,

    pub unit: CountQuotaUnit,

    pub period: CountQuotaPeriod,

    pub provenance: CountQuotaProvenance,
}

impl CountQuota {
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

    #[must_use]
    pub fn is_exhausted(&self) -> Option<bool> {
        self.remaining.map(|remaining| remaining == 0)
    }
}

#[derive(Debug, Clone, Copy, PartialOrd)]
pub struct RawPercentPoints(f64);

impl RawPercentPoints {
    pub fn new(value: f64) -> Result<Self, String> {
        if !value.is_finite() {
            return Err("raw percentage points must be finite".to_owned());
        }
        Ok(Self(normalize_zero(value)))
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    #[must_use]
    pub fn meter_geometry(self) -> MeterPercentPoints {
        MeterPercentPoints(self.0.clamp(0.0, 100.0))
    }

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

#[derive(Debug, Clone, Copy, PartialOrd)]
pub struct CanonicalUsedPercent(f64);

impl CanonicalUsedPercent {
    pub fn new(value: f64) -> Result<Self, String> {
        if !value.is_finite() || value < 0.0 {
            return Err("used percentage points must be finite and non-negative".to_owned());
        }
        Ok(Self(normalize_zero(value)))
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    #[must_use]
    pub fn meter_geometry(self) -> MeterPercentPoints {
        MeterPercentPoints(self.0.clamp(0.0, 100.0))
    }

    #[must_use]
    pub fn meter_fill(self) -> f64 {
        self.meter_geometry().get() / 100.0
    }

    #[must_use]
    pub fn remaining_points(self) -> f64 {
        (100.0 - self.0).max(0.0)
    }

    #[must_use]
    pub fn display_value(self) -> String {
        format_percent_points(self.0)
    }

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

#[derive(Debug, Clone, Copy, PartialOrd)]
pub struct CanonicalRemainingPercent(f64);

impl CanonicalRemainingPercent {
    pub fn new(value: f64) -> Result<Self, String> {
        if !value.is_finite() || value < 0.0 {
            return Err("remaining percentage points must be finite and non-negative".to_owned());
        }
        Ok(Self(normalize_zero(value)))
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    #[must_use]
    pub fn meter_geometry(self) -> MeterPercentPoints {
        MeterPercentPoints(self.0.clamp(0.0, 100.0))
    }

    #[must_use]
    pub fn meter_fill(self) -> f64 {
        self.meter_geometry().get() / 100.0
    }

    #[must_use]
    pub fn used_points(self) -> f64 {
        (100.0 - self.0).max(0.0)
    }

    #[must_use]
    pub fn derived_used_percent(self) -> CanonicalUsedPercent {
        CanonicalUsedPercent(self.used_points())
    }

    #[must_use]
    pub fn display_value(self) -> String {
        format_percent_points(self.0)
    }

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

#[derive(Debug, Clone, Copy, PartialOrd)]
pub struct MeterPercentPoints(f64);

impl MeterPercentPoints {
    pub const ZERO: Self = Self(0.0);

    pub fn new(value: f64) -> Result<Self, String> {
        if !value.is_finite() || !(0.0..=100.0).contains(&value) {
            return Err("meter percentage points must be finite and within 0..=100".to_owned());
        }
        Ok(Self(normalize_zero(value)))
    }

    #[must_use]
    pub fn from_raw(value: RawPercentPoints) -> Self {
        value.meter_geometry()
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    #[must_use]
    pub fn complement(self) -> Self {
        Self(normalize_zero(100.0 - self.0))
    }

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

fn format_percent_points(value: f64) -> String {
    let rounded = if value.abs() <= f64::MAX / 100.0 {
        (value * 100.0).round() / 100.0
    } else {
        value
    };
    rounded.to_string()
}

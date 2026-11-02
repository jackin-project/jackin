// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

const FIVE_HOUR_SECONDS: i64 = 18_000;
const WEEK_SECONDS: i64 = 604_800;
const DAY_SECONDS: i64 = 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BucketKind {
    FiveHour,
    Weekly,
    Monthly,
    Credits,
    Spend,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Money {
    minor: i64,
    exponent: u32,
    unit: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Remaining {
    Percent(f64),
    Amount(Money),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bucket {
    pub kind: BucketKind,
    pub label: String,
    pub used_percent: Option<f64>,
    pub remaining: Option<Remaining>,
    pub resets_at: Option<i64>,
}

pub const MAX_EXPONENT: u32 = 9;
const MAX_MINOR: f64 = 9.0e15;

impl Money {
    pub fn new(minor: i64, exponent: u32, unit: impl Into<String>) -> Option<Money> {
        (exponent <= MAX_EXPONENT).then(|| Money {
            minor,
            exponent,
            unit: unit.into(),
        })
    }

    pub fn cents(minor: i64, unit: impl Into<String>) -> Money {
        Money {
            minor,
            exponent: 2,
            unit: unit.into(),
        }
    }

    pub fn from_f64(value: f64, exponent: u32, unit: impl Into<String>) -> Option<Money> {
        let scaled = (value * 10_f64.powi(i32::try_from(exponent).ok()?)).round();
        Money::new(minor_units(scaled)?, exponent, unit)
    }

    pub fn minor(&self) -> i64 {
        self.minor
    }

    pub fn unit(&self) -> &str {
        &self.unit
    }

    pub fn magnitude(&self) -> Option<Money> {
        Money::new(self.minor.checked_abs()?, self.exponent, self.unit.clone())
    }

    pub fn minus(&self, other: &Money) -> Option<Money> {
        if self.exponent != other.exponent {
            return None;
        }
        Money::new(
            self.minor.checked_sub(other.minor)?,
            self.exponent,
            self.unit.clone(),
        )
    }

    #[must_use]
    pub fn clamped_at_zero(&self) -> Money {
        Money {
            minor: self.minor.max(0),
            exponent: self.exponent,
            unit: self.unit.clone(),
        }
    }

    pub fn text(&self) -> String {
        if self.exponent == 0 {
            return self.minor.to_string();
        }
        let scale = 10_u64.pow(self.exponent);
        let sign = if self.minor < 0 { "-" } else { "" };
        let magnitude = self.minor.unsigned_abs();
        format!(
            "{sign}{}.{:0width$}",
            magnitude / scale,
            magnitude % scale,
            width = self.exponent as usize
        )
    }
}

pub fn minor_units(value: f64) -> Option<i64> {
    (value.is_finite() && value.abs() <= MAX_MINOR).then_some(value as i64)
}

fn kind_from_seconds(seconds: i64) -> Option<BucketKind> {
    match seconds {
        FIVE_HOUR_SECONDS => Some(BucketKind::FiveHour),
        WEEK_SECONDS => Some(BucketKind::Weekly),
        s if (28 * DAY_SECONDS..=31 * DAY_SECONDS).contains(&s) => Some(BucketKind::Monthly),
        _ => None,
    }
}

fn seconds_label(seconds: i64) -> Option<String> {
    if seconds <= 0 {
        return None;
    }
    let (count, unit) = if seconds % WEEK_SECONDS == 0 {
        (seconds / WEEK_SECONDS, "week")
    } else if seconds % DAY_SECONDS == 0 {
        (seconds / DAY_SECONDS, "day")
    } else if seconds % 3_600 == 0 {
        (seconds / 3_600, "hour")
    } else if seconds % 60 == 0 {
        (seconds / 60, "minute")
    } else {
        (seconds, "second")
    };
    let plural = if count == 1 { "" } else { "s" };
    Some(format!("{count} {unit}{plural} window"))
}

fn kind_label(kind: BucketKind) -> Option<&'static str> {
    match kind {
        BucketKind::FiveHour => Some("5-hour"),
        BucketKind::Weekly => Some("weekly"),
        BucketKind::Monthly => Some("monthly"),
        BucketKind::Credits | BucketKind::Spend | BucketKind::Other => None,
    }
}

impl Bucket {
    pub fn named(kind: BucketKind, label: impl Into<String>) -> Bucket {
        Bucket {
            kind,
            label: label.into(),
            used_percent: None,
            remaining: None,
            resets_at: None,
        }
    }

    pub fn timed(
        prefix: Option<&str>,
        window_seconds: Option<i64>,
        slot: Option<BucketKind>,
        other_label: &str,
    ) -> Bucket {
        let by_length = window_seconds.and_then(kind_from_seconds);
        let kind = by_length.or(slot).unwrap_or(BucketKind::Other);
        let label = if let Some(fixed) = kind_label(kind) {
            prefixed(prefix, fixed)
        } else {
            let length_label = if by_length.is_none() {
                window_seconds.and_then(seconds_label)
            } else {
                None
            };
            length_label.map_or_else(|| other_label.to_owned(), |text| prefixed(prefix, &text))
        };
        Bucket::named(kind, label)
    }

    #[must_use]
    pub fn used(mut self, percent: Option<f64>) -> Bucket {
        self.used_percent = percent;
        self
    }

    #[must_use]
    pub fn left(mut self, remaining: Option<Remaining>) -> Bucket {
        self.remaining = remaining;
        self
    }

    #[must_use]
    pub fn resets(mut self, epoch: Option<i64>) -> Bucket {
        self.resets_at = epoch;
        self
    }
}

fn prefixed(prefix: Option<&str>, text: &str) -> String {
    match prefix {
        Some(prefix) if !prefix.is_empty() => format!("{prefix} {text}"),
        _ => text.to_owned(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Credential<'a> {
    Token(&'a str),
    Login(&'a str),
}

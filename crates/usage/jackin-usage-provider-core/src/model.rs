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

impl Money {
    pub fn new(minor: i64, exponent: u32, unit: impl Into<String>) -> Money {
        Money {
            minor,
            exponent,
            unit: unit.into(),
        }
    }

    pub fn from_f64(value: f64, exponent: u32, unit: impl Into<String>) -> Option<Money> {
        if !value.is_finite() {
            return None;
        }
        let scaled = (value * 10_f64.powi(i32::try_from(exponent).ok()?)).round();
        if scaled.abs() > 9.0e15 {
            return None;
        }
        Some(Money::new(scaled as i64, exponent, unit))
    }

    pub fn parse_decimal(text: &str, exponent: u32, unit: impl Into<String>) -> Option<Money> {
        let value: f64 = text.trim().parse().ok()?;
        Money::from_f64(value, exponent, unit)
    }

    pub fn minor(&self) -> i64 {
        self.minor
    }

    pub fn unit(&self) -> &str {
        &self.unit
    }

    pub fn exponent(&self) -> u32 {
        self.exponent
    }

    #[must_use]
    pub fn magnitude(&self) -> Money {
        Money::new(self.minor.abs(), self.exponent, self.unit.clone())
    }

    #[must_use]
    pub fn minus(&self, other: &Money) -> Money {
        Money::new(self.minor - other.minor, self.exponent, self.unit.clone())
    }

    #[must_use]
    pub fn clamped_at_zero(&self) -> Money {
        Money::new(self.minor.max(0), self.exponent, self.unit.clone())
    }

    pub fn text(&self) -> String {
        if self.exponent == 0 {
            return self.minor.to_string();
        }
        let scale = 10_i64.pow(self.exponent);
        let sign = if self.minor < 0 { "-" } else { "" };
        let magnitude = self.minor.unsigned_abs();
        let scale = scale.unsigned_abs();
        format!(
            "{sign}{}.{:0width$}",
            magnitude / scale,
            magnitude % scale,
            width = self.exponent as usize
        )
    }
}

pub fn kind_from_seconds(seconds: i64) -> Option<BucketKind> {
    match seconds {
        FIVE_HOUR_SECONDS => Some(BucketKind::FiveHour),
        WEEK_SECONDS => Some(BucketKind::Weekly),
        s if (28 * DAY_SECONDS..=31 * DAY_SECONDS).contains(&s) => Some(BucketKind::Monthly),
        _ => None,
    }
}

pub fn seconds_label(seconds: i64) -> Option<String> {
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

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{TerminalText, Timestamp};
use jackin_usage_provider_core::{Bucket, BucketKind, Remaining as BucketRemaining};

use crate::model::{LimitWindow, Percent, Remaining, UnavailableReason, WindowKind};

const NOT_FINITE: &str = "the provider reported a number that is not finite";
const OUT_OF_RANGE: &str = "a time in the response is out of range";
const LAST_EPOCH: i64 = 253_402_300_799;
const NO_WINDOWS: &str = "the provider reported no limit windows";

pub(crate) fn windows(buckets: Vec<Bucket>) -> Result<Vec<LimitWindow>, UnavailableReason> {
    let mut out = Vec::new();
    for bucket in buckets {
        if let Some(window) = window(bucket)? {
            out.push(window);
        }
    }
    if out.is_empty() {
        return Err(provider(NO_WINDOWS));
    }
    Ok(out)
}

fn window(bucket: Bucket) -> Result<Option<LimitWindow>, UnavailableReason> {
    let used = bucket.used_percent.map(percent).transpose()?;
    let remaining = match bucket.remaining {
        Some(BucketRemaining::Percent(value)) => Some(Remaining::Percent(percent(value)?)),
        Some(BucketRemaining::Amount(money)) => Some(Remaining::Amount {
            value: money.text(),
            unit: clean(money.unit()),
        }),
        None => match used {
            Some(used) => Some(Remaining::Percent(percent(100.0 - used.get())?)),
            None => None,
        },
    };
    let resets_at = bucket.resets_at.map(instant).transpose()?;
    if used.is_none() && remaining.is_none() && resets_at.is_none() {
        return Ok(None);
    }
    Ok(Some(LimitWindow {
        kind: kind(bucket.kind),
        label: clean(&bucket.label),
        used,
        remaining,
        resets_at,
    }))
}

fn clean(text: &str) -> String {
    TerminalText::sanitize(text.as_bytes()).to_string()
}

fn instant(epoch: i64) -> Result<Timestamp, UnavailableReason> {
    if (0..=LAST_EPOCH).contains(&epoch) {
        Ok(Timestamp(epoch))
    } else {
        Err(provider(OUT_OF_RANGE))
    }
}

fn percent(value: f64) -> Result<Percent, UnavailableReason> {
    if !value.is_finite() {
        return Err(provider(NOT_FINITE));
    }
    Percent::new(value.clamp(0.0, 100.0)).map_err(|_| provider(NOT_FINITE))
}

fn kind(kind: BucketKind) -> WindowKind {
    match kind {
        BucketKind::FiveHour => WindowKind::FiveHour,
        BucketKind::Weekly => WindowKind::Weekly,
        BucketKind::Monthly => WindowKind::Monthly,
        BucketKind::Credits => WindowKind::Credits,
        BucketKind::Spend => WindowKind::Spend,
        BucketKind::Other => WindowKind::Other,
    }
}

fn provider(message: &str) -> UnavailableReason {
    UnavailableReason::Provider(message.to_owned())
}

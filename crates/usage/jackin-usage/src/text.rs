// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use jackin_core::Timestamp;

use crate::model::{LimitWindow, Percent, Remaining, UnavailableReason};

const DAY: i64 = 86_400;
const NONE: &str = "-";

impl fmt::Display for Percent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = format!("{:.2}", (self.get() * 100.0).round() / 100.0);
        write!(f, "{}%", text.trim_end_matches('0').trim_end_matches('.'))
    }
}

impl fmt::Display for UnavailableReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnavailableReason::NoCredentials => f.write_str("no credentials"),
            UnavailableReason::ExpiredLogin => f.write_str("login expired or rejected"),
            UnavailableReason::Unsupported { detail: None } => {
                f.write_str("not supported for this provider")
            }
            UnavailableReason::Unsupported {
                detail: Some(detail),
            } => write!(f, "not supported: {detail}"),
            UnavailableReason::RateLimited { retry_at: None } => f.write_str("rate limited"),
            UnavailableReason::RateLimited { retry_at: Some(at) } => {
                write!(f, "rate limited, retry at {}", at.rfc3339())
            }
            UnavailableReason::Network(message) => write!(f, "network error: {message}"),
            UnavailableReason::Provider(message) => write!(f, "provider error: {message}"),
        }
    }
}

fn distance(seconds: i64) -> String {
    if seconds <= 0 {
        "passed".to_owned()
    } else if seconds < DAY {
        format!("in {}h{}m", seconds / 3_600, seconds % 3_600 / 60)
    } else {
        format!("in {}d{}h", seconds / DAY, seconds % DAY / 3_600)
    }
}

impl LimitWindow {
    pub fn used_text(&self) -> String {
        self.used
            .map_or_else(|| NONE.to_owned(), |used| format!("{used} used"))
    }

    pub fn remaining_text(&self) -> String {
        match &self.remaining {
            Some(Remaining::Percent(left)) => format!("{left} left"),
            Some(Remaining::Amount { value, unit }) => format!("{value} {unit} left"),
            None => NONE.to_owned(),
        }
    }

    pub fn spent_text(&self) -> String {
        match &self.spent {
            Some(Remaining::Percent(spent)) => format!("{spent} spent"),
            Some(Remaining::Amount { value, unit }) => format!("{value} {unit} spent"),
            None => NONE.to_owned(),
        }
    }

    pub fn reset_text(&self, now: Timestamp) -> String {
        self.resets_at.map_or_else(
            || NONE.to_owned(),
            |at| format!("resets {} ({})", at.rfc3339(), distance(at.0 - now.0)),
        )
    }
}

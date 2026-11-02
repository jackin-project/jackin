// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AccountName, HostPath, Provider, Timestamp, plain};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const SCHEMA: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageReport {
    pub schema: u32,
    pub generated_at: Timestamp,
    pub accounts: Vec<AccountUsage>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AccountUsage {
    pub account: AccountName,
    pub provider: Provider,
    pub freshness: Freshness,
    pub outcome: UsageOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Freshness {
    Fresh,
    Stale { since: Timestamp },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(into = "OutcomeRepr", from = "OutcomeRepr")]
pub enum UsageOutcome {
    Windows(Vec<LimitWindow>),
    Unavailable(UnavailableReason),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LimitWindow {
    pub kind: WindowKind,
    pub label: String,
    #[serde(rename = "used_percent")]
    pub used: Option<Percent>,
    pub remaining: Option<Remaining>,
    pub resets_at: Option<Timestamp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowKind {
    FiveHour,
    Weekly,
    Monthly,
    Credits,
    Spend,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Percent(f64);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(into = "RemainingRepr", from = "RemainingRepr")]
pub enum Remaining {
    Percent(Percent),
    Amount { value: String, unit: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "ReasonRepr", from = "ReasonRepr")]
pub enum UnavailableReason {
    NoCredentials,
    ExpiredLogin,
    Unsupported { detail: Option<String> },
    RateLimited { retry_at: Option<Timestamp> },
    Network(String),
    Provider(String),
}

#[derive(Debug, Error)]
pub enum UsageError {
    #[error("the usage request was cancelled")]
    Cancelled,
    #[error("{0} is not a percentage between 0 and 100")]
    InvalidPercent(f64),
    #[error("the usage cache at {path} failed: {}", plain(.message))]
    Store { path: HostPath, message: String },
    #[error("{}", plain(.0))]
    Internal(String),
}

impl Percent {
    pub fn new(value: f64) -> Result<Percent, UsageError> {
        if value.is_finite() && (0.0..=100.0).contains(&value) {
            Ok(Percent(value))
        } else {
            Err(UsageError::InvalidPercent(value))
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Percent {
    type Error = UsageError;

    fn try_from(value: f64) -> Result<Percent, UsageError> {
        Percent::new(value)
    }
}

impl From<Percent> for f64 {
    fn from(percent: Percent) -> f64 {
        percent.0
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum OutcomeRepr {
    Windows { windows: Vec<LimitWindow> },
    Unavailable { reason: UnavailableReason },
}

impl From<UsageOutcome> for OutcomeRepr {
    fn from(outcome: UsageOutcome) -> OutcomeRepr {
        match outcome {
            UsageOutcome::Windows(windows) => OutcomeRepr::Windows { windows },
            UsageOutcome::Unavailable(reason) => OutcomeRepr::Unavailable { reason },
        }
    }
}

impl From<OutcomeRepr> for UsageOutcome {
    fn from(repr: OutcomeRepr) -> UsageOutcome {
        match repr {
            OutcomeRepr::Windows { windows } => UsageOutcome::Windows(windows),
            OutcomeRepr::Unavailable { reason } => UsageOutcome::Unavailable(reason),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RemainingRepr {
    Percent { value: Percent },
    Amount { value: String, unit: String },
}

impl From<Remaining> for RemainingRepr {
    fn from(remaining: Remaining) -> RemainingRepr {
        match remaining {
            Remaining::Percent(value) => RemainingRepr::Percent { value },
            Remaining::Amount { value, unit } => RemainingRepr::Amount { value, unit },
        }
    }
}

impl From<RemainingRepr> for Remaining {
    fn from(repr: RemainingRepr) -> Remaining {
        match repr {
            RemainingRepr::Percent { value } => Remaining::Percent(value),
            RemainingRepr::Amount { value, unit } => Remaining::Amount { value, unit },
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ReasonRepr {
    NoCredentials,
    ExpiredLogin,
    Unsupported { detail: Option<String> },
    RateLimited { retry_at: Option<Timestamp> },
    Network { message: String },
    Provider { message: String },
}

impl From<UnavailableReason> for ReasonRepr {
    fn from(reason: UnavailableReason) -> ReasonRepr {
        match reason {
            UnavailableReason::NoCredentials => ReasonRepr::NoCredentials,
            UnavailableReason::ExpiredLogin => ReasonRepr::ExpiredLogin,
            UnavailableReason::Unsupported { detail } => ReasonRepr::Unsupported { detail },
            UnavailableReason::RateLimited { retry_at } => ReasonRepr::RateLimited { retry_at },
            UnavailableReason::Network(message) => ReasonRepr::Network { message },
            UnavailableReason::Provider(message) => ReasonRepr::Provider { message },
        }
    }
}

impl From<ReasonRepr> for UnavailableReason {
    fn from(repr: ReasonRepr) -> UnavailableReason {
        match repr {
            ReasonRepr::NoCredentials => UnavailableReason::NoCredentials,
            ReasonRepr::ExpiredLogin => UnavailableReason::ExpiredLogin,
            ReasonRepr::Unsupported { detail } => UnavailableReason::Unsupported { detail },
            ReasonRepr::RateLimited { retry_at } => UnavailableReason::RateLimited { retry_at },
            ReasonRepr::Network { message } => UnavailableReason::Network(message),
            ReasonRepr::Provider { message } => UnavailableReason::Provider(message),
        }
    }
}

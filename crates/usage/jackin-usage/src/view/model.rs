// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Timestamp;
use uuid::Uuid;

use crate::accounts::UsageProvider;

pub const FRESH_SECS: i64 = 600;

const SHORT_ID: usize = 8;
const COMPARE_EPSILON: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq)]
pub struct UsageView {
    pub now: Timestamp,
    pub utc_offset_secs: i32,
    pub warn_at: UsedFraction,
    pub critical_at: UsedFraction,
    pub groups: Vec<ProviderGroup>,
    pub attention: Vec<Attention>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderGroup {
    pub provider: UsageProvider,
    pub name: &'static str,
    pub accounts: Vec<AccountQuota>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccountQuota {
    pub id: Uuid,
    pub label: String,
    pub plan: Option<String>,
    pub reading: Reading,
    pub duplicates: Vec<Uuid>,
    pub feed: Feed,
    pub main: Option<Meter>,
    pub secondary: Vec<Meter>,
    pub notes: Vec<Note>,
    pub facts: Vec<Fact>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feed {
    Provider,
    StatusLine,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reading {
    Waiting,
    Fresh {
        at: Timestamp,
    },
    Stale {
        at: Timestamp,
    },
    Failed {
        at: Option<Timestamp>,
        message: String,
        retry_at: Option<Timestamp>,
        reconnect: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Meter {
    pub key: String,
    pub name: String,
    pub role: Role,
    pub window_secs: Option<u32>,
    pub resets_at: Option<Timestamp>,
    pub used: Used,
    pub rolled_over: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Overall,
    Scoped,
    Addon,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Used {
    Fraction(UsedFraction),
    Amount {
        used: f64,
        limit: PositiveAmount,
        unit: Unit,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct UsedFraction(f64);

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct PositiveAmount(f64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Credits,
    Tokens,
    Requests,
    Searches,
    Cents,
    Monetary,
    Money(Currency),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Currency([u8; 3]);

#[derive(Clone, Debug, PartialEq)]
pub enum Note {
    Balance {
        key: String,
        label: String,
        amount: f64,
        unit: Unit,
        approx: Option<(f64, Currency)>,
    },
    Resets(Vec<ResetCredit>),
    Tokens {
        today: Option<u64>,
        yesterday: Option<Activity>,
        last30: Option<Activity>,
    },
    Spend {
        today: Option<f64>,
        week: Option<f64>,
        month: Option<f64>,
        byok: Option<f64>,
        currency: Currency,
    },
    Daily {
        key: String,
        label: String,
        records: Option<u64>,
        lines: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetStatus {
    Ready,
    Queued,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResetCredit {
    pub kind: String,
    pub status: ResetStatus,
    pub expires_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activity {
    pub tokens: u64,
    pub turns: u64,
    pub cached_in: u64,
    pub input: u64,
    pub output: u64,
    pub observed: (u8, u8),
    pub models: Vec<(String, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fact {
    Region(String),
    AutoRenew(bool),
    MimoClaw(bool),
    ApiKeyAuth,
    DeviceAuth,
    GoogleProject(String),
    KeyType(String),
    PeriodStart(Timestamp),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Attention {
    Failing { account: Uuid },
    NearLimit { accounts: Vec<Uuid>, meter: String },
    Duplicate { accounts: Vec<Uuid>, remove: Uuid },
}

impl UsedFraction {
    pub fn new(value: f64) -> UsedFraction {
        if value.is_nan() {
            UsedFraction(0.0)
        } else {
            UsedFraction(value.clamp(0.0, 1.0))
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }

    pub fn percent(self) -> f64 {
        self.0 * 100.0
    }

    pub fn rounded(self) -> u8 {
        (0..=100u8)
            .find(|whole| self.percent() < f64::from(*whole) + 0.5)
            .unwrap_or(100)
    }

    pub fn at_least(self, threshold: UsedFraction) -> bool {
        self.0 + COMPARE_EPSILON >= threshold.0
    }
}

impl PositiveAmount {
    pub fn new(value: f64) -> Option<PositiveAmount> {
        (value.is_finite() && value > 0.0).then_some(PositiveAmount(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl Currency {
    pub const USD: Currency = Currency(*b"USD");

    pub fn new(code: &str) -> Option<Currency> {
        let bytes: [u8; 3] = code.as_bytes().try_into().ok()?;
        bytes
            .iter()
            .all(u8::is_ascii_alphabetic)
            .then(|| Currency(bytes.map(|byte| byte.to_ascii_uppercase())))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or_default()
    }
}

impl Reading {
    pub fn fetched_at(&self) -> Option<Timestamp> {
        match self {
            Reading::Waiting => None,
            Reading::Fresh { at } | Reading::Stale { at } => Some(*at),
            Reading::Failed { at, .. } => *at,
        }
    }

    pub fn age(&self, now: Timestamp) -> Option<i64> {
        self.fetched_at().map(|at| now.0.saturating_sub(at.0))
    }

    pub fn is_stale(&self, now: Timestamp) -> bool {
        match self {
            Reading::Stale { .. } => true,
            Reading::Failed { at: Some(at), .. } => now.0.saturating_sub(at.0) >= FRESH_SECS,
            Reading::Waiting | Reading::Fresh { .. } | Reading::Failed { at: None, .. } => false,
        }
    }
}

impl Meter {
    pub fn fraction(&self) -> UsedFraction {
        match &self.used {
            Used::Fraction(fraction) => *fraction,
            Used::Amount { used, limit, .. } => UsedFraction::new(used / limit.get()),
        }
    }

    pub fn live(&self, now: Timestamp) -> bool {
        self.resets_at.is_none_or(|at| at > now)
    }
}

impl AccountQuota {
    pub fn short(&self) -> String {
        short_id(self.id)
    }

    pub fn meters(&self) -> impl Iterator<Item = &Meter> {
        self.main.iter().chain(self.secondary.iter())
    }

    pub fn is_duplicate(&self) -> bool {
        !self.duplicates.is_empty()
    }
}

impl UsageView {
    pub fn accounts(&self) -> impl Iterator<Item = (&ProviderGroup, &AccountQuota)> {
        self.groups
            .iter()
            .flat_map(|group| group.accounts.iter().map(move |account| (group, account)))
    }

    pub fn account(&self, id: Uuid) -> Option<(&ProviderGroup, &AccountQuota)> {
        self.accounts().find(|(_, account)| account.id == id)
    }

    pub fn account_count(&self) -> usize {
        self.groups.iter().map(|group| group.accounts.len()).sum()
    }

    pub fn stale_ages(&self) -> Vec<i64> {
        let mut ages: Vec<i64> = self
            .accounts()
            .filter(|(_, account)| account.reading.is_stale(self.now))
            .filter_map(|(_, account)| account.reading.age(self.now))
            .collect();
        ages.sort_unstable();
        ages
    }
}

pub fn short_id(id: Uuid) -> String {
    id.hyphenated().to_string().chars().take(SHORT_ID).collect()
}

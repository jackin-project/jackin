// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::control::{FocusedUsageView, Money};
use crate::usage_monitor::{MonitorIssue, MonitorOperation, MonitorReply};

pub const USAGE_BROKER_PROTOCOL_VERSION: &str = "v8";

pub const USAGE_BROKER_MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UsageCredentialSourceIdentity {
    OnePassword {
        reference: String,

        account: Option<String>,
    },

    HostEnv {
        name: String,
    },

    Literal,
}

impl UsageCredentialSourceIdentity {
    #[must_use]
    pub fn from_declaration(declaration: &jackin_core::EnvValue) -> Self {
        match declaration {
            jackin_core::EnvValue::OpRef(reference) => Self::OnePassword {
                reference: reference.op.clone(),
                account: reference.account.clone(),
            },
            jackin_core::EnvValue::Extended(value) => Self::from_plain_value(&value.value),
            jackin_core::EnvValue::Plain(value) => Self::from_plain_value(value),
        }
    }

    fn from_plain_value(value: &str) -> Self {
        let name = value
            .strip_prefix("${")
            .and_then(|value| value.strip_suffix('}'))
            .or_else(|| value.strip_prefix('$'))
            .filter(|name| {
                let mut chars = name.chars();
                chars
                    .next()
                    .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
                    && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
            });
        name.map_or(Self::Literal, |name| Self::HostEnv {
            name: name.to_owned(),
        })
    }
}

#[must_use]
pub fn usage_credential_material_fingerprint(material: &str) -> String {
    jackin_core::account_key_hash("usage-credential-material-v1", material)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UsageCredentialSourceProof {
    pub account_id: String,

    pub surface_id: String,

    pub key: String,

    pub source: UsageCredentialSourceIdentity,

    pub material_fingerprint: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageCredentialScope {
    pub sources: BTreeSet<UsageCredentialSourceProof>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageRelayForwardedSourcesV1 {
    pub selected_account_ids: BTreeSet<String>,

    pub selected_account_surfaces: BTreeMap<String, String>,

    pub profile_surface_ids: BTreeSet<String>,

    pub env_keys: BTreeSet<String>,

    pub credential_scope: UsageCredentialScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UsageAccountCapability {
    pub account_id: String,

    pub surface_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UsageCatalogEntry {
    pub capability: UsageAccountCapability,

    pub revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageRelayCapabilityMappingV1 {
    pub account_id: String,

    pub surface_id: String,

    pub capability: UsageAccountCapability,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageRelayCapabilityResolutionV1 {
    pub capabilities: Vec<UsageAccountCapability>,

    pub selected_accounts: Vec<UsageRelayCapabilityMappingV1>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageRefreshPhase {
    Idle,

    Queued,

    Updating,

    Completed,

    Failed,
}

impl UsageRefreshPhase {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }

    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Updating)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageCoordinationErrorKind {
    Unavailable,

    Unauthorized,

    CatalogRevoked,

    CatalogRevisionConflict,

    OwnerLost,

    WaitTimeout,

    CorruptState,

    ProviderTimeout,

    ProviderUnavailable,

    NeedsSecret,

    RateLimited,

    ProtocolMismatch,

    BrokerConflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageCoordinationError {
    pub kind: UsageCoordinationErrorKind,

    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "u16", into = "u16")]
pub struct UsageProjectionSchemaV1;

impl TryFrom<u16> for UsageProjectionSchemaV1 {
    type Error = String;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value == 1 {
            Ok(Self)
        } else {
            Err(format!(
                "unsupported usage projection schema version {value}"
            ))
        }
    }
}

impl From<UsageProjectionSchemaV1> for u16 {
    fn from(_: UsageProjectionSchemaV1) -> Self {
        1
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(try_from = "u16", into = "u16")]
pub struct UsagePercent(u8);

impl UsagePercent {
    pub fn new(value: u8) -> Result<Self, String> {
        if value <= 100 {
            Ok(Self(value))
        } else {
            Err(format!("usage percentage {value} exceeds 100"))
        }
    }

    #[must_use]
    pub fn clamp_raw(value: i32) -> Self {
        match u8::try_from(value.clamp(0, 100)) {
            Ok(clamped) => Self(clamped),
            Err(_) => Self(100),
        }
    }

    #[must_use]
    pub fn split_raw(value: i32) -> (i32, Self) {
        (value, Self::clamp_raw(value))
    }

    #[must_use]
    pub const fn meter_fill(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u16> for UsagePercent {
    type Error = String;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        let value =
            u8::try_from(value).map_err(|_| format!("usage percentage {value} exceeds 100"))?;
        Self::new(value)
    }
}

impl From<UsagePercent> for u16 {
    fn from(value: UsagePercent) -> Self {
        u16::from(value.0)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageProjectionRefreshStateV1 {
    Idle,

    Refreshing,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageMembershipStateV1 {
    Current,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageIdentityKindV1 {
    ProviderAccountId,

    ProviderStableHandle,

    LocalSourceHandle,

    UnverifiedHandle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageLifecycleV1 {
    Available,

    AgentUninitialized,

    NeedsLogin,

    NeedsSecret,

    Unsupported,

    Unavailable,

    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageFreshnessPhaseV1 {
    Current,

    Stale,

    Refreshing,

    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageQuotaStateV1 {
    Available,

    NotStarted,

    Warning,

    Exhausted,

    Unsupported,

    Unavailable,

    NoPermission,

    Unknown,

    NotApplicable,

    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageWindowCategoryV1 {
    LongRange,

    Model,

    Session,

    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageIssueScopeV1 {
    Projection,

    Provider,

    Account,

    Window,

    Group,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageIssueRecoverabilityV1 {
    Retryable,

    ActionRequired,

    Unsupported,

    Terminal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageFreshnessV1 {
    pub generation: u64,

    pub phase: UsageFreshnessPhaseV1,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_good_at_epoch: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_at_epoch: Option<i64>,

    pub is_stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageIssueV1 {
    pub code: String,

    pub scope: UsageIssueScopeV1,

    pub recoverability: UsageIssueRecoverabilityV1,

    pub message: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_at_epoch: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageLimitWindowV1 {
    pub window_id: String,

    pub rank: u32,

    pub category: UsageWindowCategoryV1,

    pub label: String,

    pub value_label: String,

    pub reset_label: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_percent: Option<UsagePercent>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_raw_percent: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<UsagePercent>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_raw_percent: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_at_epoch: Option<i64>,

    pub quota_state: UsageQuotaStateV1,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pace_label: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs_out_label: Option<String>,
}

fn validate_percent_side(
    owner: &str,
    side: &str,
    clamped: Option<UsagePercent>,
    raw: Option<i32>,
) -> Result<bool, String> {
    if let Some(raw) = raw {
        let Some(clamped) = clamped else {
            return Err(format!(
                "usage {owner} has raw {side} percent without a clamped representation"
            ));
        };
        if clamped != UsagePercent::clamp_raw(raw) {
            return Err(format!(
                "usage {owner} has {side} percent inconsistent with its raw value"
            ));
        }
        Ok(true)
    } else {
        Ok(clamped.is_some())
    }
}

impl UsageLimitWindowV1 {
    pub fn validate(&self, expected_rank: usize) -> Result<(), String> {
        if usize::try_from(self.rank).ok() != Some(expected_rank) {
            return Err(format!("window {} has noncanonical rank", self.window_id));
        }
        let owner = format!("window {}", self.window_id);
        let remaining = validate_percent_side(
            &owner,
            "remaining",
            self.remaining_percent,
            self.remaining_raw_percent,
        )?;
        let used = validate_percent_side(&owner, "used", self.used_percent, self.used_raw_percent)?;
        if remaining && used {
            return Err(format!(
                "usage window {} has conflicting percent representations",
                self.window_id
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageMetricGroupKindV1 {
    Window,

    Balance,

    SpendCap,

    TokenTotals,

    RateLimit,

    Plan,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "period", rename_all = "snake_case")]
pub enum UsageMetricPeriodV1 {
    Rolling { window_secs: u64 },

    Calendar { granularity: UsageCalendarPeriodV1 },

    ProviderDefined,

    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageCalendarPeriodV1 {
    Daily,

    Weekly,

    Monthly,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageMetricScopeV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UsageMetricValueV1 {
    Window {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining_percent: Option<UsagePercent>,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining_raw_percent: Option<i32>,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        used_percent: Option<UsagePercent>,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        used_raw_percent: Option<i32>,

        period: UsageMetricPeriodV1,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
    },

    Balance {
        amount: Money,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at_epoch: Option<i64>,
    },

    SpendCap {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cap: Option<Money>,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        spent: Option<Money>,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining: Option<Money>,
    },

    TokenTotals {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<u64>,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<u64>,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        cached: Option<u64>,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning: Option<u64>,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        interval_label: Option<String>,
    },

    RateLimit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u64>,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining: Option<u64>,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_label: Option<String>,
    },

    Plan {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan_label: Option<String>,

        #[serde(default, skip_serializing_if = "Option::is_none")]
        tier: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageMetricGroupV1 {
    pub group_id: String,

    pub rank: u32,

    pub kind: UsageMetricGroupKindV1,

    pub label: String,

    pub scope: UsageMetricScopeV1,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_at_epoch: Option<i64>,

    pub fetched_at_epoch: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_success_at_epoch: Option<i64>,

    pub phase: UsageFreshnessPhaseV1,

    pub is_stale: bool,

    pub quota_state: UsageQuotaStateV1,

    pub value: UsageMetricValueV1,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_at_epoch: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub renews_at_epoch: Option<i64>,

    pub issues: Vec<UsageIssueV1>,
}

impl UsageMetricGroupV1 {
    pub fn validate(&self, expected_rank: usize) -> Result<(), String> {
        if usize::try_from(self.rank).ok() != Some(expected_rank) {
            return Err(format!("group {} has noncanonical rank", self.group_id));
        }
        if self.group_id.is_empty() {
            return Err("metric group has an empty group id".to_owned());
        }
        if self.label.is_empty() {
            return Err(format!("metric group {} has an empty label", self.group_id));
        }
        let kind_matches = matches!(
            (&self.kind, &self.value),
            (
                UsageMetricGroupKindV1::Window,
                UsageMetricValueV1::Window { .. }
            ) | (
                UsageMetricGroupKindV1::Balance,
                UsageMetricValueV1::Balance { .. }
            ) | (
                UsageMetricGroupKindV1::SpendCap,
                UsageMetricValueV1::SpendCap { .. }
            ) | (
                UsageMetricGroupKindV1::TokenTotals,
                UsageMetricValueV1::TokenTotals { .. }
            ) | (
                UsageMetricGroupKindV1::RateLimit,
                UsageMetricValueV1::RateLimit { .. }
            ) | (
                UsageMetricGroupKindV1::Plan,
                UsageMetricValueV1::Plan { .. }
            )
        );
        if !kind_matches {
            return Err(format!(
                "metric group {} has a value payload that does not match its kind",
                self.group_id
            ));
        }
        if let UsageMetricValueV1::Window {
            remaining_percent,
            remaining_raw_percent,
            used_percent,
            used_raw_percent,
            ..
        } = &self.value
        {
            let owner = format!("group {}", self.group_id);
            let remaining = validate_percent_side(
                &owner,
                "remaining",
                *remaining_percent,
                *remaining_raw_percent,
            )?;
            let used = validate_percent_side(&owner, "used", *used_percent, *used_raw_percent)?;
            if remaining && used {
                return Err(format!(
                    "usage group {} has conflicting percent representations",
                    self.group_id
                ));
            }
        }
        if let UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } = &self.value
        {
            let mut denomination: Option<(&str, u8)> = None;
            for money in [cap, spent, remaining].into_iter().flatten() {
                match denomination {
                    None => {
                        denomination = Some((money.currency.as_str(), money.exponent));
                    }
                    Some((currency, exponent))
                        if currency == money.currency && exponent == money.exponent => {}
                    Some(_) => {
                        return Err(format!(
                            "metric group {} mixes money denominations",
                            self.group_id
                        ));
                    }
                }
            }
        }
        if self.renews_at_epoch.is_some() && self.kind != UsageMetricGroupKindV1::Plan {
            return Err(format!(
                "metric group {} carries a renewal timestamp on a non-plan group",
                self.group_id
            ));
        }
        if self.reset_at_epoch.is_some()
            && !matches!(
                self.kind,
                UsageMetricGroupKindV1::Window
                    | UsageMetricGroupKindV1::SpendCap
                    | UsageMetricGroupKindV1::RateLimit
            )
        {
            return Err(format!(
                "metric group {} carries a reset timestamp on a group without quota reset",
                self.group_id
            ));
        }
        for issue in &self.issues {
            if issue.scope != UsageIssueScopeV1::Group {
                return Err(format!(
                    "metric group {} carries a non-group issue",
                    self.group_id
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct UsageQuotaScopeKey {
    pub service: String,

    pub billing_subject: String,

    pub scope: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl UsageQuotaScopeKey {
    pub fn new(
        service: impl Into<String>,
        billing_subject: impl Into<String>,
        scope: impl Into<String>,
    ) -> Self {
        Self {
            service: service.into(),
            billing_subject: billing_subject.into(),
            scope: scope.into(),
            model: None,
            key: None,
        }
    }

    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    #[must_use]
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    #[must_use]
    pub fn dedup_key(&self) -> String {
        fn segment(value: &str) -> String {
            format!("{}:{value}", value.len())
        }
        fn optional(value: Option<&str>) -> String {
            value.map_or_else(|| "-".to_owned(), segment)
        }
        format!(
            "quota-scope-v1:{}:{}:{}:{}:{}",
            segment(&self.service),
            segment(&self.billing_subject),
            segment(&self.scope),
            optional(self.model.as_deref()),
            optional(self.key.as_deref()),
        )
    }

    #[must_use]
    pub fn shares_allowance(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageAccountV1 {
    pub canonical_account_id: String,

    pub identity_kind: UsageIdentityKindV1,

    pub rank: u32,

    pub display_label: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_label: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_label: Option<String>,

    pub lifecycle: UsageLifecycleV1,

    pub freshness: UsageFreshnessV1,

    pub provenance_count: u32,

    pub windows: Vec<UsageLimitWindowV1>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metric_groups: Vec<UsageMetricGroupV1>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_expires_at_epoch: Option<i64>,

    pub issues: Vec<UsageIssueV1>,
}

impl UsageAccountV1 {
    fn validate(&self, expected_rank: usize) -> Result<(), String> {
        if usize::try_from(self.rank).ok() != Some(expected_rank) {
            return Err(format!(
                "account {} has noncanonical rank",
                self.canonical_account_id
            ));
        }
        for (window_rank, window) in self.windows.iter().enumerate() {
            window.validate(window_rank)?;
        }
        let mut group_ids = BTreeSet::new();
        for (group_rank, group) in self.metric_groups.iter().enumerate() {
            if !group_ids.insert(group.group_id.as_str()) {
                return Err(format!(
                    "account {} has duplicate metric group {}",
                    self.canonical_account_id, group.group_id
                ));
            }
            group.validate(group_rank)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageProviderV1 {
    pub provider_id: String,

    pub display_name: String,

    pub rank: u32,

    pub membership_state: UsageMembershipStateV1,

    pub freshness: UsageFreshnessV1,

    pub accounts: Vec<UsageAccountV1>,

    pub issues: Vec<UsageIssueV1>,
}

impl UsageProviderV1 {
    fn validate(&self, expected_rank: usize) -> Result<(), String> {
        if usize::try_from(self.rank).ok() != Some(expected_rank) {
            return Err(format!(
                "provider {} has noncanonical rank",
                self.provider_id
            ));
        }
        for (account_rank, account) in self.accounts.iter().enumerate() {
            account.validate(account_rank)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageUnresolvedV1 {
    pub provider_id: String,

    pub capability_id: String,

    pub configuration_count: u32,

    pub state: UsageLifecycleV1,

    pub issues: Vec<UsageIssueV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageProjectionV1 {
    pub schema_version: UsageProjectionSchemaV1,

    pub projection_id: String,

    pub generated_at_epoch: i64,

    pub discovery_revision: String,

    pub broker_instance_id: String,

    pub broker_generation: u64,

    pub refresh_state: UsageProjectionRefreshStateV1,

    pub providers: Vec<UsageProviderV1>,

    pub unresolved: Vec<UsageUnresolvedV1>,

    pub issues: Vec<UsageIssueV1>,
}

impl UsageProjectionV1 {
    pub fn validate(&self) -> Result<(), String> {
        for (provider_rank, provider) in self.providers.iter().enumerate() {
            provider.validate(provider_rank)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageGenerationView {
    pub capability: UsageAccountCapability,

    pub generation: u64,

    pub phase: UsageRefreshPhase,

    pub snapshot: Option<FocusedUsageView>,

    pub error: Option<UsageCoordinationError>,

    pub retry_at_epoch: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum UsageBrokerOperation {
    CurrentProjection,

    RequestRefresh {
        force: bool,

        observed_projection_id: Option<String>,
    },

    JoinPublication {
        projection_id: String,

        timeout_ms: u64,
    },

    CurrentProjectionForSurface,

    RequestRefreshForSurface {
        force: bool,

        observed_projection_id: Option<String>,
    },

    JoinPublicationForSurface {
        projection_id: String,

        timeout_ms: u64,
    },

    CurrentForCapability {
        capability: UsageAccountCapability,
    },

    RefreshForCapability {
        capability: UsageAccountCapability,

        observed_generation: u64,

        force: bool,
    },

    JoinForCapability {
        capability: UsageAccountCapability,

        generation: u64,

        timeout_ms: u64,
    },

    Current {
        capability: UsageAccountCapability,
    },

    Refresh {
        capability: UsageAccountCapability,

        observed_generation: u64,

        force: bool,
    },

    Join {
        capability: UsageAccountCapability,

        generation: u64,

        timeout_ms: u64,
    },

    Monitor {
        request: MonitorOperation,
    },

    ResolveRelayCapabilities {
        scope_label: String,

        forwarded_sources: UsageRelayForwardedSourcesV1,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageBrokerRequest {
    pub protocol_version: String,

    pub build_id: String,

    pub operation: UsageBrokerOperation,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_credential_scope: Option<UsageCredentialScope>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageRelayTunnelRequest {
    pub request_id: u64,

    pub request: UsageBrokerRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UsageBrokerResponse {
    State {
        state: Box<UsageGenerationView>,
    },

    Projection {
        projection: Box<UsageProjectionV1>,
    },

    Error {
        error: UsageCoordinationError,
    },

    Monitor {
        reply: MonitorReply,
    },

    MonitorError {
        issue: MonitorIssue,
    },

    RelayCapabilities {
        resolution: Box<UsageRelayCapabilityResolutionV1>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageRelayTunnelResponse {
    pub request_id: u64,

    pub response: UsageBrokerResponse,
}

#[cfg(test)]
mod tests;

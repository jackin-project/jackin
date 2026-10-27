// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::control::{
    CountQuota, CountQuotaPeriod, FocusedUsageView, MeterPercentPoints, Money,
    PercentObservationOrigin, ProviderQuotaState, RawPercentPoints, UsageMeterSide,
};
use crate::usage_monitor::{MonitorIssue, MonitorOperation, MonitorReply};

pub const USAGE_BROKER_PROTOCOL_VERSION: &str = "v9";

pub const USAGE_BROKER_MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const USAGE_RELAY_READY_MAX_FRAME_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UsageRelayReadyV1 {
    pub kind: UsageRelayReadyKindV1,
    pub protocol: UsageRelayReadyProtocolV2,
    pub version: UsageRelayReadyVersionV1,
}

impl UsageRelayReadyV1 {
    #[must_use]
    pub const fn canonical() -> Self {
        Self {
            kind: UsageRelayReadyKindV1::UsageRelayReady,
            protocol: UsageRelayReadyProtocolV2::V2,
            version: UsageRelayReadyVersionV1,
        }
    }

    pub fn encode_frame(self) -> Result<Vec<u8>, String> {
        let mut frame = serde_json::to_vec(&self).map_err(|error| error.to_string())?;
        frame.push(b'\n');
        if frame.len() > USAGE_RELAY_READY_MAX_FRAME_BYTES {
            return Err("usage relay readiness frame exceeds its size limit".to_owned());
        }
        Ok(frame)
    }

    pub fn decode_frame(frame: &[u8]) -> Result<Self, String> {
        if frame.is_empty() || frame.len() > USAGE_RELAY_READY_MAX_FRAME_BYTES {
            return Err("usage relay readiness frame has invalid framing".to_owned());
        }
        let Some(payload) = frame.strip_suffix(b"\n") else {
            return Err("usage relay readiness frame has invalid framing".to_owned());
        };
        if payload.contains(&b'\n') {
            return Err("usage relay readiness frame has invalid framing".to_owned());
        }
        let value: Self = serde_json::from_slice(payload).map_err(|error| error.to_string())?;
        if value != Self::canonical() || value.encode_frame()?.as_slice() != frame {
            return Err("usage relay readiness frame is not canonical".to_owned());
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UsageRelayReadyKindV1 {
    #[serde(rename = "usage_relay_ready")]
    UsageRelayReady,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UsageRelayReadyProtocolV2 {
    #[serde(rename = "usage-relay-owner-v2")]
    V2,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "u8", into = "u8")]
pub struct UsageRelayReadyVersionV1;

impl TryFrom<u8> for UsageRelayReadyVersionV1 {
    type Error = String;

    fn try_from(version: u8) -> Result<Self, Self::Error> {
        if version == 1 {
            Ok(Self)
        } else {
            Err(format!(
                "unsupported usage relay readiness version {version}"
            ))
        }
    }
}

impl From<UsageRelayReadyVersionV1> for u8 {
    fn from(_: UsageRelayReadyVersionV1) -> Self {
        1
    }
}

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

impl UsageAccountCapability {
    #[must_use]
    pub fn opaque_identity_key(&self) -> String {
        let identity = format!(
            "{}:{}{}:{}",
            self.surface_id.len(),
            self.surface_id,
            self.account_id.len(),
            self.account_id
        );
        let digest = jackin_core::account_key_hash("usage-capability-v1", &identity);
        format!("capability-v1:{digest}")
    }
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
pub struct UsageProjectionSchemaV2;

impl TryFrom<u16> for UsageProjectionSchemaV2 {
    type Error = String;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value == 2 {
            Ok(Self)
        } else {
            Err(format!(
                "unsupported usage projection schema version {value}"
            ))
        }
    }
}

impl From<UsageProjectionSchemaV2> for u16 {
    fn from(_: UsageProjectionSchemaV2) -> Self {
        2
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
    pub remaining_percent: Option<RawPercentPoints>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_percent_origin: Option<PercentObservationOrigin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<RawPercentPoints>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percent_origin: Option<PercentObservationOrigin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meter_percent: Option<MeterPercentPoints>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meter_side: Option<UsageMeterSide>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count_quota: Option<CountQuota>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_quota_state: Option<ProviderQuotaState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub money_used_percent: Option<RawPercentPoints>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_at_epoch: Option<i64>,
    pub quota_state: UsageQuotaStateV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pace_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs_out_label: Option<String>,
}

fn validate_percent_origin(
    owner: &str,
    side: &str,
    raw: Option<RawPercentPoints>,
    origin: Option<PercentObservationOrigin>,
) -> Result<(), String> {
    if raw.is_some() != origin.is_some() {
        return Err(format!(
            "usage {owner} has {side} percent and origin without a matching pair"
        ));
    }
    Ok(())
}

fn validate_meter_pair(
    owner: &str,
    meter_percent: Option<MeterPercentPoints>,
    meter_side: Option<UsageMeterSide>,
) -> Result<(), String> {
    if meter_percent.is_some() != meter_side.is_some() {
        return Err(format!(
            "usage {owner} has meter geometry and side without a matching pair"
        ));
    }
    Ok(())
}

fn validate_provider_quota_state(
    owner: &str,
    provider_state: Option<ProviderQuotaState>,
    meter_percent: Option<MeterPercentPoints>,
    meter_side: Option<UsageMeterSide>,
    effective_state: UsageQuotaStateV1,
) -> Result<(), String> {
    let lifecycle_override = matches!(
        effective_state,
        UsageQuotaStateV1::Error
            | UsageQuotaStateV1::Unavailable
            | UsageQuotaStateV1::NoPermission
            | UsageQuotaStateV1::Unsupported
    );

    match provider_state {
        None => Ok(()),
        Some(ProviderQuotaState::Unlimited) => {
            if meter_percent.is_some() || meter_side.is_some() {
                return Err(format!(
                    "usage {owner} has meter geometry for an unlimited provider quota"
                ));
            }
            if !lifecycle_override && effective_state != UsageQuotaStateV1::NotApplicable {
                return Err(format!(
                    "usage {owner} has an effective quota state inconsistent with an unlimited provider quota"
                ));
            }
            Ok(())
        }
        Some(ProviderQuotaState::Exhausted) => {
            if meter_percent.map(MeterPercentPoints::get) != Some(0.0)
                || meter_side != Some(UsageMeterSide::Remaining)
            {
                return Err(format!(
                    "usage {owner} must use zero remaining meter geometry for an exhausted provider quota"
                ));
            }
            if !lifecycle_override && effective_state != UsageQuotaStateV1::Exhausted {
                return Err(format!(
                    "usage {owner} has an effective quota state inconsistent with an exhausted provider quota"
                ));
            }
            Ok(())
        }
    }
}

fn validate_count_period_unit(
    owner: &str,
    count_quota: CountQuota,
    period: UsageMetricPeriodV1,
    unit: Option<&str>,
) -> Result<(), String> {
    if count_quota.unit.machine_unit() != unit {
        return Err(format!(
            "usage {owner} has a count unit inconsistent with its window unit"
        ));
    }
    let period_matches = matches!(
        (count_quota.period, period),
        (
            CountQuotaPeriod::UtcDaily,
            UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Daily
            }
        ) | (CountQuotaPeriod::Unknown, UsageMetricPeriodV1::Unknown)
    );
    if !period_matches {
        return Err(format!(
            "usage {owner} has a count period inconsistent with its window period"
        ));
    }
    Ok(())
}

impl UsageLimitWindowV1 {
    pub fn validate(&self, expected_rank: usize) -> Result<(), String> {
        if usize::try_from(self.rank).ok() != Some(expected_rank) {
            return Err(format!("window {} has noncanonical rank", self.window_id));
        }
        let owner = format!("window {}", self.window_id);
        validate_percent_origin(
            &owner,
            "remaining",
            self.remaining_percent,
            self.remaining_percent_origin,
        )?;
        validate_percent_origin(&owner, "used", self.used_percent, self.used_percent_origin)?;
        validate_meter_pair(&owner, self.meter_percent, self.meter_side)?;
        validate_provider_quota_state(
            &owner,
            self.provider_quota_state,
            self.meter_percent,
            self.meter_side,
            self.quota_state,
        )?;
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
        remaining_percent: Option<RawPercentPoints>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining_percent_origin: Option<PercentObservationOrigin>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        used_percent: Option<RawPercentPoints>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        used_percent_origin: Option<PercentObservationOrigin>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meter_percent: Option<MeterPercentPoints>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meter_side: Option<UsageMeterSide>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count_quota: Option<CountQuota>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider_quota_state: Option<ProviderQuotaState>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        money_used_percent: Option<RawPercentPoints>,
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
            remaining_percent_origin,
            used_percent,
            used_percent_origin,
            meter_percent,
            meter_side,
            count_quota,
            provider_quota_state,
            money_used_percent: _,
            period,
            unit,
        } = &self.value
        {
            let owner = format!("group {}", self.group_id);
            validate_percent_origin(
                &owner,
                "remaining",
                *remaining_percent,
                *remaining_percent_origin,
            )?;
            validate_percent_origin(&owner, "used", *used_percent, *used_percent_origin)?;
            validate_meter_pair(&owner, *meter_percent, *meter_side)?;
            validate_provider_quota_state(
                &owner,
                *provider_quota_state,
                *meter_percent,
                *meter_side,
                self.quota_state,
            )?;
            if let Some(count_quota) = count_quota {
                validate_count_period_unit(&owner, *count_quota, *period, unit.as_deref())?;
            }
        }
        if let UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } = &self.value
        {
            let mut denomination: Option<&str> = None;
            for money in [cap, spent, remaining].into_iter().flatten() {
                match denomination {
                    None => {
                        denomination = Some(money.currency.as_str());
                    }
                    Some(currency) if currency == money.currency => {}
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
    pub schema_version: UsageProjectionSchemaV2,
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
#[path = "usage_broker/numeric_tests.rs"]
mod tests;

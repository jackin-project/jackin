// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Versioned, secret-free usage-broker wire records.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::control::{FocusedUsageView, Money};

/// Usage-broker wire protocol version.
pub const USAGE_BROKER_PROTOCOL_VERSION: &str = "v3";

/// Maximum newline-delimited request or response body.
pub const USAGE_BROKER_MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Exact non-secret identity of a launch credential source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UsageCredentialSourceIdentity {
    /// Pinned 1Password reference and, when configured, its account.
    OnePassword {
        /// Canonical `op://` reference.
        reference: String,
        /// Explicit 1Password account selector.
        account: Option<String>,
    },
    /// Host environment variable name used by a `$VAR` declaration.
    HostEnv {
        /// Exact host variable name.
        name: String,
    },
    /// Inline literal source. Its material fingerprint carries the value
    /// without putting that value on the wire.
    Literal,
}

impl UsageCredentialSourceIdentity {
    /// Derive the source identity from one persisted operator declaration.
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

/// Content fingerprint of resolved credential material.
///
/// The material is accepted only at launch staging and never serialized.
#[must_use]
pub fn usage_credential_material_fingerprint(material: &str) -> String {
    jackin_core::account_key_hash("usage-credential-material-v1", material)
}

/// Secret-free proof that one exact launch credential source was staged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UsageCredentialSourceProof {
    /// Configured account selected for the launch instance.
    pub account_id: String,
    /// Canonical broker surface.
    pub surface_id: String,
    /// Governed environment key consumed by the provider.
    pub key: String,
    /// Exact source declaration identity at staging time.
    pub source: UsageCredentialSourceIdentity,
    /// Fingerprint of the material staged into the instance credential file.
    pub material_fingerprint: String,
}

/// Immutable launch scope carried from staging through the relay to the broker.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageCredentialScope {
    /// All exact source proofs admitted to this launch.
    pub sources: BTreeSet<UsageCredentialSourceProof>,
}

/// Opaque authority for one canonical provider account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UsageAccountCapability {
    /// Host-generated opaque canonical account identifier.
    pub account_id: String,
    /// Closed Rust-owned provider surface identifier.
    pub surface_id: String,
}

/// One current broker-catalog member and its secret-free credential/capability
/// revision. The revision fences work started under an older catalog entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UsageCatalogEntry {
    /// Canonical provider-account capability.
    pub capability: UsageAccountCapability,
    /// Content-derived revision for this capability's current admission.
    pub revision: String,
}

/// Lifecycle phase of one account refresh generation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageRefreshPhase {
    /// No generation has started.
    Idle,
    /// A bounded worker owns the generation but has not begun its probe.
    Queued,
    /// Provider work is active.
    Updating,
    /// The generation published a data-bearing result.
    Completed,
    /// The generation terminated without replacing last-good data.
    Failed,
}

impl UsageRefreshPhase {
    /// Whether this phase has a terminal result.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }

    /// Whether this phase has an active owner.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Updating)
    }
}

/// Stable coordination failure category; never contains raw I/O details.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageCoordinationErrorKind {
    /// Broker or state infrastructure is unavailable.
    Unavailable,
    /// The caller lacks the requested account capability.
    Unauthorized,
    /// The capability was removed or disabled from the current broker catalog.
    CatalogRevoked,
    /// A catalog rotation was based on an obsolete publication lease.
    CatalogRevisionConflict,
    /// The active generation owner disappeared.
    OwnerLost,
    /// A bounded generation wait expired while ownership remained active.
    WaitTimeout,
    /// Persisted state failed validation.
    CorruptState,
    /// Provider work timed out without publishing empty data.
    ProviderTimeout,
    /// Provider declined or cannot supply this usage surface.
    ProviderUnavailable,
    /// Provider authentication needs a host-side secret.
    NeedsSecret,
    /// Provider rate limiting deferred the next generation.
    RateLimited,
    /// Broker protocol or build handshake failed.
    ProtocolMismatch,
}

/// Sanitized coordination error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageCoordinationError {
    /// Stable failure category.
    pub kind: UsageCoordinationErrorKind,
    /// Bounded operator-facing message with no path or credential material.
    pub message: String,
}

/// Canonical usage-projection schema version 1.
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

/// Validated percentage in the inclusive range `0..=100`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(try_from = "u16", into = "u16")]
pub struct UsagePercent(u8);

impl UsagePercent {
    /// Build a validated percentage.
    pub fn new(value: u8) -> Result<Self, String> {
        if value <= 100 {
            Ok(Self(value))
        } else {
            Err(format!("usage percentage {value} exceeds 100"))
        }
    }

    /// Saturating clamp of a raw provider percentage into `0..=100`.
    ///
    /// Never wraps or underflows: negatives become `0`, over-100% becomes
    /// `100`. The unclamped raw value is preserved beside the clamped percent
    /// so labels can report overage honestly; only bar geometry uses this.
    #[must_use]
    pub fn clamp_raw(value: i32) -> Self {
        match u8::try_from(value.clamp(0, 100)) {
            Ok(clamped) => Self(clamped),
            Err(_) => Self(100),
        }
    }

    /// Split a raw provider percentage into its preserved raw value and its
    /// clamped geometry percent.
    #[must_use]
    pub fn split_raw(value: i32) -> (i32, Self) {
        (value, Self::clamp_raw(value))
    }

    /// Bar-geometry fill in `0..=100`. Geometry clamps only: consumers must
    /// never derive display text or remaining credit from this alone when a
    /// raw value is present.
    #[must_use]
    pub const fn meter_fill(self) -> u8 {
        self.0
    }

    /// Return the validated integer percentage.
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

/// Whole-projection refresh state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageProjectionRefreshStateV1 {
    /// No canonical publication is being refreshed.
    Idle,
    /// One canonical publication generation is active.
    Refreshing,
}

/// Current-configuration membership state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageMembershipStateV1 {
    /// The provider is present in current read-only discovery.
    Current,
}

/// Non-secret evidence kind backing canonical account identity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageIdentityKindV1 {
    /// Provider-issued immutable account or organization identifier.
    ProviderAccountId,
    /// Provider-issued stable non-secret handle.
    ProviderStableHandle,
}

/// Account or agent lifecycle independent of quota freshness.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageLifecycleV1 {
    /// Account can currently supply usage.
    Available,
    /// Capsule agent has not started its first session.
    AgentUninitialized,
    /// Operator login is required.
    NeedsLogin,
    /// A trusted credential is required.
    NeedsSecret,
    /// Provider or account has no supported usage capability.
    Unsupported,
    /// Usage is temporarily unavailable.
    Unavailable,
    /// Usage failed with a sanitized error.
    Error,
}

/// Freshness phase for a provider or account.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageFreshnessPhaseV1 {
    /// Data is current at its broker deadline.
    Current,
    /// Last-good data is retained beyond its current deadline.
    Stale,
    /// Last-good data is retained while refresh work runs.
    Refreshing,
    /// No usable current or last-good data exists.
    Failed,
}

/// Semantic state of one provider-supplied quota window or metric group.
///
/// Every variant stays distinct: unknown is never rendered as `0%`, a missing
/// permission is never reported as unsupported, and exhaustion is never
/// confused with an authentication or provider error.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageQuotaStateV1 {
    /// Quota is available.
    Available,
    /// Provider explicitly reports that the window has not started. Missing
    /// data alone never establishes this state.
    NotStarted,
    /// Quota is below the Rust-owned warning threshold.
    Warning,
    /// Zero remaining from a valid authoritative response.
    Exhausted,
    /// Window semantics are unsupported.
    Unsupported,
    /// Window is temporarily unavailable.
    Unavailable,
    /// Valid credentials lack permission for this metric (for example, an
    /// inference key reading a billing balance). Never collapse into
    /// [`Self::Unsupported`]: other metrics on the same account keep working.
    NoPermission,
    /// No usable limit is known: unverified identity, missing permission
    /// detail, source failure, unsupported schema, or a confirmed unpublished
    /// limit. Never render an empty `0%` bar for this state.
    Unknown,
    /// Quota semantics do not apply to this metric (plan metadata, token
    /// totals, uncapped spend). Never merge into [`Self::Available`].
    NotApplicable,
    /// Window failed with a sanitized error.
    Error,
}

/// Semantic quota-window category used for Rust-owned summary priority.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageWindowCategoryV1 {
    /// Daily, weekly, or monthly provider allowance.
    LongRange,
    /// Provider-supplied model-specific allowance.
    Model,
    /// Short session or rolling interaction allowance.
    Session,
    /// Provider-defined quota without a more specific category.
    Other,
}

/// Scope of one sanitized canonical issue.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageIssueScopeV1 {
    /// Whole projection.
    Projection,
    /// Provider group.
    Provider,
    /// Canonical account.
    Account,
    /// Quota window.
    Window,
    /// Typed metric group.
    Group,
}

/// Recovery category for one sanitized canonical issue.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageIssueRecoverabilityV1 {
    /// Broker may retry under policy.
    Retryable,
    /// Operator action is required.
    ActionRequired,
    /// Contract is unsupported.
    Unsupported,
    /// Failure is terminal for current membership.
    Terminal,
}

/// Freshness metadata shared by provider and account projections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageFreshnessV1 {
    /// Broker generation supplying this state.
    pub generation: u64,
    /// Current freshness phase.
    pub phase: UsageFreshnessPhaseV1,
    /// Last successful observation time in UTC Unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_good_at_epoch: Option<i64>,
    /// Earliest broker-owned retry time in UTC Unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_at_epoch: Option<i64>,
    /// Whether displayed data is retained last-good data.
    pub is_stale: bool,
}

/// Sanitized structured issue in a canonical projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageIssueV1 {
    /// Stable machine-readable issue code.
    pub code: String,
    /// Projection location affected by the issue.
    pub scope: UsageIssueScopeV1,
    /// Recovery category.
    pub recoverability: UsageIssueRecoverabilityV1,
    /// Rust-owned bounded operator message.
    pub message: String,
    /// Earliest broker-owned retry time in UTC Unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_at_epoch: Option<i64>,
}

/// One provider-supplied quota window in final Rust-owned order.
///
/// `windows` is the principal-window projection: the short list every surface
/// renders. Full typed detail lives in
/// [`UsageAccountV1::metric_groups`]; window semantics here are unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageLimitWindowV1 {
    /// Stable opaque window identifier.
    pub window_id: String,
    /// Zero-based Rust-owned display rank.
    pub rank: u32,
    /// Rust-owned semantic category; consumers never parse `label`.
    pub category: UsageWindowCategoryV1,
    /// Rust-owned provider window label.
    pub label: String,
    /// Rust-owned primary value label.
    pub value_label: String,
    /// Rust-owned reset label.
    pub reset_label: String,
    /// Remaining quota when the provider reports a remaining representation.
    /// Clamped geometry; the preserved raw value travels in
    /// `remaining_raw_percent`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_percent: Option<UsagePercent>,
    /// Preserved raw remaining percentage, unclamped. Over-100% overage and
    /// negative provider values survive here; bar geometry clamps only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_raw_percent: Option<i32>,
    /// Used quota when the provider reports a used representation. Clamped
    /// geometry; the preserved raw value travels in `used_raw_percent`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<UsagePercent>,
    /// Preserved raw used percentage, unclamped. Over-100% overage survives
    /// here; bar geometry clamps only and negative remaining never becomes
    /// fabricated credit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_raw_percent: Option<i32>,
    /// Quota-window reset time in UTC Unix seconds when known. This is not a
    /// credential expiry ([`UsageAccountV1::credential_expires_at_epoch`]) and
    /// not a subscription renewal (plan-group `renews_at_epoch`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_at_epoch: Option<i64>,
    /// Semantic quota state.
    pub quota_state: UsageQuotaStateV1,
    /// Optional rich-surface pace label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pace_label: Option<String>,
    /// Optional rich current-detail run-out estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs_out_label: Option<String>,
}

/// Validate one remaining/used percent side: a raw value requires its clamped
/// representation, and the clamped value must equal the saturating clamp of
/// the raw value. Returns whether this side carries a representation.
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
    /// Validate cross-field representation invariants.
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

/// Canonical class of one per-account metric group.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageMetricGroupKindV1 {
    /// Allowance window with a limit/used/remaining representation.
    Window,
    /// Prepaid or remaining balance; never a percentage without a meaningful
    /// denominator.
    Balance,
    /// Spending cap with cap/spend/remaining amounts.
    SpendCap,
    /// Input/output/cached/reasoning token totals. Totals are observed
    /// consumption, never a claim of remaining allowance.
    TokenTotals,
    /// Requests- or tokens-per-interval rate limit, separate from prepaid or
    /// subscription balance.
    RateLimit,
    /// Provider-returned plan, tenant, tier, and auth metadata.
    Plan,
}

/// Typed allowance period.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "period", rename_all = "snake_case")]
pub enum UsageMetricPeriodV1 {
    /// Rolling duration window.
    Rolling {
        /// Window length in seconds.
        window_secs: u64,
    },
    /// Fixed calendar period.
    Calendar {
        /// Calendar granularity.
        granularity: UsageCalendarPeriodV1,
    },
    /// Provider-defined period without a machine-readable duration.
    ProviderDefined,
    /// Period is not known.
    Unknown,
}

/// Fixed calendar period granularity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageCalendarPeriodV1 {
    /// Daily allowance.
    Daily,
    /// Weekly allowance.
    Weekly,
    /// Monthly allowance.
    Monthly,
}

/// Non-secret scope labels locating one metric group inside its account.
///
/// Every label is provider-supplied identity (exact model/pool identifier,
/// opaque key identity), never a secret. `None` means the provider did not
/// scope this group on that axis, not that the axis was merged away.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageMetricScopeV1 {
    /// Provider service identity when it narrows the account scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// Exact model identifier when the group is model-scoped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Named credit pool when the group is pool-scoped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    /// Opaque non-secret key identity when the group is key-cap-scoped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
}

/// Typed payload of one metric group.
///
/// Money always carries currency and exponent ([`Money`]); percents always
/// pair clamped geometry with preserved raw values. Nothing here is inferred:
/// a missing field means the provider did not supply it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UsageMetricValueV1 {
    /// Allowance-window payload.
    Window {
        /// Clamped remaining geometry.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining_percent: Option<UsagePercent>,
        /// Preserved raw remaining percentage.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining_raw_percent: Option<i32>,
        /// Clamped used geometry.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        used_percent: Option<UsagePercent>,
        /// Preserved raw used percentage.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        used_raw_percent: Option<i32>,
        /// Typed allowance period.
        period: UsageMetricPeriodV1,
        /// Provider unit label when supplied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
    },
    /// Balance payload.
    Balance {
        /// Prepaid or remaining value with currency and exponent.
        amount: Money,
        /// Balance expiry in UTC Unix seconds when supplied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at_epoch: Option<i64>,
    },
    /// Spending-cap payload.
    SpendCap {
        /// Cap amount; `None` means uncapped spend tracking.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cap: Option<Money>,
        /// Spent amount in the billing period.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spent: Option<Money>,
        /// Remaining amount when the provider reports it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining: Option<Money>,
    },
    /// Token-totals payload.
    TokenTotals {
        /// Input tokens observed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input: Option<u64>,
        /// Output tokens observed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<u64>,
        /// Cached tokens observed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cached: Option<u64>,
        /// Reasoning tokens observed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning: Option<u64>,
        /// Measurement interval or source label.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        interval_label: Option<String>,
    },
    /// Rate-limit payload.
    RateLimit {
        /// Requests or tokens allowed per interval.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u64>,
        /// Requests or tokens remaining when supplied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining: Option<u64>,
        /// Interval label when supplied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_label: Option<String>,
    },
    /// Plan and account-metadata payload.
    Plan {
        /// Provider-returned plan label.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plan_label: Option<String>,
        /// Provider-returned tier label.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tier: Option<String>,
    },
}

/// One independently fetched typed metric group.
///
/// Each group carries its own scope labels, observation timestamps, freshness
/// phase, quota state, typed value, and issues. Fresh main quota never makes a
/// retained old balance fresh: staleness is per group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageMetricGroupV1 {
    /// Stable opaque group identifier.
    pub group_id: String,
    /// Zero-based Rust-owned display rank within the account.
    pub rank: u32,
    /// Canonical metric class.
    pub kind: UsageMetricGroupKindV1,
    /// Rust-owned group display label.
    pub label: String,
    /// Non-secret scope labels.
    pub scope: UsageMetricScopeV1,
    /// Provider observation time in UTC Unix seconds when reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_at_epoch: Option<i64>,
    /// Transport completion time in UTC Unix seconds.
    pub fetched_at_epoch: i64,
    /// Last successful observation time in UTC Unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_success_at_epoch: Option<i64>,
    /// Group freshness phase, independent of sibling groups.
    pub phase: UsageFreshnessPhaseV1,
    /// Whether displayed data is retained last-good data.
    pub is_stale: bool,
    /// Semantic quota state; [`UsageQuotaStateV1`] variants stay distinct.
    pub quota_state: UsageQuotaStateV1,
    /// Typed value payload; must match `kind`.
    pub value: UsageMetricValueV1,
    /// Quota-window or billing-period reset in UTC Unix seconds. Valid only on
    /// window, spend-cap, and rate-limit groups; never a credential expiry or
    /// a subscription renewal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_at_epoch: Option<i64>,
    /// Subscription renewal in UTC Unix seconds. Valid only on plan groups;
    /// never a quota reset or a credential expiry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renews_at_epoch: Option<i64>,
    /// Sanitized group-scoped issues; every entry has
    /// [`UsageIssueScopeV1::Group`] scope.
    pub issues: Vec<UsageIssueV1>,
}

impl UsageMetricGroupV1 {
    /// Validate rank, kind/value consistency, percent pairing, money
    /// consistency, reset/renewal separation, and issue scope.
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

/// Canonical quota scope: service plus billing subject plus scope, model, and
/// key identity.
///
/// Two observations share one canonical allowance if and only if their full
/// scope keys are equal ([`Self::shares_allowance`]).
///
/// Never-merge-independent-caps rule: observations under the same billing
/// subject but with different key identities are independent key caps and must
/// never merge, sum, or share one canonical observation. An unscoped
/// observation (`model`/`key` [`None`]) never shares an allowance with a
/// scoped one, and an empty scope string never equals a missing one. Merging
/// happens only on full key equality.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct UsageQuotaScopeKey {
    /// Provider service identity (for example, distinct billing products of
    /// one provider are distinct services).
    pub service: String,
    /// Verified billing subject: person, organization, workspace, or project
    /// scope the provider bills.
    pub billing_subject: String,
    /// Quota scope within the billing subject (subscription, key, pool, or
    /// organization cap identity).
    pub scope: String,
    /// Exact model identity when the allowance is model-scoped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Opaque non-secret key identity when the allowance is key-cap-scoped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl UsageQuotaScopeKey {
    /// Build a quota scope key for a subscription-level allowance.
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

    /// Narrow this scope to one exact model identity.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Narrow this scope to one opaque non-secret key identity.
    #[must_use]
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Stable deduplication key. Length-prefixed segments keep distinct
    /// segment tuples distinct, and missing optionals encode distinctly from
    /// empty strings.
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

    /// Whether two observations may share one canonical allowance: full scope
    /// key equality, nothing less.
    #[must_use]
    pub fn shares_allowance(&self, other: &Self) -> bool {
        self == other
    }
}

/// One deduplicated canonical account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageAccountV1 {
    /// Opaque canonical account identifier.
    pub canonical_account_id: String,
    /// Non-secret evidence kind backing the identifier.
    pub identity_kind: UsageIdentityKindV1,
    /// Zero-based Rust-owned account display rank.
    pub rank: u32,
    /// Rust-owned full account display label.
    pub display_label: String,
    /// Provider plan label when supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_label: Option<String>,
    /// Rust-owned account status label when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_label: Option<String>,
    /// Account or Capsule-agent lifecycle.
    pub lifecycle: UsageLifecycleV1,
    /// Account freshness.
    pub freshness: UsageFreshnessV1,
    /// Count of current discovery observations merged into this account.
    pub provenance_count: u32,
    /// Provider/source-ordered principal quota windows. Unchanged semantics:
    /// the short list every surface renders.
    pub windows: Vec<UsageLimitWindowV1>,
    /// Typed metric groups with per-group scope, timestamps, freshness, and
    /// issues. Empty until a broker populates full typed detail.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metric_groups: Vec<UsageMetricGroupV1>,
    /// Credential or auth-session expiry in UTC Unix seconds when the provider
    /// reports it. This is never a quota-window reset (`reset_at_epoch`) and
    /// never a subscription renewal (plan-group `renews_at_epoch`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_expires_at_epoch: Option<i64>,
    /// Sanitized account/window issues.
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

/// One current provider group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageProviderV1 {
    /// Closed provider identifier.
    pub provider_id: String,
    /// Provider-only visible name.
    pub display_name: String,
    /// Zero-based settled provider rank.
    pub rank: u32,
    /// Current configuration membership.
    pub membership_state: UsageMembershipStateV1,
    /// Provider freshness.
    pub freshness: UsageFreshnessV1,
    /// Canonical accounts in Rust-owned order.
    pub accounts: Vec<UsageAccountV1>,
    /// Sanitized provider issues.
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

/// Configured capability lacking non-secret canonical identity evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageUnresolvedV1 {
    /// Closed provider identifier.
    pub provider_id: String,
    /// Opaque non-secret capability identifier.
    pub capability_id: String,
    /// Number of current configuration observations for this capability.
    pub configuration_count: u32,
    /// Rust-owned unresolved state label.
    pub state: UsageLifecycleV1,
    /// Sanitized resolution issues.
    pub issues: Vec<UsageIssueV1>,
}

/// Immutable canonical usage publication consumed by every surface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageProjectionV1 {
    /// Exact schema major, serialized as integer `1`.
    pub schema_version: UsageProjectionSchemaV1,
    /// Opaque monotonic publication identifier.
    pub projection_id: String,
    /// Publication time in UTC Unix seconds.
    pub generated_at_epoch: i64,
    /// Opaque current discovery revision.
    pub discovery_revision: String,
    /// Opaque broker process incarnation.
    pub broker_instance_id: String,
    /// Monotonic broker publication generation.
    pub broker_generation: u64,
    /// Whole-projection refresh state.
    pub refresh_state: UsageProjectionRefreshStateV1,
    /// Current providers in settled host order.
    pub providers: Vec<UsageProviderV1>,
    /// Current configured capabilities without canonical identity evidence.
    pub unresolved: Vec<UsageUnresolvedV1>,
    /// Sanitized projection issues.
    pub issues: Vec<UsageIssueV1>,
}

impl UsageProjectionV1 {
    /// Validate ranks and cross-field window invariants.
    pub fn validate(&self) -> Result<(), String> {
        for (provider_rank, provider) in self.providers.iter().enumerate() {
            provider.validate(provider_rank)?;
        }
        Ok(())
    }
}

/// Current projection of one canonical refresh generation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageGenerationView {
    /// Account authority this state belongs to.
    pub capability: UsageAccountCapability,
    /// Monotonic per-account generation number.
    pub generation: u64,
    /// Current generation phase.
    pub phase: UsageRefreshPhase,
    /// Sanitized current or preserved last-good quota projection.
    pub snapshot: Option<FocusedUsageView>,
    /// Typed terminal or coordination failure.
    pub error: Option<UsageCoordinationError>,
    /// Shared provider retry deadline when supplied.
    pub retry_at_epoch: Option<i64>,
}

/// One client operation against the host usage broker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum UsageBrokerOperation {
    /// Host-only atomic replacement of the broker's current capability catalog.
    ///
    /// A Capsule relay must reject this operation; it is never forwarded from
    /// an in-container caller.
    ReconcileCatalog {
        /// Publication lease observed immediately before discovery started.
        /// The broker rejects the rotation when this is no longer current.
        expected_projection_id: Option<String>,
        /// Content-derived current discovery revision.
        catalog_revision: String,
        /// Current canonical capability entries.
        entries: Vec<UsageCatalogEntry>,
    },
    /// Read the latest immutable canonical projection without provider work.
    CurrentProjection,
    /// Request one broker-owned projection refresh and return the latest publication.
    RequestRefresh {
        /// True only for an explicit operator refresh.
        force: bool,
        /// Publication observed by the caller, when available.
        observed_projection_id: Option<String>,
    },
    /// Join a named immutable projection publication.
    JoinPublication {
        /// Publication identifier returned by a refresh request.
        projection_id: String,
        /// Bounded client wait in milliseconds.
        timeout_ms: u64,
    },
    /// Relay-only current canonical projection request.
    CurrentProjectionForSurface,
    /// Relay-only projection refresh request.
    RequestRefreshForSurface {
        /// True only for an explicit operator refresh.
        force: bool,
        /// Publication observed by the caller, when available.
        observed_projection_id: Option<String>,
    },
    /// Relay-only projection publication join.
    JoinPublicationForSurface {
        /// Publication identifier returned by a refresh request.
        projection_id: String,
        /// Bounded client wait in milliseconds.
        timeout_ms: u64,
    },
    /// Relay-only current-state request for one exact forwarded capability.
    /// The per-container relay authorizes this capability before forwarding;
    /// the global host broker rejects this operation directly.
    CurrentForCapability {
        /// Exact account authority selected for this Capsule session.
        capability: UsageAccountCapability,
    },
    /// Relay-only refresh request for one exact forwarded capability.
    RefreshForCapability {
        /// Exact account authority selected for this Capsule session.
        capability: UsageAccountCapability,
        /// Last generation observed by the caller.
        observed_generation: u64,
        /// True only for an explicit operator Refresh action.
        force: bool,
    },
    /// Relay-only wait for one exact capability generation.
    JoinForCapability {
        /// Exact account authority selected for this Capsule session.
        capability: UsageAccountCapability,
        /// Generation returned by a prior refresh request.
        generation: u64,
        /// Bounded client wait in milliseconds.
        timeout_ms: u64,
    },
    /// Read current account state without starting provider work.
    Current {
        /// Authorized account.
        capability: UsageAccountCapability,
    },
    /// Request or join a refresh generation.
    Refresh {
        /// Authorized account.
        capability: UsageAccountCapability,
        /// Last generation observed by the caller.
        observed_generation: u64,
        /// True only for an explicit operator Refresh action.
        force: bool,
    },
    /// Wait for a named generation to become terminal.
    Join {
        /// Authorized account.
        capability: UsageAccountCapability,
        /// Generation returned by a prior refresh request.
        generation: u64,
        /// Bounded client wait in milliseconds.
        timeout_ms: u64,
    },
}

/// Versioned request envelope with a build handshake.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageBrokerRequest {
    /// [`USAGE_BROKER_PROTOCOL_VERSION`].
    pub protocol_version: String,
    /// Exact host build identifier.
    pub build_id: String,
    /// Requested operation.
    pub operation: UsageBrokerOperation,
    /// Host-staged credential proof. The Capsule cannot choose this value:
    /// the host relay replaces any request-supplied scope with its immutable
    /// launch scope before forwarding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_credential_scope: Option<UsageCredentialScope>,
}

/// Multiplexed request carried by the host-started container stdio tunnel.
/// The tunnel is already scoped to one container; account authorization still
/// happens against that tunnel's immutable host-side capability allowlist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageRelayTunnelRequest {
    /// Process-local request identifier used only to route the response.
    pub request_id: u64,
    /// Unmodified broker request emitted by a Capsule client.
    pub request: UsageBrokerRequest,
}

/// Versioned broker response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UsageBrokerResponse {
    /// Operation succeeded or joined an active generation.
    State {
        /// Current generation projection.
        state: Box<UsageGenerationView>,
    },
    /// Immutable canonical projection publication.
    Projection {
        /// Current surface-neutral projection.
        projection: Box<UsageProjectionV1>,
    },
    /// Operation failed before provider dispatch.
    Error {
        /// Typed sanitized failure.
        error: UsageCoordinationError,
    },
}

/// Multiplexed response returned through the container stdio tunnel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageRelayTunnelResponse {
    /// Identifier from [`UsageRelayTunnelRequest`].
    pub request_id: u64,
    /// Sanitized broker or authorization result.
    pub response: UsageBrokerResponse,
}

#[cfg(test)]
mod tests;

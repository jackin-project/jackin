// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Versioned, secret-free usage-broker wire records.

use serde::{Deserialize, Serialize};

use crate::control::FocusedUsageView;

/// Usage-broker wire protocol version.
pub const USAGE_BROKER_PROTOCOL_VERSION: &str = "v1";

/// Maximum newline-delimited request or response body.
pub const USAGE_BROKER_MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Opaque authority for one canonical provider account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UsageAccountCapability {
    /// Host-generated opaque canonical account identifier.
    pub account_id: String,
    /// Closed Rust-owned provider surface identifier.
    pub surface_id: String,
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

/// Semantic state of one provider-supplied quota window.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageQuotaStateV1 {
    /// Quota is available.
    Available,
    /// Provider explicitly reports that the window has not started.
    NotStarted,
    /// Quota is below the Rust-owned warning threshold.
    Warning,
    /// Quota is exhausted.
    Exhausted,
    /// Window semantics are unsupported.
    Unsupported,
    /// Window is temporarily unavailable.
    Unavailable,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_percent: Option<UsagePercent>,
    /// Used quota when the provider reports a used representation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_percent: Option<UsagePercent>,
    /// Reset time in UTC Unix seconds when known.
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

impl UsageLimitWindowV1 {
    /// Validate cross-field representation invariants.
    pub fn validate(&self, expected_rank: usize) -> Result<(), String> {
        if usize::try_from(self.rank).ok() != Some(expected_rank) {
            return Err(format!("window {} has noncanonical rank", self.window_id));
        }
        if self.remaining_percent.is_some() && self.used_percent.is_some() {
            return Err(format!(
                "usage window {} has conflicting percent representations",
                self.window_id
            ));
        }
        Ok(())
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
    /// Provider/source-ordered quota windows.
    pub windows: Vec<UsageLimitWindowV1>,
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
    /// Relay-only current-state request for one known provider surface.
    /// The per-container relay resolves this to exactly one allowed capability;
    /// the global host broker rejects this operation directly.
    CurrentForSurface {
        /// Closed provider surface id already known to the Capsule.
        surface_id: String,
    },
    /// Relay-only refresh request for one known provider surface.
    RefreshForSurface {
        /// Closed provider surface id already known to the Capsule.
        surface_id: String,
        /// Last generation observed by the caller.
        observed_generation: u64,
        /// True only for an explicit operator Refresh action.
        force: bool,
    },
    /// Relay-only wait for one surface generation.
    JoinForSurface {
        /// Closed provider surface id already known to the Capsule.
        surface_id: String,
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

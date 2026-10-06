// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Broker-owned refresh cadence and retry policy.

use std::time::Duration;

use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCoordinationErrorKind};

/// Operator activity used to select the automatic refresh cadence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageActivity {
    /// A surface is actively being used.
    DirectInteraction,
    /// The operator interacted recently.
    Recent,
    /// No recent interaction.
    Idle,
    /// Long-idle or Low Power Mode.
    LongIdle,
}

/// Frozen broker timing constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsagePolicy {
    /// Broker exits after this idle interval.
    pub idle_exit: Duration,
    /// Lease lifetime.
    pub lease_duration: Duration,
    /// Lease renewal interval.
    pub lease_renewal: Duration,
    /// Provider hard timeout.
    pub provider_timeout: Duration,
    /// Minimum retry backoff.
    pub retry_base: Duration,
    /// Maximum retry backoff.
    pub retry_cap: Duration,
}

impl Default for UsagePolicy {
    fn default() -> Self {
        Self {
            idle_exit: Duration::from_mins(10),
            lease_duration: Duration::from_secs(30),
            lease_renewal: Duration::from_secs(10),
            provider_timeout: Duration::from_secs(30),
            retry_base: Duration::from_secs(30),
            retry_cap: Duration::from_mins(15),
        }
    }
}

/// Select the automatic refresh cadence. Low Power Mode always uses long idle.
#[must_use]
pub const fn cadence(activity: UsageActivity, low_power: bool) -> Duration {
    if low_power {
        return Duration::from_mins(30);
    }
    match activity {
        UsageActivity::DirectInteraction => Duration::from_mins(2),
        UsageActivity::Recent => Duration::from_mins(5),
        UsageActivity::Idle => Duration::from_mins(15),
        UsageActivity::LongIdle => Duration::from_mins(30),
    }
}

/// Whether a terminal failure should receive broker-owned transient retry.
#[must_use]
pub const fn is_retryable(kind: UsageCoordinationErrorKind) -> bool {
    matches!(
        kind,
        UsageCoordinationErrorKind::Unavailable
            | UsageCoordinationErrorKind::OwnerLost
            | UsageCoordinationErrorKind::ProviderTimeout
            | UsageCoordinationErrorKind::ProviderUnavailable
            | UsageCoordinationErrorKind::RateLimited
    )
}

/// Deterministic full-jitter retry deadline. The capability and generation seed
/// it, so joined callers never derive different retry times.
#[must_use]
pub fn retry_deadline(
    policy: UsagePolicy,
    capability: &UsageAccountCapability,
    generation: u64,
    failures: u32,
    provider_deadline: Option<i64>,
    finished_at_epoch: i64,
) -> Option<i64> {
    if failures == 0 {
        return provider_deadline;
    }
    let shift = failures.saturating_sub(1).min(8);
    let exponential = policy
        .retry_base
        .checked_mul(1u32 << shift)
        .unwrap_or(policy.retry_cap)
        .min(policy.retry_cap);
    let seed = account_key_hash_seed(capability, generation, failures);
    let span = exponential.as_secs().saturating_add(1);
    let jitter = seed % span;
    let fallback = finished_at_epoch.saturating_add(i64::try_from(jitter).unwrap_or(i64::MAX));
    Some(provider_deadline.map_or(fallback, |deadline| deadline.max(fallback)))
}

fn account_key_hash_seed(
    capability: &UsageAccountCapability,
    generation: u64,
    failures: u32,
) -> u64 {
    let digest = jackin_core::account_key_hash(
        "usage-retry-v1",
        &format!("{}:{}:{}", capability.account_id, generation, failures),
    );
    digest
        .chars()
        .filter_map(|character| character.to_digit(16))
        .take(16)
        .fold(0u64, |value, nibble| {
            value.wrapping_mul(16).wrapping_add(u64::from(nibble))
        })
}

#[cfg(test)]
mod tests;

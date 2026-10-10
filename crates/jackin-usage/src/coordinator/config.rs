// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Coordinator configuration.

use std::collections::BTreeSet;

use std::time::Duration;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind,
};

use super::{coordination_error, policy};

/// Coordinator scheduling policy.
#[derive(Debug, Clone, Copy)]
pub struct UsageCoordinatorConfig {
    /// Maximum number of distinct account probes that may run concurrently.
    pub max_concurrency: usize,
    /// Ambient success cooldown.
    pub success_cooldown: Duration,
    /// Probe deadline used for terminal classification after the worker returns.
    pub provider_timeout: Duration,
    /// Maximum queued account generations.
    pub queue_capacity: usize,
    /// Broker-owned adaptive retry/jitter policy.
    pub retry_policy: policy::UsagePolicy,
}

impl Default for UsageCoordinatorConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            success_cooldown: Duration::from_mins(5),
            provider_timeout: Duration::from_secs(30),
            queue_capacity: 256,
            retry_policy: policy::UsagePolicy::default(),
        }
    }
}

/// Exact capability allowlist used by a per-container relay.
#[derive(Debug, Clone, Default)]
pub struct UsageCapabilitySet {
    allowed: BTreeSet<UsageAccountCapability>,
}

impl UsageCapabilitySet {
    /// Build an immutable exact-account allowlist.
    #[must_use]
    pub fn new(capabilities: impl IntoIterator<Item = UsageAccountCapability>) -> Self {
        Self {
            allowed: capabilities.into_iter().collect(),
        }
    }

    /// Reject an account absent from the launch-derived capability set.
    pub fn authorize(
        &self,
        capability: &UsageAccountCapability,
    ) -> Result<(), UsageCoordinationError> {
        if self.allowed.contains(capability) {
            Ok(())
        } else {
            Err(coordination_error(
                UsageCoordinationErrorKind::Unauthorized,
                "usage account capability is not authorized",
            ))
        }
    }

    /// Resolve a provider surface only when this scope authorizes exactly one
    /// canonical account. Missing and ambiguous mappings are equally denied.
    pub fn resolve_surface(
        &self,
        surface_id: &str,
    ) -> Result<UsageAccountCapability, UsageCoordinationError> {
        let mut matches = self
            .allowed
            .iter()
            .filter(|capability| capability.surface_id == surface_id);
        let Some(capability) = matches.next() else {
            return Err(coordination_error(
                UsageCoordinationErrorKind::Unauthorized,
                "usage provider surface is not authorized",
            ));
        };
        if matches.next().is_some() {
            return Err(coordination_error(
                UsageCoordinationErrorKind::Unauthorized,
                "usage provider surface is not uniquely authorized",
            ));
        }
        Ok(capability.clone())
    }

    /// Number of authorized canonical accounts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.allowed.len()
    }

    /// Whether no account capability is authorized.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty()
    }
}

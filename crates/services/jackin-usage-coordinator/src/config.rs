// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use std::time::Duration;

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind,
};

use super::{coordination_error, policy};

#[derive(Debug, Clone, Copy)]
pub struct UsageCoordinatorConfig {
    pub max_concurrency: usize,

    pub success_cooldown: Duration,

    pub provider_timeout: Duration,

    pub queue_capacity: usize,

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

#[derive(Debug, Clone, Default)]
pub struct UsageCapabilitySet {
    allowed: BTreeSet<UsageAccountCapability>,
}

impl UsageCapabilitySet {
    #[must_use]
    pub fn new(capabilities: impl IntoIterator<Item = UsageAccountCapability>) -> Self {
        Self {
            allowed: capabilities.into_iter().collect(),
        }
    }

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

    #[must_use]
    pub fn len(&self) -> usize {
        self.allowed.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty()
    }
}

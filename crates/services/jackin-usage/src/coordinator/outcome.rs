// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Probe outcomes and executor trait.

use jackin_protocol::control::FocusedUsageView;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageCredentialScope,
};

pub(crate) const TERMINAL_HISTORY_LIMIT: usize = 8;

/// Result returned by a host-owned provider adapter.
#[derive(Debug, Clone)]
pub enum ProviderProbeOutcome {
    /// Data-bearing provider result.
    Success(Box<FocusedUsageView>),
    /// Typed failure that preserves last-good quota.
    Failure {
        /// Stable failure kind.
        kind: UsageCoordinationErrorKind,
        /// Sanitized operator-facing message.
        message: String,
        /// Provider-supplied retry deadline, when present.
        retry_at_epoch: Option<i64>,
    },
}

impl ProviderProbeOutcome {
    /// Wrap one data-bearing provider result without exposing wire-size details.
    #[must_use]
    pub fn success(view: FocusedUsageView) -> Self {
        Self::Success(Box::new(view))
    }
}

/// Configurable provider execution port.
pub trait UsageProviderExecutor: Send + Sync {
    /// Execute one canonical account probe. Implementations own bounded network
    /// timeouts; the coordinator retains generation ownership until this call
    /// actually returns.
    fn probe(&self, capability: &UsageAccountCapability, generation: u64) -> ProviderProbeOutcome;

    /// Execute one launch-scoped probe with immutable source proof. The
    /// default preserves source compatibility for executors that do not need
    /// launch-specific routing.
    fn probe_scoped(
        &self,
        capability: &UsageAccountCapability,
        generation: u64,
        _scope: &UsageCredentialScope,
    ) -> ProviderProbeOutcome {
        self.probe(capability, generation)
    }

    /// Reconcile provider bindings before a new catalog revision can start
    /// work. A failed reconciliation does not admit the new catalog.
    fn reconcile_catalog(
        &self,
        _entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        Ok(())
    }

    /// Reconcile provider bindings against a complete caller catalog. The
    /// default keeps older in-process executors source-compatible; broker
    /// executors that can rediscover credentials should compare both values.
    fn reconcile_catalog_revision(
        &self,
        _catalog_revision: &str,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        self.reconcile_catalog(entries)
    }

    /// Validate a catalog without changing provider bindings.
    ///
    /// The publisher calls this before touching coordinator durable state.
    /// Implementations that need external discovery or binding allocation can
    /// reject here and retain the previous catalog unchanged.
    fn validate_catalog(
        &self,
        _entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        Ok(())
    }

    /// Validate a complete caller catalog before durable or in-memory
    /// mutation. The default delegates to the legacy entry-only hook.
    fn validate_catalog_revision(
        &self,
        _catalog_revision: &str,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        self.validate_catalog(entries)
    }

    /// Authorize one launch-scoped source proof before provider work starts.
    ///
    /// Executors without source inventory fail closed. The production
    /// discovery executor overrides this with exact declaration/material
    /// matching; test doubles must opt into authorization explicitly.
    fn authorize_credential_scope(
        &self,
        _capability: &UsageAccountCapability,
        _scope: &UsageCredentialScope,
    ) -> Result<(), UsageCoordinationError> {
        Err(UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unauthorized,
            message: "launch credential source authorization unavailable".to_owned(),
        })
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::FocusedUsageView;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageCredentialScope,
};

pub(crate) const TERMINAL_HISTORY_LIMIT: usize = 8;

#[derive(Debug, Clone)]
pub enum ProviderProbeOutcome {
    Success(Box<FocusedUsageView>),
    Failure {
        kind: UsageCoordinationErrorKind,
        message: String,
        retry_at_epoch: Option<i64>,
    },
}

impl ProviderProbeOutcome {
    #[must_use]
    pub fn success(view: FocusedUsageView) -> Self {
        Self::Success(Box::new(view))
    }
}

pub trait UsageProviderExecutor: Send + Sync {
    fn probe(&self, capability: &UsageAccountCapability, generation: u64) -> ProviderProbeOutcome;

    fn probe_scoped(
        &self,
        capability: &UsageAccountCapability,
        generation: u64,
        _scope: &UsageCredentialScope,
    ) -> ProviderProbeOutcome {
        self.probe(capability, generation)
    }

    fn reconcile_catalog(
        &self,
        _entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        Ok(())
    }

    fn reconcile_catalog_revision(
        &self,
        _catalog_revision: &str,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        self.reconcile_catalog(entries)
    }

    fn validate_catalog(
        &self,
        _entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        Ok(())
    }

    fn validate_catalog_revision(
        &self,
        _catalog_revision: &str,
        entries: &[UsageCatalogEntry],
    ) -> Result<(), UsageCoordinationError> {
        self.validate_catalog(entries)
    }

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

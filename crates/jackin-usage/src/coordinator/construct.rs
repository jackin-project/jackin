// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Coordinator construction and catalog install.

use std::collections::BTreeMap;
use std::sync::mpsc::{self};
use std::sync::{Arc, Condvar, Mutex};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCatalogEntry, UsageCoordinationError, UsageCredentialScope,
};

use super::{
    AccountStateStore, CoordinatorState, MonotonicClock, Shared, SystemMonotonicClock,
    UsageCoordinator, UsageCoordinatorConfig, UsageProviderExecutor, coordinator_worker,
};

impl UsageCoordinator {
    /// Start a bounded coordinator worker pool.
    #[must_use]
    pub fn new(
        executor: Arc<dyn UsageProviderExecutor>,
        store: Arc<dyn AccountStateStore>,
        config: UsageCoordinatorConfig,
    ) -> Self {
        Self::start(executor, store, config, None, None)
    }

    /// Verify a launch-scoped credential proof against the executor's current
    /// source binding before admitting a provider operation.
    pub fn authorize_credential_scope(
        &self,
        capability: &UsageAccountCapability,
        scope: &UsageCredentialScope,
    ) -> Result<(), UsageCoordinationError> {
        self.shared
            .executor
            .authorize_credential_scope(capability, scope)
    }

    /// Start a coordinator with the catalog from the last durable broker
    /// publication. The broker reconciles it with current discovery before
    /// admitting new work.
    #[must_use]
    pub fn with_catalog(
        executor: Arc<dyn UsageProviderExecutor>,
        store: Arc<dyn AccountStateStore>,
        config: UsageCoordinatorConfig,
        catalog: impl IntoIterator<Item = UsageCatalogEntry>,
    ) -> Self {
        let catalog = catalog
            .into_iter()
            .map(|entry| (entry.capability, entry.revision))
            .collect();
        Self::start(executor, store, config, Some(catalog), None)
    }

    /// Start a coordinator with the exact revision persisted beside the
    /// catalog. Broker recovery uses this so executor rollback is fenced by
    /// the same caller/service revision as the forward rotation.
    #[must_use]
    pub fn with_catalog_revision(
        executor: Arc<dyn UsageProviderExecutor>,
        store: Arc<dyn AccountStateStore>,
        config: UsageCoordinatorConfig,
        catalog: impl IntoIterator<Item = UsageCatalogEntry>,
        catalog_revision: String,
    ) -> Self {
        let catalog = catalog
            .into_iter()
            .map(|entry| (entry.capability, entry.revision))
            .collect();
        Self::start(
            executor,
            store,
            config,
            Some(catalog),
            Some(catalog_revision),
        )
    }

    pub(crate) fn start(
        executor: Arc<dyn UsageProviderExecutor>,
        store: Arc<dyn AccountStateStore>,
        config: UsageCoordinatorConfig,
        catalog: Option<BTreeMap<UsageAccountCapability, String>>,
        catalog_revision: Option<String>,
    ) -> Self {
        Self::start_with_clock(
            executor,
            store,
            config,
            catalog,
            catalog_revision,
            Arc::new(SystemMonotonicClock::default()),
        )
    }

    pub(crate) fn start_with_clock(
        executor: Arc<dyn UsageProviderExecutor>,
        store: Arc<dyn AccountStateStore>,
        config: UsageCoordinatorConfig,
        catalog: Option<BTreeMap<UsageAccountCapability, String>>,
        catalog_revision: Option<String>,
        clock: Arc<dyn MonotonicClock>,
    ) -> Self {
        let config = UsageCoordinatorConfig {
            max_concurrency: config.max_concurrency.max(1),
            queue_capacity: config.queue_capacity.max(1),
            ..config
        };
        let shared = Arc::new(Shared {
            state: Mutex::new(CoordinatorState {
                catalog,
                catalog_revision,
                ..CoordinatorState::default()
            }),
            catalog_lifecycle: Mutex::new(()),
            changed: Condvar::new(),
            executor,
            store,
            config,
            clock,
            #[cfg(test)]
            before_provider_call_hook: Mutex::new(None),
        });
        let (jobs, receiver) = mpsc::sync_channel(config.queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(config.max_concurrency);
        for index in 0..config.max_concurrency {
            let worker_shared = Arc::clone(&shared);
            let worker_receiver = Arc::clone(&receiver);
            let name = format!("usage-coordinator-{index}");
            match jackin_telemetry::spawn::thread_joined_named(name, move || {
                coordinator_worker(&worker_shared, &worker_receiver);
            }) {
                Ok(worker) => workers.push(worker),
                Err(_) => break,
            }
        }
        Self {
            shared,
            jobs,
            workers,
        }
    }
}

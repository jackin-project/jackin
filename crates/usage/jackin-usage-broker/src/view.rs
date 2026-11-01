// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageGenerationView,
};

use crate::UsageBrokerClient;

impl UsageBrokerClient {
    #[must_use]
    pub fn subscribe_all(
        &self,
        capabilities: impl IntoIterator<Item = UsageAccountCapability>,
    ) -> Vec<(
        UsageAccountCapability,
        Result<UsageGenerationView, UsageCoordinationError>,
    )> {
        let mut results = Vec::new();
        for capability in capabilities
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
        {
            let result = self.subscribe(capability.clone());
            results.push((capability, result));
        }
        results
    }

    pub fn subscribe(
        &self,
        capability: UsageAccountCapability,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        let observed = self.current(capability.clone()).map_or_else(
            |_| self.observed_generation(&capability).unwrap_or(0),
            |view| view.generation,
        );
        match self.refresh(capability.clone(), observed, false) {
            Ok(view) => {
                self.record_observed(&capability, view.generation);
                Ok(view)
            }
            Err(error) => {
                self.record_observed_fallback(&capability, observed);
                Err(error)
            }
        }
    }

    pub fn unsubscribe(&self, capability: &UsageAccountCapability) -> bool {
        self.subscriptions
            .lock()
            .is_ok_and(|mut subscriptions| subscriptions.remove(capability).is_some())
    }

    pub fn unsubscribe_all(&self) {
        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            subscriptions.clear();
        }
    }

    #[must_use]
    pub fn refresh_due(
        &self,
        force: bool,
    ) -> Vec<(
        UsageAccountCapability,
        Result<UsageGenerationView, UsageCoordinationError>,
    )> {
        let mut results = Vec::new();
        for capability in self.subscriptions() {
            let observed = self.observed_generation(&capability).unwrap_or(0);
            let result = self.refresh(capability.clone(), observed, force);
            if let Ok(view) = &result {
                self.record_observed(&capability, view.generation);
            }
            results.push((capability, result));
        }
        results
    }

    #[must_use]
    pub fn subscriptions(&self) -> Vec<UsageAccountCapability> {
        self.subscriptions
            .lock()
            .map(|subscriptions| subscriptions.keys().cloned().collect())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn observed_generation(&self, capability: &UsageAccountCapability) -> Option<u64> {
        self.subscriptions
            .lock()
            .ok()
            .and_then(|subscriptions| subscriptions.get(capability).copied())
    }

    fn record_observed(&self, capability: &UsageAccountCapability, generation: u64) {
        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            subscriptions.insert(capability.clone(), generation);
        }
    }

    fn record_observed_fallback(&self, capability: &UsageAccountCapability, generation: u64) {
        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            subscriptions
                .entry(capability.clone())
                .or_insert(generation);
        }
    }
}

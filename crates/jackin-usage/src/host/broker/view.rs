// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Client-side broker subscription set for monitoring screens.
//!
//! A screen subscribes to the accounts it renders and the client remembers
//! the last generation observed for each. All provider work stays
//! broker-owned: subscribing requests or joins broker generations, while
//! unsubscribing only releases local interest. There is no cancel path —
//! dropping a client or unsubscribing one account never cancels a generation
//! another client owns or awaits. A client that stops waiting simply stops
//! observing; broker ownership always runs to terminal.
//!
//! Cloning a client forks its subscription set: the clone starts with the
//! same observed generations but later (un)subscribes diverge, so two screens
//! never share one subscription.

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageGenerationView,
};

use super::UsageBrokerClient;

impl UsageBrokerClient {
    /// Screen-open path: subscribe to every enabled account and request due
    /// observations through the broker.
    ///
    /// Each account is handled independently: one account's failure is
    /// reported alongside the others, never as a batch abort. Duplicate
    /// capabilities are requested once. Requests use `force: false`, so a
    /// still-fresh observation is reused and an active generation is joined;
    /// duplicates are never forced.
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

    /// Subscribe to one account: read cached state, then request freshness
    /// with `force: false` (reuse still-fresh data, join active work).
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

    /// Release local interest in one account.
    ///
    /// This performs no broker I/O: it cannot cancel a generation another
    /// client owns or awaits. Returns whether the account was subscribed.
    pub fn unsubscribe(&self, capability: &UsageAccountCapability) -> bool {
        self.subscriptions
            .lock()
            .is_ok_and(|mut subscriptions| subscriptions.remove(capability).is_some())
    }

    /// Release local interest in every subscribed account.
    ///
    /// Equivalent to dropping the client: prompt, infallible, and without
    /// broker I/O, so broker-owned generations always run to terminal.
    pub fn unsubscribe_all(&self) {
        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            subscriptions.clear();
        }
    }

    /// Heartbeat path: request due observations for every subscribed account.
    ///
    /// Pass `force: true` only for an explicit operator refresh; periodic
    /// monitoring always passes `false` so broker cadence and retry deadlines
    /// win. One account's failure never blocks the others.
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

    /// Currently subscribed capabilities in settled order.
    #[must_use]
    pub fn subscriptions(&self) -> Vec<UsageAccountCapability> {
        self.subscriptions
            .lock()
            .map(|subscriptions| subscriptions.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Last generation observed for one account, if subscribed.
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

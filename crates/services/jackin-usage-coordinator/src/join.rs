// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Generation join and load methods.

use std::time::{Duration, Instant};

use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageGenerationView, UsageRefreshPhase,
};

use super::{
    AccountEntry, AccountStateEnvelope, ProbeJob, UsageCoordinator, catalog_revoked_error,
    coordination_error, finish_failure, generation_view, policy, record_blocked_terminal,
    state_error, unavailable_error,
};

impl UsageCoordinator {
    /// Wait for one named generation. A wait timeout never changes ownership.
    pub fn join_generation(
        &self,
        capability: &UsageAccountCapability,
        generation: u64,
        timeout: Duration,
        now_epoch: i64,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        {
            let _catalog_lifecycle = self
                .shared
                .catalog_lifecycle
                .lock()
                .map_err(|_| unavailable_error())?;
            self.ensure_loaded(capability, now_epoch)?;
        }
        let deadline = Instant::now() + timeout;
        loop {
            let catalog_lifecycle = self
                .shared
                .catalog_lifecycle
                .lock()
                .map_err(|_| unavailable_error())?;
            let state = self.shared.state.lock().map_err(|_| unavailable_error())?;
            if let Some(error) = state.blocked.get(capability) {
                return Err(error.clone());
            }
            let entry = state
                .accounts
                .get(capability)
                .ok_or_else(unavailable_error)?;
            if entry.revoked {
                return Err(catalog_revoked_error());
            }
            if entry.fenced_generations.contains(&generation) {
                return Err(catalog_revoked_error());
            }
            if let Some(terminal) = entry
                .history
                .iter()
                .find(|view| view.generation == generation)
            {
                return Ok(terminal.clone());
            }
            if entry.envelope.generation == generation && entry.envelope.phase.is_terminal() {
                return Ok(generation_view(&entry.envelope));
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(coordination_error(
                    UsageCoordinationErrorKind::WaitTimeout,
                    "usage refresh is still updating",
                ));
            };
            drop(catalog_lifecycle);
            let (next_state, wait) = self
                .shared
                .changed
                .wait_timeout(state, remaining)
                .map_err(|_| unavailable_error())?;
            drop(next_state);
            if wait.timed_out() {
                return Err(coordination_error(
                    UsageCoordinationErrorKind::WaitTimeout,
                    "usage refresh is still updating",
                ));
            }
        }
    }

    pub(crate) fn ensure_loaded(
        &self,
        capability: &UsageAccountCapability,
        now_epoch: i64,
    ) -> Result<(), UsageCoordinationError> {
        {
            let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
            if let Some(error) = state.blocked.get(capability).cloned() {
                let Some(envelope) = state
                    .accounts
                    .get(capability)
                    .map(|entry| entry.envelope.clone())
                else {
                    return Err(error);
                };
                if self.shared.store.store(&envelope, now_epoch).is_err() {
                    return Err(error);
                }
                state.blocked.remove(capability);
                record_blocked_terminal(&mut state, capability, &envelope)?;
                self.shared.changed.notify_all();
            }
            if state.accounts.contains_key(capability) {
                return Ok(());
            }
            if state
                .catalog
                .as_ref()
                .is_some_and(|catalog| !catalog.contains_key(capability))
            {
                return Err(catalog_revoked_error());
            }
        }
        let loaded = self.shared.store.load(capability, now_epoch);
        let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        if state.accounts.contains_key(capability) {
            return Ok(());
        }
        if state
            .catalog
            .as_ref()
            .is_some_and(|catalog| !catalog.contains_key(capability))
        {
            return Err(catalog_revoked_error());
        }
        let catalog_revision = state
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.get(capability).cloned());
        match loaded {
            Ok(envelope) => {
                let mut envelope =
                    envelope.unwrap_or_else(|| AccountStateEnvelope::idle(capability.clone()));
                let recovery_pending = envelope.phase.is_active();
                if recovery_pending {
                    let consecutive_failures = envelope.consecutive_failures.saturating_add(1);
                    envelope.phase = UsageRefreshPhase::Failed;
                    envelope.terminal_result = None;
                    envelope.terminal_error = Some(coordination_error(
                        UsageCoordinationErrorKind::OwnerLost,
                        "usage refresh owner exited before completion",
                    ));
                    envelope.completed_at_epoch = Some(now_epoch);
                    let retry_deadline = policy::retry_deadline(
                        self.shared.config.retry_policy,
                        &envelope.capability,
                        envelope.generation,
                        consecutive_failures,
                        envelope.retry_deadline_epoch,
                        now_epoch,
                    );
                    envelope.retry_deadline_epoch = match policy::minimum_attempt_deadline(
                        &envelope.capability,
                        envelope.started_at_epoch,
                    ) {
                        Some(floor) => {
                            Some(retry_deadline.map_or(floor, |deadline| deadline.max(floor)))
                        }
                        None => retry_deadline,
                    };
                    envelope.success_deadline_epoch = None;
                    envelope.consecutive_failures = consecutive_failures;
                    if self.shared.store.store(&envelope, now_epoch).is_err() {
                        let error = unavailable_error();
                        state.blocked.insert(capability.clone(), error.clone());
                        return Err(error);
                    }
                }
                state.accounts.insert(
                    capability.clone(),
                    AccountEntry::new(envelope, recovery_pending, now_epoch, catalog_revision),
                );
                Ok(())
            }
            Err(error) => {
                let error = state_error(error);
                state.blocked.insert(capability.clone(), error.clone());
                Err(error)
            }
        }
    }

    pub(crate) fn fail_without_probe(
        &self,
        job: &ProbeJob,
        kind: UsageCoordinationErrorKind,
        message: &str,
        now_epoch: i64,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        finish_failure(&self.shared, job, kind, message, None, now_epoch);
        self.current(&job.capability, now_epoch)
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Refresh request methods.

use std::collections::BTreeMap;
use std::sync::mpsc::TrySendError;

use super::policy::{self, UsageActivity};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageCredentialScope, UsageGenerationView, UsageRefreshPhase,
};

use super::{
    ProbeJob, UsageCoordinator, WorkerMessage, account_cooldown_deadline, cadence_deadline,
    catalog_revoked_error, generation_view, unavailable_error,
};

impl UsageCoordinator {
    /// Read current state without dispatching provider work.
    pub fn current(
        &self,
        capability: &UsageAccountCapability,
        now_epoch: i64,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        let _catalog_lifecycle = self
            .shared
            .catalog_lifecycle
            .lock()
            .map_err(|_| unavailable_error())?;
        self.ensure_loaded(capability, now_epoch)?;
        let state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        if let Some(error) = state.blocked.get(capability) {
            return Err(error.clone());
        }
        let entry = state
            .accounts
            .get(capability)
            .ok_or_else(unavailable_error)?;
        // Read-only access retains the materialized last-good view after
        // revocation. New work and new loads are fenced below; an existing
        // session may keep displaying its immutable materialized result until
        // it explicitly stops or recreates.
        Ok(generation_view(&entry.envelope))
    }

    /// Start or join one generation. A stale observed generation always adopts
    /// the winner and cannot queue a second force refresh.
    pub fn request_refresh(
        &self,
        capability: &UsageAccountCapability,
        observed_generation: u64,
        force: bool,
        now_epoch: i64,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.request_refresh_with_scope(capability, observed_generation, force, now_epoch, None)
    }

    /// Start one refresh whose provider work must use the supplied immutable
    /// launch source proof. The proof travels with the generation job so a
    /// later sibling binding cannot authorize a different refresh authority.
    pub fn request_refresh_scoped(
        &self,
        capability: &UsageAccountCapability,
        observed_generation: u64,
        force: bool,
        now_epoch: i64,
        credential_scope: UsageCredentialScope,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        self.request_refresh_with_scope(
            capability,
            observed_generation,
            force,
            now_epoch,
            Some(credential_scope),
        )
    }

    pub(crate) fn request_refresh_with_scope(
        &self,
        capability: &UsageAccountCapability,
        observed_generation: u64,
        force: bool,
        now_epoch: i64,
        credential_scope: Option<UsageCredentialScope>,
    ) -> Result<UsageGenerationView, UsageCoordinationError> {
        let catalog_lifecycle = self
            .shared
            .catalog_lifecycle
            .lock()
            .map_err(|_| unavailable_error())?;
        self.ensure_loaded(capability, now_epoch)?;
        let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        if let Some(error) = state.blocked.get(capability) {
            return Err(error.clone());
        }
        let entry = state
            .accounts
            .get_mut(capability)
            .ok_or_else(unavailable_error)?;
        if entry.revoked {
            return Err(catalog_revoked_error());
        }
        if entry.envelope.phase.is_active()
            || (!entry.recovery_pending && observed_generation < entry.envelope.generation)
        {
            return Ok(generation_view(&entry.envelope));
        }
        if entry
            .envelope
            .rate_limit_deadline_epoch
            .is_some_and(|deadline| deadline > now_epoch)
            || entry
                .envelope
                .retry_deadline_epoch
                .is_some_and(|deadline| deadline > now_epoch)
            || policy::minimum_attempt_deadline(capability, entry.envelope.started_at_epoch)
                .is_some_and(|deadline| deadline > now_epoch)
            || (!force
                && entry
                    .envelope
                    .success_deadline_epoch
                    .is_some_and(|deadline| deadline > now_epoch))
        {
            return Ok(generation_view(&entry.envelope));
        }

        let previous = entry.envelope.clone();
        let recovery_pending = entry.recovery_pending;
        entry.recovery_pending = false;
        entry.envelope.generation = entry.envelope.generation.saturating_add(1);
        entry.envelope.phase = UsageRefreshPhase::Queued;
        entry.envelope.started_at_epoch = Some(now_epoch);
        entry.envelope.completed_at_epoch = None;
        entry.envelope.terminal_result = None;
        entry.envelope.terminal_error = None;
        entry.envelope.retry_deadline_epoch = None;
        let generation = entry.envelope.generation;
        if self.shared.store.store(&entry.envelope, now_epoch).is_err() {
            entry.envelope = previous;
            entry.recovery_pending = recovery_pending;
            let error = unavailable_error();
            state.blocked.insert(capability.clone(), error.clone());
            return Err(error);
        }
        let queued = generation_view(&entry.envelope);
        let catalog_revision = entry.catalog_revision.clone();
        drop(state);

        let job = ProbeJob {
            capability: capability.clone(),
            generation,
            started_at_epoch: now_epoch,
            catalog_revision,
            credential_scope,
        };
        drop(catalog_lifecycle);
        match self.jobs.try_send(WorkerMessage::Probe(job)) {
            Ok(()) => Ok(queued),
            Err(TrySendError::Full(message) | TrySendError::Disconnected(message)) => match message
            {
                WorkerMessage::Probe(job) => self.fail_without_probe(
                    &job,
                    UsageCoordinationErrorKind::Unavailable,
                    "usage coordinator queue is unavailable",
                    now_epoch,
                ),
                WorkerMessage::Shutdown => Err(unavailable_error()),
            },
        }
    }

    /// Issue one request per unique canonical account.
    pub fn request_refresh_all(
        &self,
        requests: impl IntoIterator<Item = (UsageAccountCapability, u64)>,
        force: bool,
        now_epoch: i64,
    ) -> Vec<Result<UsageGenerationView, UsageCoordinationError>> {
        let unique = requests.into_iter().collect::<BTreeMap<_, _>>();
        unique
            .into_iter()
            .map(|(capability, observed)| {
                self.request_refresh(&capability, observed, force, now_epoch)
            })
            .collect()
    }

    /// Select the periodic cadence tier for one account. A due account stays
    /// due; otherwise the next poll moves earlier when the new cadence is
    /// shorter. Never dispatches provider work.
    pub fn set_activity(
        &self,
        capability: &UsageAccountCapability,
        activity: UsageActivity,
        low_power: bool,
        now_epoch: i64,
    ) -> Result<(), UsageCoordinationError> {
        let _catalog_lifecycle = self
            .shared
            .catalog_lifecycle
            .lock()
            .map_err(|_| unavailable_error())?;
        self.ensure_loaded(capability, now_epoch)?;
        let mut state = self.shared.state.lock().map_err(|_| unavailable_error())?;
        if let Some(error) = state.blocked.get(capability) {
            return Err(error.clone());
        }
        let entry = state
            .accounts
            .get_mut(capability)
            .ok_or_else(unavailable_error)?;
        if entry.revoked {
            return Err(catalog_revoked_error());
        }
        entry.cadence.activity = activity;
        entry.cadence.low_power = low_power;
        let deadline = cadence_deadline(
            activity,
            low_power,
            capability,
            entry.envelope.generation,
            now_epoch,
        );
        let next_due = entry.cadence.next_due_epoch.min(deadline);
        entry.cadence.next_due_epoch = account_cooldown_deadline(&entry.envelope)
            .filter(|shared_deadline| *shared_deadline > now_epoch)
            .map_or(next_due, |shared_deadline| shared_deadline.max(next_due));
        Ok(())
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Poll cadence methods.

use std::collections::BTreeSet;

use jackin_protocol::usage_broker::{UsageAccountCapability, UsageGenerationView};

use super::entries::effective_due_epoch;
use super::{UsageCoordinator, account_cooldown_deadline, cadence_deadline};

impl UsageCoordinator {
    /// Earliest periodic due time across known accounts, for scheduler sleep.
    /// `None` when no account is tracked yet.
    #[must_use]
    pub fn next_due_epoch(&self) -> Option<i64> {
        let clock_sample = self.shared.clock.sample(0);
        let _catalog_lifecycle = self.shared.catalog_lifecycle.lock().ok()?;
        self.shared.state.lock().ok().and_then(|state| {
            state
                .accounts
                .values()
                .map(|entry| effective_due_epoch(entry, clock_sample))
                .min()
        })
    }

    /// Earliest periodic due time among the caller-selected capabilities.
    /// When a catalog is installed, each selected account is resolved through
    /// that catalog; omitted capabilities cannot wake this scheduler.
    #[must_use]
    pub fn next_due_epoch_for_capabilities(
        &self,
        capabilities: impl IntoIterator<Item = UsageAccountCapability>,
        now_epoch: i64,
    ) -> Option<i64> {
        let selected = capabilities.into_iter().collect::<BTreeSet<_>>();
        for capability in &selected {
            drop(self.current(capability, now_epoch));
        }
        let clock_sample = self.shared.clock.sample(now_epoch);
        let _catalog_lifecycle = self.shared.catalog_lifecycle.lock().ok()?;
        self.shared.state.lock().ok().and_then(|state| {
            state
                .accounts
                .iter()
                .filter(|(capability, entry)| {
                    selected.contains(*capability)
                        && !entry.revoked
                        && !state.blocked.contains_key(*capability)
                })
                .map(|(_, entry)| effective_due_epoch(entry, clock_sample))
                .min()
        })
    }

    /// Poll every account whose periodic cadence is due. Each due account
    /// issues at most one ambient (non-force) refresh, which joins in-flight
    /// work and honors shared Retry-After/cooldown deadlines, so one call can
    /// never produce a burst of missed polls. Returns the started or joined
    /// views; blocked accounts are skipped.
    pub fn poll_due(&self, now_epoch: i64) -> Vec<UsageGenerationView> {
        self.poll_due_filtered(now_epoch, None)
    }

    /// Poll due accounts only from the caller-selected capability set. This
    /// is the scheduler path for explicitly opted-in collectors: a loaded but
    /// unselected account stays untouched even when its cadence is overdue.
    /// An installed catalog still rejects selected capabilities it does not
    /// admit.
    /// Each selected account follows the same single-flight and shared
    /// cooldown admission path as [`UsageCoordinator::poll_due`].
    pub fn poll_due_for_capabilities(
        &self,
        capabilities: impl IntoIterator<Item = UsageAccountCapability>,
        now_epoch: i64,
    ) -> Vec<UsageGenerationView> {
        let selected = capabilities.into_iter().collect::<BTreeSet<_>>();
        if selected.is_empty() {
            return Vec::new();
        }
        for capability in &selected {
            drop(self.current(capability, now_epoch));
        }
        self.poll_due_filtered(now_epoch, Some(&selected))
    }

    fn poll_due_filtered(
        &self,
        now_epoch: i64,
        selected: Option<&BTreeSet<UsageAccountCapability>>,
    ) -> Vec<UsageGenerationView> {
        let clock_sample = self.shared.clock.sample(now_epoch);
        let due: Vec<(UsageAccountCapability, u64)> = {
            let Ok(_catalog_lifecycle) = self.shared.catalog_lifecycle.lock() else {
                return Vec::new();
            };
            let Ok(state) = self.shared.state.lock() else {
                return Vec::new();
            };
            state
                .accounts
                .iter()
                .filter_map(|(capability, entry)| {
                    if selected.is_some_and(|selected| !selected.contains(capability))
                        || entry.revoked
                        || state.blocked.contains_key(capability)
                    {
                        return None;
                    }
                    let next_due = effective_due_epoch(entry, clock_sample);
                    (now_epoch >= next_due).then(|| (capability.clone(), entry.envelope.generation))
                })
                .collect()
        };
        let mut views = Vec::with_capacity(due.len());
        for (capability, observed) in due {
            let Ok(view) = self.request_refresh(&capability, observed, false, now_epoch) else {
                continue;
            };
            self.advance_cadence(&capability, observed, now_epoch);
            views.push(view);
        }
        views
    }

    /// Recalculate due times after sleep/wake or network reconnection. Every
    /// missed due time becomes one jittered cadence deadline from now, so the
    /// next [`UsageCoordinator::poll_due`] issues at most one poll per
    /// account. Future due times are untouched. Returns the number of
    /// recalculated accounts. Never dispatches provider work.
    pub fn note_wake(&self, now_epoch: i64) -> usize {
        let Ok(_catalog_lifecycle) = self.shared.catalog_lifecycle.lock() else {
            return 0;
        };
        let Ok(mut state) = self.shared.state.lock() else {
            return 0;
        };
        let mut recalculated = 0;
        for (capability, entry) in &mut state.accounts {
            if entry.cadence.next_due_epoch < now_epoch {
                let cadence_due = cadence_deadline(
                    entry.cadence.activity,
                    entry.cadence.low_power,
                    capability,
                    entry.envelope.generation,
                    now_epoch,
                );
                entry.cadence.next_due_epoch = account_cooldown_deadline(&entry.envelope)
                    .filter(|deadline| *deadline > now_epoch)
                    .map_or(cadence_due, |deadline| deadline.max(cadence_due));
                recalculated += 1;
            }
        }
        recalculated
    }

    pub(crate) fn advance_cadence(
        &self,
        capability: &UsageAccountCapability,
        _observed_generation: u64,
        now_epoch: i64,
    ) {
        let Ok(_catalog_lifecycle) = self.shared.catalog_lifecycle.lock() else {
            return;
        };
        let Ok(mut state) = self.shared.state.lock() else {
            return;
        };
        let Some(entry) = state.accounts.get_mut(capability) else {
            return;
        };
        let cadence_due = cadence_deadline(
            entry.cadence.activity,
            entry.cadence.low_power,
            capability,
            entry.envelope.generation,
            now_epoch,
        );
        entry.cadence.next_due_epoch = account_cooldown_deadline(&entry.envelope)
            .filter(|deadline| *deadline > now_epoch)
            .map_or(cadence_due, |deadline| deadline.max(cadence_due));
    }

    /// Whether no queued or active generation is retained by this authority.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        let Ok(_catalog_lifecycle) = self.shared.catalog_lifecycle.lock() else {
            return false;
        };
        self.shared.state.lock().is_ok_and(|state| {
            state
                .accounts
                .values()
                .all(|entry| !entry.envelope.phase.is_active())
        })
    }
}

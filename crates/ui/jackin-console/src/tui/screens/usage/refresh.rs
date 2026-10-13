// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage refresh lifecycle.

use super::{USAGE_HEARTBEAT_INTERVAL, UsageRefreshOutcome, UsageRefreshRequest, UsageScreenState};
use std::time::Instant;

use crate::tui::runtime::{BlockingSubscription, SubscriptionPoll};

impl UsageScreenState {
    /// Apply a completed background refresh, re-anchoring selection by
    /// stable id so renames/reorders keep the operator's row. A removed
    /// selection falls back to Overview with an inline notice.
    pub fn apply_refresh(&mut self, snapshot: Self, now: Instant) {
        self.accounts = snapshot.accounts;
        self.notice = snapshot.notice;
        self.canonical_projection = snapshot.canonical_projection;
        self.generated_at_epoch = snapshot.generated_at_epoch;
        self.projection_issues = snapshot.projection_issues;
        self.last_refresh_at = Some(now);
        self.consume_refresh_intent();
        // A refresh replaces the list: old offsets are meaningless and a
        // stale deep offset would blank the panes until the operator
        // scrolled back (same rule as `reanchor_after_view_change`).
        self.scroll = 0;
        match &self.selected_id {
            None => self.selected = 0,
            Some(id) => {
                if let Some(pos) = self
                    .visible_order()
                    .iter()
                    .position(|&index| self.accounts[index].stable_id() == *id)
                {
                    self.selected = pos.saturating_add(1);
                } else if self.accounts.iter().any(|a| &a.stable_id() == id) {
                    // Still configured but hidden by the current filter: park
                    // on Overview and keep the id so clearing the filter (or
                    // the next refresh) restores the row without a notice.
                    self.selected = 0;
                } else {
                    self.selected = 0;
                    self.selected_id = None;
                    self.notice = Some(match self.notice.take() {
                        Some(notice) => format!(
                            "{notice} · previously selected account unavailable; showing Overview"
                        ),
                        None => {
                            "Previously selected account unavailable; showing Overview".to_owned()
                        }
                    });
                }
            }
        }
        self.selected = self.selected.min(self.visible_order().len());
    }

    /// Record a failed background refresh. The timer still advances so a
    /// broken broker retries at heartbeat cadence (or on manual `r`),
    /// never once per keypress.
    pub fn apply_refresh_error(&mut self, notice: String, now: Instant) {
        self.notice = Some(notice);
        self.last_refresh_at = Some(now);
        self.consume_refresh_intent();
    }

    #[must_use]
    pub fn refresh_in_flight(&self) -> bool {
        self.refresh_rx.is_some()
    }

    /// True while an empty route is still waiting on broker work: a refresh
    /// is either in flight or due (the open path marks one due before the
    /// worker starts). Empty branches render the loading line in this case
    /// instead of claiming no providers are configured.
    #[must_use]
    pub fn loading(&self) -> bool {
        self.refreshing() || self.refresh_due
    }

    #[must_use]
    pub fn refreshing(&self) -> bool {
        self.refresh_in_flight()
            || self
                .canonical_projection
                .as_ref()
                .is_some_and(|projection| {
                    projection.refresh_state
                        == jackin_protocol::usage_broker::UsageProjectionRefreshStateV1::Refreshing
                })
    }

    /// Claim the next due refresh, if any, following the instance-refresh
    /// throttle shape: at most one generation is ever in flight, and a
    /// requester arriving while one runs joins that shared work instead of
    /// queueing a duplicate. Claiming consumes `refresh_due` and the pending
    /// force flag; the worker must tag its outcome with the generation via
    /// [`Self::begin_refresh`].
    pub fn next_refresh_plan_if_due(&mut self, now: Instant) -> Option<UsageRefreshRequest> {
        if self.refresh_in_flight() {
            self.consume_refresh_intent();
            return None;
        }
        if !self.refresh_due && !self.heartbeat_due(now) {
            return None;
        }
        self.refresh_generation = self.refresh_generation.wrapping_add(1);
        let force = self.consume_refresh_intent();
        Some(UsageRefreshRequest {
            generation: self.refresh_generation,
            force,
        })
    }

    // Joining, dispatching, and adopting a completed cycle all consume the
    // pending request together. A force bit never survives a cleared due bit.
    pub(crate) fn consume_refresh_intent(&mut self) -> bool {
        self.refresh_due = false;
        std::mem::take(&mut self.force_refresh_pending)
    }

    pub fn begin_refresh(&mut self, rx: BlockingSubscription<(u64, UsageRefreshOutcome)>) {
        self.refresh_rx = Some(Box::new(rx));
    }

    /// Poll the in-flight refresh once. `None` means still running — or that
    /// the completed generation was stale and its outcome was dropped.
    pub fn poll_refresh(&mut self) -> Option<UsageRefreshOutcome> {
        let rx = self.refresh_rx.as_mut()?;
        match rx.poll_next() {
            SubscriptionPoll::Ready((generation, outcome)) => {
                self.refresh_rx = None;
                if generation == self.refresh_generation {
                    Some(outcome)
                } else {
                    None
                }
            }
            SubscriptionPoll::Closed => {
                self.refresh_rx = None;
                Some(Err("usage refresh worker disconnected".to_owned()))
            }
            SubscriptionPoll::Pending => None,
        }
    }

    /// True once the last completed refresh is older than the heartbeat
    /// interval. Never true before the first completion: the open path
    /// drives that via `refresh_due`.
    #[must_use]
    pub fn heartbeat_due(&self, now: Instant) -> bool {
        self.last_refresh_at
            .is_some_and(|at| now.duration_since(at) >= USAGE_HEARTBEAT_INTERVAL)
    }
}

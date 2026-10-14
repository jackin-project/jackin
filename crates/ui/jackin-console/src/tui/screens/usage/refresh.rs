// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{USAGE_HEARTBEAT_INTERVAL, UsageRefreshOutcome, UsageRefreshRequest, UsageScreenState};
use std::time::Instant;

use crate::tui::runtime::{BlockingSubscription, SubscriptionPoll};

impl UsageScreenState {
    pub fn apply_refresh(&mut self, snapshot: Self, now: Instant) {
        self.accounts = snapshot.accounts;
        self.notice = snapshot.notice;
        self.canonical_projection = snapshot.canonical_projection;
        self.generated_at_epoch = snapshot.generated_at_epoch;
        self.projection_issues = snapshot.projection_issues;
        self.last_refresh_at = Some(now);
        self.consume_refresh_intent();
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

    pub fn apply_refresh_error(&mut self, notice: String, now: Instant) {
        self.notice = Some(notice);
        self.last_refresh_at = Some(now);
        self.consume_refresh_intent();
    }

    #[must_use]
    pub fn refresh_in_flight(&self) -> bool {
        self.refresh_rx.is_some()
    }

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

    pub(crate) fn consume_refresh_intent(&mut self) -> bool {
        self.refresh_due = false;
        std::mem::take(&mut self.force_refresh_pending)
    }

    pub fn begin_refresh(&mut self, rx: BlockingSubscription<(u64, UsageRefreshOutcome)>) {
        self.refresh_rx = Some(Box::new(rx));
    }

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

    #[must_use]
    pub fn heartbeat_due(&self, now: Instant) -> bool {
        self.last_refresh_at
            .is_some_and(|at| now.duration_since(at) >= USAGE_HEARTBEAT_INTERVAL)
    }
}

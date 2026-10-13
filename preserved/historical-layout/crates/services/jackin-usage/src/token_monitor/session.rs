// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Token monitor sessions.

use std::time::Instant;

use jackin_core::Agent;

use super::{
    PollStatus, TokenTotals, amp, claude, codex, kimi, opencode, pricing, record_token_usage,
};

/// Per-session token monitor state.
#[derive(Debug)]
pub struct TokenSession {
    pub agent: Agent,
    pub totals: TokenTotals,
    /// Last rowid seen in `SQLite` (for `OpenCode` incremental reads).
    pub last_rowid: i64,
    /// Time of last poll.
    pub last_polled: Instant,
    /// Consecutive polls with no new data (for back-off).
    pub silent_polls: u32,
}

impl TokenSession {
    pub fn new(agent: Agent) -> Self {
        Self {
            agent,
            totals: TokenTotals::default(),
            last_rowid: 0,
            last_polled: Instant::now(),
            silent_polls: 0,
        }
    }

    /// Poll interval considering back-off.
    /// Base: 30s; after 5 consecutive silent polls: 60s.
    pub fn poll_interval_secs(&self) -> u64 {
        if self.silent_polls >= 5 { 60 } else { 30 }
    }

    /// Returns true if a poll is due.
    pub fn poll_due(&self) -> bool {
        self.last_polled.elapsed().as_secs() >= self.poll_interval_secs()
    }

    /// Poll for new token data while preserving adapter degradation.
    pub(crate) async fn poll(&mut self) -> PollStatus {
        self.last_polled = Instant::now();
        let previous = self.totals.clone();
        let changed = match self.agent {
            Agent::Claude => claude::poll_session(self),
            Agent::Codex => codex::poll_session(self),
            Agent::Kimi => kimi::poll_session(self),
            // OpenCode reads SQLite via async turso.
            Agent::Opencode => opencode::poll_session(self).await,
            Agent::Amp => amp::poll_session(self),
            // No token-spend reader for Grok yet.
            Agent::Grok => PollStatus::Unchanged,
            // No token-spend readers for the catalog additions yet.
            Agent::Antigravity
            | Agent::Gemini
            | Agent::Cursor
            | Agent::Muse
            | Agent::Omp
            | Agent::Hermes => PollStatus::Unchanged,
        };
        if changed == PollStatus::Changed {
            self.silent_polls = 0;
            // Fill cost from the static pricing table when the provider's own
            // stream did not carry a precomputed cost. Key on the wire model when
            // present, else the agent slug (so e.g. Kimi, which carries no model,
            // still prices off its `kimi` row).
            if self.totals.cost_usd.is_none() {
                let model = self.totals.model.as_deref().unwrap_or(self.agent.slug());
                self.totals.cost_usd = pricing::estimate_cost_usd(
                    model,
                    self.totals.input_tokens,
                    self.totals.output_tokens,
                    self.totals.cache_read_tokens,
                    self.totals.cache_write_tokens,
                );
            }
            record_token_usage(self.agent, &previous, &self.totals);
        } else if changed == PollStatus::Unchanged {
            self.silent_polls = self.silent_polls.saturating_add(1);
        }
        changed
    }
}

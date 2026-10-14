// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use jackin_core::Agent;

use super::{
    PollStatus, TokenTotals, amp, claude, codex, kimi, opencode, pricing, record_token_usage,
};

#[derive(Debug)]
pub struct TokenSession {
    pub agent: Agent,
    pub totals: TokenTotals,

    pub last_rowid: i64,

    pub last_polled: Instant,

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

    pub fn poll_interval_secs(&self) -> u64 {
        if self.silent_polls >= 5 { 60 } else { 30 }
    }

    pub fn poll_due(&self) -> bool {
        self.last_polled.elapsed().as_secs() >= self.poll_interval_secs()
    }

    pub(crate) async fn poll(&mut self) -> PollStatus {
        self.last_polled = Instant::now();
        let previous = self.totals.clone();
        let changed = match self.agent {
            Agent::Claude => claude::poll_session(self),
            Agent::Codex => codex::poll_session(self),
            Agent::Kimi => kimi::poll_session(self),

            Agent::Opencode => opencode::poll_session(self).await,
            Agent::Amp => amp::poll_session(self),

            Agent::Grok => PollStatus::Unchanged,

            Agent::Antigravity
            | Agent::Gemini
            | Agent::Cursor
            | Agent::Muse
            | Agent::Omp
            | Agent::Hermes => PollStatus::Unchanged,
        };
        if changed == PollStatus::Changed {
            self.silent_polls = 0;

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

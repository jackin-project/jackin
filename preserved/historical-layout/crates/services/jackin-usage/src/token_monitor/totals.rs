// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Token totals and spend accumulation.

use std::time::SystemTime;

use jackin_protocol::control::TokenUsageSummary;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PollReport {
    pub attempted: usize,
    pub changed: usize,
    pub degraded: usize,
}

/// Aggregated token totals for one session.
#[derive(Debug, Clone, Default)]
pub struct TokenTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// Pre-calculated cost when the provider stream provides it directly, or the
    /// pricing-table estimate filled in after a poll.
    pub cost_usd: Option<f64>,
    /// Most recently used model in this session.
    pub model: Option<String>,
    /// Start of the current 5-hour billing window (Claude-specific).
    pub window_start: Option<SystemTime>,
}

/// Recompute accumulator shared by the sum-per-message adapters (Claude, Kimi,
/// Amp). Each poll re-reads the provider logs whole and folds every message's
/// usage in here, then `commit`s the result by SET (never `+=`), so a re-read
/// never double-counts. (Codex keeps its own accumulator — its wire format is a
/// monotonic cumulative, not a per-message sum.)
#[derive(Debug, Default)]
pub struct SpendAcc {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub cost: f64,
    pub has_cost: bool,
    pub model: Option<String>,
    pub seen: bool,
}

impl SpendAcc {
    /// Write this recomputed pass onto `totals` by assignment (never addition),
    /// so polling the same logs twice yields the same totals. A model/cost is
    /// only written when this pass actually resolved one, so a model-less pass
    /// never clobbers a previously-resolved model. Returns whether anything moved.
    pub fn commit(self, totals: &mut TokenTotals) -> bool {
        let cost = self.has_cost.then_some(self.cost);
        let changed = self.input != totals.input_tokens
            || self.output != totals.output_tokens
            || self.cache_read != totals.cache_read_tokens
            || self.cache_write != totals.cache_write_tokens
            || (cost.is_some() && cost != totals.cost_usd);
        if changed {
            totals.input_tokens = self.input;
            totals.output_tokens = self.output;
            totals.cache_read_tokens = self.cache_read;
            totals.cache_write_tokens = self.cache_write;
            if cost.is_some() {
                totals.cost_usd = cost;
            }
            if self.model.is_some() {
                totals.model = self.model;
            }
        }
        changed
    }
}

impl TokenTotals {
    pub fn to_summary(&self) -> TokenUsageSummary {
        TokenUsageSummary {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_read_tokens: self.cache_read_tokens,
            cache_write_tokens: self.cache_write_tokens,
            cost_usd: self.cost_usd,
            model: self.model.clone(),
        }
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Per-session token-spend monitor.
//!
//! Reads provider-specific local `JSONL` / `SQLite` files inside the container and
//! tracks per-session input/output/cache token totals (and cost, from the
//! provider stream or the static pricing table). Polled from the daemon state
//! tick (self-throttled to a 30s/60s cadence); totals are served on demand
//! through the `ClientMsg::TokenUsage` control reply.

pub mod amp;
pub mod claude;
pub mod codex;
pub mod kimi;
pub mod opencode;
pub mod pricing;

use std::collections::HashMap;
use std::path::Path;
use std::time::{Instant, SystemTime};

use jackin_core::agent::Agent;

use jackin_protocol::control::TokenUsageSummary;

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

    /// Poll for new token data. Returns `true` when totals changed.
    pub async fn poll(&mut self) -> bool {
        self.last_polled = Instant::now();
        let changed = match self.agent {
            Agent::Claude => claude::poll_session(self),
            Agent::Codex => codex::poll_session(self),
            Agent::Kimi => kimi::poll_session(self),
            // OpenCode reads SQLite via async turso.
            Agent::Opencode => opencode::poll_session(self).await,
            Agent::Amp => amp::poll_session(self),
            // No token-spend reader for Grok yet.
            Agent::Grok => false,
        };
        if changed {
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
        } else {
            self.silent_polls = self.silent_polls.saturating_add(1);
        }
        changed
    }
}

/// Read a `u64` field from a JSON object, defaulting to 0 when the key is
/// absent or not an unsigned integer. Folds the `get(..).and_then(as_u64)
/// .unwrap_or(0)` chain repeated across every provider's usage parse.
pub fn json_u64(v: &serde_json::Value, key: &str) -> u64 {
    v.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0)
}

/// Default recursion bound for providers that nest session logs (Claude one
/// level `projects/<dir>/*.jsonl`, Codex three `sessions/YYYY/MM/DD/*.jsonl`);
/// also a guard against a pathological tree.
pub const PROVIDER_WALK_DEPTH: usize = 8;

/// Walk `base_dirs` up to `max_depth` levels deep and return every file with
/// extension `ext`. `max_depth = 0` reads only the top level of each base dir
/// (Amp's flat `threads/*.json`); deeper providers pass [`PROVIDER_WALK_DEPTH`].
/// A missing or unreadable directory yields no files, never an error.
pub fn find_provider_files(
    base_dirs: &[&str],
    ext: &str,
    max_depth: usize,
) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    let mut stack: Vec<(std::path::PathBuf, usize)> = base_dirs
        .iter()
        .map(|b| (Path::new(b).to_owned(), 0))
        .collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if depth < max_depth {
                    stack.push((p, depth + 1));
                }
            } else if p.extension().and_then(|e| e.to_str()) == Some(ext) {
                paths.push(p);
            }
        }
    }
    paths
}

/// Read a file in full for a recompute pass. Token logs are re-read whole each
/// poll (adapters recompute totals from scratch), avoiding the per-file
/// byte-offset bookkeeping that silently double-counted across globbed files.
///
/// `Ok(None)` is an absent file (expected — the agent simply has not written it).
/// `Err` is a real IO failure (permission, transient): the caller must NOT treat
/// that as "the file is empty" and recompute a smaller total — under the SET
/// model that would silently regress a monotonic counter. Callers abort the
/// recompute on `Err` and keep the prior totals instead.
pub fn read_file_text(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Recompute a session's spend by reading every provider file whole and folding
/// each file's text into a `SpendAcc` via `fold`. This is the outer shape every
/// adapter shares; only `fold` (the per-file parse) differs.
///
/// Returns `None` — meaning "do not change the session totals" — in both the
/// no-file-contributed case and on a real read failure (which is logged with
/// `label` + path). Both collapse to the same caller action: keep the prior
/// totals rather than SET a partial recompute that could regress a monotonic
/// counter. `Some(acc)` is a complete pass the caller commits.
pub fn recompute_spend(
    files: &[std::path::PathBuf],
    label: &str,
    mut fold: impl FnMut(&str, &mut SpendAcc),
) -> Option<SpendAcc> {
    let mut acc = SpendAcc::default();
    for path in files {
        match read_file_text(path) {
            Ok(Some(text)) => fold(&text, &mut acc),
            Ok(None) => {}
            Err(e) => {
                crate::cdebug!("token monitor: {label} read {path:?} failed: {e}");
                return None;
            }
        }
    }
    acc.seen.then_some(acc)
}

/// The token monitor manages per-session polling.
#[derive(Debug, Default)]
pub struct TokenMonitor {
    sessions: HashMap<u64, TokenSession>,
}

impl TokenMonitor {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Register a new session for monitoring.
    pub fn register_session(&mut self, session_id: u64, agent: Agent) {
        self.sessions.insert(session_id, TokenSession::new(agent));
    }

    /// Deregister a session when it exits.
    pub fn deregister_session(&mut self, session_id: u64) {
        self.sessions.remove(&session_id);
    }

    /// Reconcile the tracked set against the currently live agent sessions:
    /// register any newly-seen `(id, agent)` and drop any that have exited.
    /// One robust sync point beats hooking every session spawn/close site.
    pub fn reconcile_sessions(&mut self, live: &[(u64, Agent)]) {
        let live_ids: std::collections::HashSet<u64> = live.iter().map(|(id, _)| *id).collect();
        for &(id, agent) in live {
            if !self.sessions.contains_key(&id) {
                self.register_session(id, agent);
            }
        }
        let stale: Vec<u64> = self
            .sessions
            .keys()
            .copied()
            .filter(|id| !live_ids.contains(id))
            .collect();
        for id in stale {
            self.deregister_session(id);
        }
    }

    /// Poll all sessions that are due. Returns session IDs whose totals changed.
    pub async fn poll_due_sessions(&mut self) -> Vec<u64> {
        let due: Vec<u64> = self
            .sessions
            .iter()
            .filter(|(_, session)| session.poll_due())
            .map(|(id, _)| *id)
            .collect();
        let mut changed = Vec::new();
        for id in due {
            if let Some(session) = self.sessions.get_mut(&id)
                && session.poll().await
            {
                changed.push(id);
            }
        }
        changed
    }

    /// Get current totals for a session.
    pub fn totals(&self, session_id: u64) -> Option<&TokenTotals> {
        self.sessions.get(&session_id).map(|s| &s.totals)
    }

    #[cfg(test)]
    pub fn contains_session(&self, session_id: u64) -> bool {
        self.sessions.contains_key(&session_id)
    }
}

#[cfg(test)]
mod tests;

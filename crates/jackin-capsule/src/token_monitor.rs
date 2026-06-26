//! Per-session token-spend monitor.
//!
//! Reads provider-specific local `JSONL` / `SQLite` files inside the container and
//! tracks per-session input/output/cache token totals (and cost, from the
//! provider stream or the static pricing table). Polled from the daemon state
//! tick (self-throttled to a 30s/60s cadence); totals are served on demand
//! through the `ClientMsg::TokenUsage` control reply.

pub(crate) mod amp;
pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod kimi;
pub(crate) mod opencode;
pub(crate) mod pricing;

use std::collections::HashMap;
use std::path::Path;
use std::time::{Instant, SystemTime};

use jackin_core::agent::Agent;

use jackin_protocol::control::TokenUsageSummary;

/// Aggregated token totals for one session.
#[derive(Debug, Clone, Default)]
pub(crate) struct TokenTotals {
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) cache_read_tokens: u64,
    pub(crate) cache_write_tokens: u64,
    /// Pre-calculated cost when the provider stream provides it directly, or the
    /// pricing-table estimate filled in after a poll.
    pub(crate) cost_usd: Option<f64>,
    /// Most recently used model in this session.
    pub(crate) model: Option<String>,
    /// Start of the current 5-hour billing window (Claude-specific).
    pub(crate) window_start: Option<SystemTime>,
}

impl TokenTotals {
    pub(crate) fn to_summary(&self) -> TokenUsageSummary {
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
pub(crate) struct TokenSession {
    pub(crate) agent: Agent,
    pub(crate) totals: TokenTotals,
    /// Last rowid seen in `SQLite` (for `OpenCode` incremental reads).
    pub(crate) last_rowid: i64,
    /// Time of last poll.
    pub(crate) last_polled: Instant,
    /// Consecutive polls with no new data (for back-off).
    pub(crate) silent_polls: u32,
}

impl TokenSession {
    pub(crate) fn new(agent: Agent) -> Self {
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
    pub(crate) fn poll_interval_secs(&self) -> u64 {
        if self.silent_polls >= 5 { 60 } else { 30 }
    }

    /// Returns true if a poll is due.
    pub(crate) fn poll_due(&self) -> bool {
        self.last_polled.elapsed().as_secs() >= self.poll_interval_secs()
    }

    /// Poll for new token data. Returns `true` when totals changed.
    pub(crate) async fn poll(&mut self) -> bool {
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
                let slug = self.agent.slug();
                let model = self.totals.model.as_deref().unwrap_or(slug);
                self.totals.cost_usd = pricing::estimate_cost_usd(
                    slug,
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

/// Recursively walk `base_dirs` and return every file with extension `ext`.
/// Providers nest session logs at varying depths — Claude one level
/// (`projects/<dir>/*.jsonl`), Codex three (`sessions/YYYY/MM/DD/*.jsonl`) — so
/// the walk is fully recursive (bounded by `MAX_WALK_DEPTH` against a pathological
/// tree). A missing or unreadable directory yields no files, never an error.
pub(crate) fn find_provider_files(base_dirs: &[&str], ext: &str) -> Vec<std::path::PathBuf> {
    const MAX_WALK_DEPTH: usize = 8;
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
                if depth < MAX_WALK_DEPTH {
                    stack.push((p, depth + 1));
                }
            } else if p.extension().and_then(|e| e.to_str()) == Some(ext) {
                paths.push(p);
            }
        }
    }
    paths
}

/// Read a file in full, returning its text. Token logs are re-read whole on each
/// poll (the adapters recompute totals from scratch); this avoids the per-file
/// byte-offset bookkeeping that silently double-counted when one shared offset
/// was applied across multiple globbed files.
pub(crate) fn read_file_text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// The token monitor manages per-session polling.
#[derive(Debug, Default)]
pub(crate) struct TokenMonitor {
    sessions: HashMap<u64, TokenSession>,
}

impl TokenMonitor {
    pub(crate) fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Register a new session for monitoring.
    pub(crate) fn register_session(&mut self, session_id: u64, agent: Agent) {
        self.sessions.insert(session_id, TokenSession::new(agent));
    }

    /// Deregister a session when it exits.
    pub(crate) fn deregister_session(&mut self, session_id: u64) {
        self.sessions.remove(&session_id);
    }

    /// Reconcile the tracked set against the currently live agent sessions:
    /// register any newly-seen `(id, agent)` and drop any that have exited.
    /// One robust sync point beats hooking every session spawn/close site.
    pub(crate) fn reconcile_sessions(&mut self, live: &[(u64, Agent)]) {
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
    pub(crate) async fn poll_due_sessions(&mut self) -> Vec<u64> {
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
    pub(crate) fn totals(&self, session_id: u64) -> Option<&TokenTotals> {
        self.sessions.get(&session_id).map(|s| &s.totals)
    }

    #[cfg(test)]
    pub(crate) fn contains_session(&self, session_id: u64) -> bool {
        self.sessions.contains_key(&session_id)
    }
}

#[cfg(test)]
mod tests;

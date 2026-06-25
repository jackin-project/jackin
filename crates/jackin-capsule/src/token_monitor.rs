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
    pub(crate) agent: String,
    pub(crate) totals: TokenTotals,
    /// Last byte offset read in the JSONL file (for incremental reads).
    pub(crate) file_offset: u64,
    /// Last rowid seen in `SQLite` (for `OpenCode` incremental reads).
    pub(crate) last_rowid: i64,
    /// Time of last poll.
    pub(crate) last_polled: Instant,
    /// Consecutive polls with no new data (for back-off).
    pub(crate) silent_polls: u32,
}

impl TokenSession {
    pub(crate) fn new(agent: &str) -> Self {
        Self {
            agent: agent.to_owned(),
            totals: TokenTotals::default(),
            file_offset: 0,
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
        let changed = match self.agent.as_str() {
            "claude" => claude::poll_session(self),
            "codex" => codex::poll_session(self),
            "kimi" => kimi::poll_session(self),
            // OpenCode reads SQLite via async turso.
            "opencode" => opencode::poll_session(self).await,
            "amp" => amp::poll_session(self),
            _ => false,
        };
        if changed {
            self.silent_polls = 0;
            // Fill cost from the static pricing table when the provider's own
            // stream did not carry a precomputed cost.
            if self.totals.cost_usd.is_none()
                && let Some(model) = self.totals.model.clone()
            {
                self.totals.cost_usd = pricing::estimate_cost_usd(
                    &self.agent,
                    &model,
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

/// Walk `base_dirs` and return all files with extension `ext` found either as
/// direct children of each base directory or one level deeper (for providers
/// that nest files inside a per-session subdirectory).
pub(crate) fn find_provider_files(base_dirs: &[&str], ext: &str) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    for &base in base_dirs {
        let Ok(dir) = std::fs::read_dir(base) else {
            continue;
        };
        for session in dir.flatten() {
            let sp = session.path();
            if sp.extension().and_then(|e| e.to_str()) == Some(ext) {
                paths.push(sp);
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&sp) else {
                continue;
            };
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some(ext) {
                    paths.push(p);
                }
            }
        }
    }
    paths
}

pub(crate) fn read_new_text(path: &Path, offset: &mut u64) -> Option<(String, u64)> {
    let content = std::fs::read_to_string(path).ok()?;
    let len = content.len() as u64;
    let start = if *offset <= len {
        *offset as usize
    } else {
        crate::cdebug!(
            "token monitor: offset {} beyond len {} for {:?}, resetting",
            *offset,
            len,
            path
        );
        *offset = 0;
        0
    };
    Some((content[start..].to_owned(), len))
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
    pub(crate) fn register_session(&mut self, session_id: u64, agent: &str) {
        self.sessions.insert(session_id, TokenSession::new(agent));
    }

    /// Deregister a session when it exits.
    pub(crate) fn deregister_session(&mut self, session_id: u64) {
        self.sessions.remove(&session_id);
    }

    /// Reconcile the tracked set against the currently live agent sessions:
    /// register any newly-seen `(id, agent)` and drop any that have exited.
    /// One robust sync point beats hooking every session spawn/close site.
    pub(crate) fn reconcile_sessions(&mut self, live: &[(u64, String)]) {
        let live_ids: std::collections::HashSet<u64> = live.iter().map(|(id, _)| *id).collect();
        for (id, agent) in live {
            if !self.sessions.contains_key(id) {
                self.register_session(*id, agent);
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

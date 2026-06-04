//! Token consumption monitor.
//!
//! Reads provider-specific local JSONL/SQLite/JSON files inside the container
//! and reports per-session token totals through the event stream.
//!
//! Architecture:
//!  - One `TokenSession` per live agent session.
//!  - Polled from the daemon's 30-second ticker.
//!  - Emits `token_usage_changed` events when totals change.
//!  - Caches last-known totals in `/jackin/state/token-monitor/<session_id>.json`.

pub mod amp;
pub mod claude;
pub mod codex;
pub mod kimi;
pub mod models;
pub mod opencode;
pub mod pricing;

use std::collections::HashMap;
use std::time::{Instant, SystemTime};

use jackin_protocol::control::TokenUsageSummary;

/// Aggregated token totals for one session.
#[derive(Debug, Clone, Default)]
pub struct TokenTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// Pre-calculated cost when the JSONL provides it directly.
    pub cost_usd: Option<f64>,
    /// Most recently used model in this session.
    pub model: Option<String>,
    /// Start of the current 5-hour billing window (Claude-specific).
    pub window_start: Option<SystemTime>,
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
pub struct TokenSession {
    pub session_id: u64,
    pub agent: String,
    pub totals: TokenTotals,
    /// Last byte offset read in the JSONL file (for incremental reads).
    pub file_offset: u64,
    /// Last rowid seen in SQLite (for OpenCode incremental reads).
    pub last_rowid: i64,
    /// Time of last poll.
    pub last_polled: Instant,
    /// Consecutive polls with no new data (for back-off).
    pub silent_polls: u32,
}

impl TokenSession {
    pub fn new(session_id: u64, agent: &str) -> Self {
        Self {
            session_id,
            agent: agent.to_string(),
            totals: TokenTotals::default(),
            file_offset: 0,
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
    pub fn poll(&mut self) -> bool {
        self.last_polled = Instant::now();
        let changed = match self.agent.as_str() {
            "claude" => claude::poll_session(self),
            "codex" => codex::poll_session(self),
            "kimi" => kimi::poll_session(self),
            "opencode" => opencode::poll_session(self),
            "amp" => amp::poll_session(self),
            _ => false,
        };
        if changed {
            self.silent_polls = 0;
        } else {
            self.silent_polls = self.silent_polls.saturating_add(1);
        }
        changed
    }
}

/// The token monitor manages per-session polling.
#[derive(Default)]
pub struct TokenMonitor {
    sessions: HashMap<u64, TokenSession>,
}

impl TokenMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new session for monitoring.
    pub fn register_session(&mut self, session_id: u64, agent: &str) {
        self.sessions
            .insert(session_id, TokenSession::new(session_id, agent));
    }

    /// Deregister a session when it exits.
    pub fn deregister_session(&mut self, session_id: u64) {
        self.sessions.remove(&session_id);
    }

    /// Poll all sessions that are due. Returns session IDs whose totals changed.
    pub fn poll_due_sessions(&mut self) -> Vec<u64> {
        let mut changed = Vec::new();
        for (id, session) in self.sessions.iter_mut() {
            if session.poll_due() && session.poll() {
                changed.push(*id);
            }
        }
        changed
    }

    /// Get current totals for a session.
    pub fn totals(&self, session_id: u64) -> Option<&TokenTotals> {
        self.sessions.get(&session_id).map(|s| &s.totals)
    }
}

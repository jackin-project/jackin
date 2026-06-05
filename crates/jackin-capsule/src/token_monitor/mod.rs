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

/// One time-window rate/quota card (e.g. 5-hour session, weekly).
#[derive(Debug, Clone)]
pub struct RateWindow {
    /// Display label: "Session", "Weekly", "Claude Sonnet (weekly)", etc.
    pub label: String,
    /// Usage percentage 0–100.
    pub used_percent: f64,
    /// Window duration in minutes (300 = 5h, 10080 = 1 week).
    pub window_minutes: Option<u32>,
    /// Next reset timestamp.
    pub resets_at: Option<SystemTime>,
    /// Custom reset description e.g. "Resets Monday 3:00 AM".
    pub reset_description: Option<String>,
}

/// Complete token/quota snapshot for one provider in one session.
#[derive(Debug, Clone)]
pub struct ProviderUsageSnapshot {
    pub provider: String,
    pub model: Option<String>,
    /// Session-level quota (5h for Claude, hourly for OpenAI).
    pub primary: Option<RateWindow>,
    /// Weekly quota.
    pub secondary: Option<RateWindow>,
    /// Monthly or model-specific quota.
    pub tertiary: Option<RateWindow>,
    /// Additional per-model breakdowns.
    pub extra_windows: Vec<RateWindow>,
    pub fetched_at: SystemTime,
}

impl ProviderUsageSnapshot {
    /// Build a minimal snapshot from raw token totals (no OAuth quota data).
    pub fn from_totals(provider: &str, totals: &TokenTotals) -> Self {
        Self {
            provider: provider.to_string(),
            model: totals.model.clone(),
            primary: None,
            secondary: None,
            tertiary: None,
            extra_windows: Vec::new(),
            fetched_at: SystemTime::now(),
        }
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

/// Walk `base_dirs` and return all files with extension `ext` found in any
/// immediate subdirectory. Used by per-provider token readers to locate
/// session JSONL files without duplicating the scan logic.
pub(crate) fn find_provider_files(base_dirs: &[&str], ext: &str) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    for &base in base_dirs {
        let Ok(dir) = std::fs::read_dir(base) else { continue };
        for session in dir.flatten() {
            let sp = session.path();
            if sp.extension().and_then(|e| e.to_str()) == Some(ext) {
                paths.push(sp);
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&sp) else { continue };
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

/// Seek a file to `offset`, resetting the offset to 0 on failure.
///
/// The `let _ = file.seek()` pattern silently continues from the wrong
/// position if the seek fails (e.g. on a truncated/rotated log file).
/// This helper logs the failure and resets so the next read starts
/// from the beginning instead of a phantom offset.
pub(crate) fn seek_or_reset(
    file: &mut std::fs::File,
    offset: &mut u64,
    path: &std::path::Path,
) {
    use std::io::{Seek, SeekFrom};
    if file.seek(SeekFrom::Start(*offset)).is_err() {
        crate::cdebug!("token monitor: seek failed for {:?}, resetting offset", path);
        *offset = 0;
        let _ = file.seek(SeekFrom::Start(0));
    }
}

/// The token monitor manages per-session polling.
#[derive(Default)]
pub struct TokenMonitor {
    sessions: HashMap<u64, TokenSession>,
    pub token_snapshots: HashMap<u64, ProviderUsageSnapshot>,
    pub model_catalog: models::ModelCatalog,
}

impl TokenMonitor {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            token_snapshots: HashMap::new(),
            model_catalog: models::ModelCatalog::new(),
        }
    }

    /// Register a new session for monitoring.
    pub fn register_session(&mut self, session_id: u64, agent: &str) {
        self.sessions
            .insert(session_id, TokenSession::new(session_id, agent));
        if self.model_catalog.needs_refresh() {
            self.model_catalog.populate(agent);
        }
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
                let snapshot = ProviderUsageSnapshot::from_totals(&session.agent, &session.totals);
                self.token_snapshots.insert(*id, snapshot);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn token_monitor_backs_off_after_silence() {
        let session = TokenSession::new(1, "claude");
        assert_eq!(session.poll_interval_secs(), 30);
        let mut session2 = TokenSession::new(1, "claude");
        session2.silent_polls = 5;
        assert_eq!(session2.poll_interval_secs(), 60);
    }

    #[test]
    fn token_monitor_resets_backoff_after_change() {
        let mut session = TokenSession::new(1, "claude");
        session.silent_polls = 5;
        assert_eq!(session.poll_interval_secs(), 60);
        session.silent_polls = 0;
        assert_eq!(session.poll_interval_secs(), 30);
    }

    #[test]
    fn token_monitor_poll_due_respects_interval() {
        let mut session = TokenSession::new(1, "claude");
        session.last_polled = Instant::now();
        assert!(!session.poll_due());
    }

    #[test]
    fn session_info_includes_token_usage_when_available() {
        let totals = TokenTotals {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 100,
            cache_write_tokens: 50,
            cost_usd: Some(0.42),
            model: Some("claude-sonnet-4-6".to_string()),
            window_start: None,
        };
        let summary = totals.to_summary();
        assert_eq!(summary.input_tokens, 1000);
        assert_eq!(summary.output_tokens, 500);
        assert_eq!(summary.cost_usd, Some(0.42));
        assert_eq!(summary.model.as_deref(), Some("claude-sonnet-4-6"));
    }
}

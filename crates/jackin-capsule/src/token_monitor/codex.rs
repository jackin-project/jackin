//! JSONL reader for Codex token usage.

use super::TokenSession;

/// Poll Codex JSONL files for new token data.
/// Returns true when totals changed.
pub fn poll_session(session: &mut TokenSession) -> bool {
    // Codex stores sessions in ~/.codex/sessions/**/*.jsonl
    // Each file has events with type "event_msg" containing token_count payloads.
    // Simplified stub — full implementation is a Phase 6 follow-up.
    let _ = session;
    false
}

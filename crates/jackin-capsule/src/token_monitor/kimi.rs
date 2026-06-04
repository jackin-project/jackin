//! JSONL reader for Kimi token usage.

use super::TokenSession;

/// Poll Kimi wire.jsonl files for new token data.
/// Returns true when totals changed.
pub fn poll_session(session: &mut TokenSession) -> bool {
    // Kimi stores sessions in ~/.kimi/sessions/{GROUP_ID}/{SESSION_UUID}/wire.jsonl
    // Simplified stub — full implementation is a Phase 6 follow-up.
    let _ = session;
    false
}

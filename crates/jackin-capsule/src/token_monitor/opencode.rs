//! Token reader for OpenCode (SQLite database).
//!
//! Reads `/home/agent/.local/share/opencode/opencode.db`.
//! Stub implementation — full SQLite reading requires the `rusqlite` crate
//! which is deferred to a follow-up PR.

use super::TokenSession;

/// Poll OpenCode token data.
/// Returns true when totals changed.
pub fn poll_session(session: &mut TokenSession) -> bool {
    // OpenCode stores per-message token fields in SQLite at
    // ~/.local/share/opencode/opencode.db. Fields: input, output, reasoning,
    // cache.read, cache.write, cost (USD).
    // Full implementation requires rusqlite; stub for now.
    let _ = session;
    false
}

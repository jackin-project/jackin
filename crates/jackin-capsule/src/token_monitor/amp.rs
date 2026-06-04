//! Token reader for Amp thread files.
//!
//! Reads `/home/agent/.local/share/amp/threads/*.json`.

use super::TokenSession;

/// Poll Amp thread files for token data.
/// Returns true when totals changed.
pub fn poll_session(session: &mut TokenSession) -> bool {
    // Amp stores per-message usage metadata in thread JSON files at
    // ~/.local/share/amp/threads/*.json.
    // Stub implementation — full parsing is a Phase 6 follow-up.
    let _ = session;
    false
}

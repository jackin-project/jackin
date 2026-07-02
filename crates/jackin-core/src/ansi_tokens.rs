//! Pure ANSI terminal control constants and helpers relocated from
//! `jackin-tui` (ansi module). These have no ratatui or crossterm
//! dependencies and are safe to use from `jackin-runtime` (and below)
//! for host pointer/clipboard signaling.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;

/// OSC 22 cursor-shape escapes. `POINTER_HAND` switches the terminal
/// pointer to the hand/`pointer` shape over a clickable element;
/// `POINTER_DEFAULT` restores it. Shared by every TUI surface so the
/// "this is clickable" cue is identical (terminals without OSC 22 ignore
/// the sequence harmlessly).
pub const POINTER_HAND: &str = "\x1b]22;pointer\x1b\\";
pub const POINTER_DEFAULT: &str = "\x1b]22;default\x1b\\";

/// OSC 52 clipboard-write sequence. Targets the system clipboard (`c`)
/// and uses BEL termination, which is accepted by Ghostty, Kitty, iTerm2,
/// Alacritty, and `WezTerm`. (GNOME Terminal / VTE has historically required
/// ST `\x1b\\` for OSC 52 — keep it off the BEL-supported list until a
/// specific VTE version can be cited.)
#[must_use]
pub fn encode_osc52_clipboard_write(payload: &str) -> Vec<u8> {
    let encoded = BASE64.encode(payload.as_bytes());
    let mut out = Vec::with_capacity(8 + encoded.len());
    out.extend_from_slice(b"\x1b]52;c;");
    out.extend_from_slice(encoded.as_bytes());
    out.extend_from_slice(b"\x07");
    out
}

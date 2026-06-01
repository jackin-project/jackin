use std::io::Write;

use anyhow::{Context, Result};

use crate::tui::app::PointerShape;
use crate::tui::components::status_bar::STATUS_BAR_ROWS;

pub const DEFAULT_ROWS: u16 = 24;
pub const DEFAULT_COLS: u16 = 80;

const MIN_ROWS: u16 = STATUS_BAR_ROWS + 3;
const MIN_COLS: u16 = 3;

pub fn normalize_size(rows: u16, cols: u16) -> (u16, u16) {
    let rows = if rows == 0 { DEFAULT_ROWS } else { rows }.max(MIN_ROWS);
    let cols = if cols == 0 { DEFAULT_COLS } else { cols }.max(MIN_COLS);
    (rows, cols)
}

/// Terminal-reset escape bytes written when the attach client detaches, minus
/// the alternate-screen leave (`?1049l`). The leave is appended only when this
/// client entered its own alternate screen.
const OUTER_TERMINAL_RESET_BASE: &[u8] =
    b"\x1b]22;default\x1b\\\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1007l\x1b[?1004l\x1b[?2004l\x1b[?1l\x1b[<u\x1b[?25h";
const ALTERNATE_SCREEN_LEAVE: &[u8] = b"\x1b[?1049l";

/// True when the host orchestrator owns one continuous alternate screen for the
/// whole launch flow and asked this attach client (via `JACKIN_HOST_ALT_SCREEN`
/// on the `docker exec`) not to toggle its own. Skipping the toggle keeps the
/// flow on a single screen so detaching the capsule does not pop the operator
/// back to the cooked terminal mid-flow.
pub(crate) fn host_owns_alt_screen() -> bool {
    std::env::var_os("JACKIN_HOST_ALT_SCREEN").is_some()
}

fn outer_terminal_reset_sequence() -> Vec<u8> {
    let mut seq = OUTER_TERMINAL_RESET_BASE.to_vec();
    if !host_owns_alt_screen() {
        seq.extend_from_slice(ALTERNATE_SCREEN_LEAVE);
    }
    seq
}

/// Outer-terminal modes owned by the attach client, not by the focused pane.
/// Reassert after attach and focus swaps so a pane that requested legacy X10
/// or press-only mouse tracking cannot downgrade the multiplexer's own input
/// channel. Alternate-scroll (`?1007`) is disabled because some terminals
/// translate wheel gestures in the alternate screen into cursor keys; jackin'
/// needs the wheel to stay as mouse input so the daemon can decide whether
/// scrollback, PTY mouse forwarding, or a no-op owns it.
pub(crate) fn client_owned_mode_state() -> &'static [u8] {
    b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1005l\x1b[?1015l\x1b[?1007l\x1b[?1003h\x1b[?1006h\x1b[?1004h"
}

pub(crate) fn osc22_pointer_shape(shape: PointerShape) -> Vec<u8> {
    format!("\x1b]22;{}\x1b\\", shape.as_osc22_name()).into_bytes()
}

pub(crate) fn enter_attach_terminal(stdout: &mut std::io::Stdout) -> Result<RawModeGuard> {
    crossterm::terminal::enable_raw_mode().context("failed to enable raw mode")?;
    let cleanup = RawModeGuard;
    if host_owns_alt_screen() {
        stdout.write_all(b"\x1b[2J\x1b[H")?;
    } else {
        stdout.write_all(b"\x1b[?1049h\x1b[2J\x1b[H")?;
    }
    stdout.write_all(client_owned_mode_state())?;
    stdout.flush()?;
    Ok(cleanup)
}

/// Return the outer terminal size as `(rows, cols)`.
///
/// `crossterm::terminal::size()` returns `(columns, rows)`. Keep the flip
/// explicit so the agent PTY receives the expected shape.
pub(crate) fn terminal_size() -> (u16, u16) {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((DEFAULT_COLS, DEFAULT_ROWS));
    normalize_size(rows, cols)
}

pub(crate) struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        // Failures here leave the operator's host terminal in raw mode
        // + alt-screen + mouse tracking on, so surface them on stderr.
        let mut stdout = std::io::stdout().lock();
        let write_result = stdout
            .write_all(&outer_terminal_reset_sequence())
            .and_then(|_| stdout.flush());
        drop(stdout);
        let log = |label: &str, e: &dyn std::fmt::Display| {
            eprintln!("[jackin-capsule] failed to {label} on detach: {e}");
        };
        if let Err(e) = write_result {
            log("write outer-terminal reset", &e);
        }
        if let Err(e) = crossterm::terminal::disable_raw_mode() {
            log("disable raw mode", &e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_owned_mode_state_captures_mouse_focus_and_alternate_scroll() {
        let state = client_owned_mode_state();
        for needle in [
            &b"\x1b[?1003h"[..],
            &b"\x1b[?1006h"[..],
            &b"\x1b[?1004h"[..],
            &b"\x1b[?1007l"[..],
        ] {
            assert!(
                state.windows(needle.len()).any(|w| w == needle),
                "client_owned_mode_state missing {needle:?}; got {state:?}"
            );
        }
    }

    #[test]
    fn osc22_pointer_shape_uses_css_names() {
        assert_eq!(
            osc22_pointer_shape(PointerShape::Pointer),
            b"\x1b]22;pointer\x1b\\"
        );
        assert_eq!(
            osc22_pointer_shape(PointerShape::EwResize),
            b"\x1b]22;ew-resize\x1b\\"
        );
    }

    #[test]
    fn outer_terminal_reset_disables_alternate_scroll() {
        let reset = outer_terminal_reset_sequence();
        let needle = b"\x1b[?1007l";
        assert!(
            reset.windows(needle.len()).any(|w| w == needle),
            "outer terminal reset missing alternate-scroll disable: {reset:?}"
        );
    }

    #[test]
    fn reset_base_excludes_alt_screen_leave() {
        assert!(
            !OUTER_TERMINAL_RESET_BASE
                .windows(ALTERNATE_SCREEN_LEAVE.len())
                .any(|w| w == ALTERNATE_SCREEN_LEAVE),
            "reset base must not contain the alternate-screen leave"
        );
        let mut full = OUTER_TERMINAL_RESET_BASE.to_vec();
        full.extend_from_slice(ALTERNATE_SCREEN_LEAVE);
        assert!(full.ends_with(ALTERNATE_SCREEN_LEAVE));
    }

    #[test]
    fn normalize_size_replaces_zero_dimensions_with_defaults() {
        assert_eq!(normalize_size(0, 0), (DEFAULT_ROWS, DEFAULT_COLS));
    }

    #[test]
    fn normalize_size_clamps_tiny_dimensions_to_pty_safe_floor() {
        assert_eq!(normalize_size(1, 1), (MIN_ROWS, MIN_COLS));
    }
}

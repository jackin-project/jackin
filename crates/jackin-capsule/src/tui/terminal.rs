//! Terminal setup and teardown for the capsule TUI: raw mode, alternate
//! screen, size normalization, and escape-sequence cleanup on detach.
//!
//! Not responsible for: widget rendering, input parsing, or session I/O.

use std::io::Write;

use anyhow::{Context, Result};
use jackin_tui::PointerShape;

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
///
/// Leads with SGR reset: the last composed frame leaves its final colors
/// asserted on the outer terminal (in `--debug` runs, the red run-id chip
/// painted bottom-right), and without `\x1b[0m` everything the host prints
/// after detach fills with that background via BCE — the whole post-exit
/// screen turned red.
const OUTER_TERMINAL_RESET_BASE: &[u8] =
    b"\x1b[0m\x1b]22;default\x1b\\\x1b[?7h\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1007l\x1b[?1004l\x1b[?2004l\x1b[?1l\x1b[<u\x1b[?25h";
const ALTERNATE_SCREEN_LEAVE: &[u8] = b"\x1b[?1049l";
pub(crate) const RESET_CLEAR_HOME: &[u8] = b"\x1b[0m\x1b[2J\x1b[H";

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
/// translate wheel gestures in the alternate screen into cursor keys; jackin❯
/// needs the wheel to stay as mouse input so the daemon can decide whether
/// scrollback, PTY mouse forwarding, or a no-op owns it.
///
/// Autowrap (`?7l`) is disabled because the Ratatui compositor positions every
/// cell absolutely and paints the bottom-right cell every full redraw. With
/// autowrap on, writing that last cell pends a wrap and the next byte scrolls
/// the whole screen up one row — the brand/tab status row scrolls off and the
/// frame drifts. Cell-positioned compositors must own autowrap off.
pub(crate) fn client_owned_mode_state() -> &'static [u8] {
    b"\x1b[?7l\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1005l\x1b[?1015l\x1b[?1007l\x1b[?1003h\x1b[?1006h\x1b[?1004h"
}

pub(crate) fn osc22_pointer_shape(shape: PointerShape) -> Vec<u8> {
    jackin_tui::osc22_pointer_shape(shape).into_bytes()
}

pub(crate) fn enter_attach_terminal(stdout: &mut std::io::Stdout) -> Result<RawModeGuard> {
    crossterm::terminal::enable_raw_mode().context("failed to enable raw mode")?;
    let cleanup = RawModeGuard;
    if host_owns_alt_screen() {
        stdout.write_all(RESET_CLEAR_HOME)?;
    } else {
        stdout.write_all(b"\x1b[?1049h")?;
        stdout.write_all(RESET_CLEAR_HOME)?;
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
            .and_then(|()| stdout.flush());
        drop(stdout);
        let log = |label: &str, e: &dyn std::fmt::Display| {
            crate::output::stderr_line(format_args!(
                "[jackin-capsule] failed to {label} on detach: {e}"
            ));
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
mod tests;

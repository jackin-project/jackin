// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;

use anyhow::{Context, Result};
use termrock::osc::PointerShape;

use crate::tui::components::status_bar::STATUS_BAR_ROWS;

pub const DEFAULT_ROWS: u16 = 24;
pub const DEFAULT_COLS: u16 = 80;

const MIN_ROWS: u16 = STATUS_BAR_ROWS + 3;
const MIN_COLS: u16 = 3;

#[must_use]
pub fn normalize_size(rows: u16, cols: u16) -> (u16, u16) {
    let rows = if rows == 0 { DEFAULT_ROWS } else { rows }.max(MIN_ROWS);
    let cols = if cols == 0 { DEFAULT_COLS } else { cols }.max(MIN_COLS);
    (rows, cols)
}

const OUTER_TERMINAL_RESET_BASE: &[u8] =
    b"\x1b[0m\x1b]22;default\x1b\\\x1b[?7h\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1007l\x1b[?1004l\x1b[?2004l\x1b[?1l\x1b[<u\x1b[?25h";
const ALTERNATE_SCREEN_LEAVE: &[u8] = b"\x1b[?1049l";
pub(crate) const RESET_CLEAR_HOME: &[u8] = b"\x1b[0m\x1b[2J\x1b[H";

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

pub(crate) fn client_owned_mode_state() -> &'static [u8] {
    b"\x1b[?7l\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1005l\x1b[?1015l\x1b[?1007l\x1b[?1003h\x1b[?1006h\x1b[?1004h"
}

pub(crate) fn osc22_pointer_shape(shape: PointerShape) -> Vec<u8> {
    termrock::osc::encode_pointer(shape)
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

pub(crate) fn terminal_size() -> (u16, u16) {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((DEFAULT_COLS, DEFAULT_ROWS));
    normalize_size(rows, cols)
}

pub(crate) struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
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

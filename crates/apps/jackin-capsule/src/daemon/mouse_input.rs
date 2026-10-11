// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! TUI mouse, pointer, hover, and text-selection methods for the daemon-owned `Multiplexer`.

use super::Instant;

mod host_open;
mod hover;
mod selection;

#[cfg(test)]
pub(crate) use host_open::host_url_opening_allowed_for;
pub(crate) use host_open::{host_url_opening_allowed, is_double_click};

/// A primary press on a pane cell, in content coordinates, stamped for
/// double-click classification.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PanePress {
    pub(super) session_id: u64,
    pub(super) content_row: usize,
    pub(super) col: u16,
    pub(super) at: Instant,
}

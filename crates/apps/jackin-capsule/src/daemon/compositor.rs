// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! TUI compositor methods for the daemon-owned `Multiplexer`.
//!
//! Moved from daemon.rs to separate this concern from session lifecycle and
//! input dispatch. All methods are impl Multiplexer blocks.

use crate::tui::socket_backend::SgrMetadata;

mod frame;
mod ratatui_frame;
mod regions;

pub(crate) use regions::cached_pane_regions;
#[cfg(test)]
pub(crate) use regions::pane_sgr_regions;

/// Client terminal state the encoder asserted with the last frame. The
/// reconciliation in `append_client_state_reconciliation` diffs the desired
/// state (derived fresh from the focused pane's grid every frame) against
/// this and emits only the transitions — replacing the three hand-maintained
/// mode lists (`current_mode_state`, `drain_mode_transitions`,
/// `focus_swap_reset`) with one derivation (§3.4 of the capsule rendering
/// plan).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssertedClientState {
    pub(super) bracketed_paste: bool,
    pub(super) application_cursor: bool,
    pub(super) kitty_flags: u32,
    pub(super) cursor_visible: bool,
    /// DECSCUSR style (`0` = terminal default).
    pub(super) cursor_style: u16,
}

type HyperlinkRegion = (ratatui::layout::Rect, String);
type SgrRegion = (ratatui::layout::Rect, SgrMetadata);
type PaneRegions = (Vec<HyperlinkRegion>, Vec<SgrRegion>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaneRegionCache {
    key: PaneRegionCacheKey,
    hyperlinks: Vec<HyperlinkRegion>,
    sgr: Vec<SgrRegion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PaneRegionCacheKey {
    inner: ratatui::layout::Rect,
    scrollback_offset: usize,
    focused: bool,
    allow_hyperlinks: bool,
}

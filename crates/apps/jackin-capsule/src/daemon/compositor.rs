// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::socket_backend::SgrMetadata;

mod frame;
mod ratatui_frame;
mod regions;

pub(crate) use regions::cached_pane_regions;
#[cfg(test)]
pub(crate) use regions::pane_sgr_regions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssertedClientState {
    pub(super) bracketed_paste: bool,
    pub(super) application_cursor: bool,
    pub(super) kitty_flags: u32,
    pub(super) cursor_visible: bool,

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

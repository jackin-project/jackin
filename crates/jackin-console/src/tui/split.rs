// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Sidebar/main-area split layout: percentage-based split state, drag
//! clamping, and seam hit-testing for the two-panel console layout.
//!
//! Not responsible for: computing final pixel rects (see `layout`) or
//! rendering either panel.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragState {
    pub anchor_pct: u16,
    pub anchor_x: u16,
}

pub const MIN_SPLIT_PCT: u16 = 20;
pub const MAX_SPLIT_PCT: u16 = 80;
pub const DEFAULT_SPLIT_PCT: u16 = 30;

#[must_use]
pub const fn clamp_split(pct: u16) -> u16 {
    if pct < MIN_SPLIT_PCT {
        MIN_SPLIT_PCT
    } else if pct > MAX_SPLIT_PCT {
        MAX_SPLIT_PCT
    } else {
        pct
    }
}

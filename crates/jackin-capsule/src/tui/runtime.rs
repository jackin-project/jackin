// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! G0 shared-runtime wiring for the capsule TUI.
//!
//! The shared TEA `Component<Ev, Msg>` and `View<Model>` contracts live in
//! `termrock::runtime`. This module is the capsule's implementation of
//! those traits over its surface types. `CapsuleView` is the production
//! adapter: `daemon/compositor.rs` routes the Ratatui frame through
//! [`termrock::runtime::drive_frame`] (plan 021). Render still
//! delegates to `tui/view.rs` (`render_capsule_ratatui_frame`); input
//! still parses via `tui/input.rs` (`InputParser::parse`).

use crate::tui::input::{InputEvent, InputParser};
use crate::tui::view::CapsuleRatatuiFrame;

impl termrock::runtime::View<CapsuleRatatuiFrame<'_>> for CapsuleView {
    fn render(
        &self,
        model: &CapsuleRatatuiFrame<'_>,
        frame: &mut ratatui::Frame<'_>,
        _area: ratatui::layout::Rect,
    ) {
        crate::tui::view::render_capsule_ratatui_frame(frame, model.clone());
    }
}

/// Zero-sized view handle for the G0 `View<Model>` contract. The capsule
/// render function lives in `tui/view.rs`; this wrapper exists so the
/// contract is satisfied without a behavioural change.
#[derive(Debug)]
pub struct CapsuleView;

impl termrock::runtime::Component<Vec<u8>, Vec<InputEvent>> for InputParser {
    fn handle_event(&mut self, event: &Vec<u8>) -> Option<Vec<InputEvent>> {
        let events = self.parse(event);
        if events.is_empty() {
            None
        } else {
            Some(events)
        }
    }
}

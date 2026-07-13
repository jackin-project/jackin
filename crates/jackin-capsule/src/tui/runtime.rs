// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! G0 shared-runtime wiring for the capsule TUI.
//!
//! The shared TEA `Component<Ev, Msg>` and `View<Model>` contracts live in
//! `jackin_tui::runtime`. This module is the capsule's implementation of
//! those traits over its surface types. The existing render path in
//! `tui/view.rs` (`render_capsule_ratatui_frame`) and the existing input
//! path in `tui/input.rs` (`InputParser::parse`) are unchanged; the trait
//! impls are thin delegations that satisfy the shared contract at the type
//! level. Migrating the call sites in `daemon/compositor.rs` and the
//! attach-loop in `tui/run.rs` to dispatch through these traits is a
//! follow-up tracked in G3.

use crate::tui::input::{InputEvent, InputParser};
use crate::tui::view::CapsuleRatatuiFrame;

impl jackin_tui::runtime::View<CapsuleRatatuiFrame<'_>> for CapsuleView {
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

impl jackin_tui::runtime::Component<Vec<u8>, Vec<InputEvent>> for InputParser {
    fn handle_event(&mut self, event: &Vec<u8>) -> Option<Vec<InputEvent>> {
        let events = self.parse(event);
        if events.is_empty() {
            None
        } else {
            Some(events)
        }
    }
}

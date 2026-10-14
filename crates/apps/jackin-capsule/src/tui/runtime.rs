// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

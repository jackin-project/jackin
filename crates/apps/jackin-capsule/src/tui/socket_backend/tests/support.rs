// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn frame_sgr_output(metadata: SgrMetadata, span: Span<'_>) -> Vec<u8> {
    let backend = SocketBackend::new(10, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .backend_mut()
        .set_sgr_regions(vec![(Rect::new(0, 0, 1, 1), metadata)]);
    terminal
        .draw(|frame| {
            frame.render_widget(Paragraph::new(span), frame.area());
        })
        .unwrap();
    terminal.backend_mut().take_output()
}

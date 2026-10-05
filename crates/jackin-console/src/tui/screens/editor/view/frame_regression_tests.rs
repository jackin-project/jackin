// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::layout::Rect;

use super::{EditorScrollGeometry, clamp_editor_scroll_for_frame};

fn scroll_state() -> termrock::widgets::ScrollAreaState {
    crate::tui::scroll_block::console_scroll_area_state()
}

#[test]
fn clamp_binds_both_axes_before_input_and_after_resize() {
    let small = Rect::new(0, 0, 20, 8);
    let geometry = EditorScrollGeometry {
        active_mounts: false,
        content_width: 60,
        content_height: 30,
        mounts_content_width: 1,
    };
    let mut tab = scroll_state();
    let mut mounts = scroll_state();

    clamp_editor_scroll_for_frame(small, geometry, &mut tab, &mut mounts);
    tab.scroll_by(isize::MAX, isize::MAX);
    assert_eq!(
        tab.offset_x(),
        termrock::scroll::max_offset_u16(
            geometry.content_width,
            termrock::scroll::viewport_width(small)
        )
    );
    assert_eq!(
        tab.offset_y(),
        termrock::scroll::max_offset_u16(
            geometry.content_height,
            termrock::scroll::viewport_height(small)
        )
    );

    // A later event must use the same bounded dimensions, so neither axis
    // can continue past the frame's maximum.
    tab.scroll_by(1, 1);
    assert_eq!(
        tab.offset_x(),
        termrock::scroll::max_offset_u16(
            geometry.content_width,
            termrock::scroll::viewport_width(small)
        )
    );
    assert_eq!(
        tab.offset_y(),
        termrock::scroll::max_offset_u16(
            geometry.content_height,
            termrock::scroll::viewport_height(small)
        )
    );

    let large = Rect::new(0, 0, 80, 40);
    clamp_editor_scroll_for_frame(large, geometry, &mut tab, &mut mounts);
    assert_eq!(tab.offset_x(), 0);
    assert_eq!(tab.offset_y(), 0);

    // After resize makes both axes fit, stale frame dimensions must not let
    // the next input recreate an offset.
    tab.scroll_by(1, 1);
    assert_eq!(tab.offset_x(), 0);
    assert_eq!(tab.offset_y(), 0);
}

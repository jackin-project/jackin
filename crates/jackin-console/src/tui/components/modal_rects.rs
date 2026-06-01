//! Shared modal size and placement helpers.

use ratatui::layout::Rect;

use crate::tui::layout::centered_rect_fixed;

#[must_use]
pub fn text_input_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 60, 5)
}

#[must_use]
pub fn source_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 50, 5)
}

#[must_use]
pub fn scope_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 50, 5)
}

#[must_use]
pub fn op_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 80, 22)
}

#[must_use]
pub fn role_picker_rect_for_count(outer: Rect, filtered_len: usize) -> Rect {
    let rows = (filtered_len as u16).saturating_add(6).min(15);
    centered_rect_fixed(outer, 50, rows)
}

#[must_use]
pub fn confirm_rect(outer: Rect, state: &jackin_tui::components::ConfirmState) -> Rect {
    centered_rect_fixed(
        outer,
        jackin_tui::components::confirm_width_pct(state),
        jackin_tui::components::confirm_required_height(state),
    )
}

#[must_use]
pub fn mount_choice_rect(outer: Rect) -> Rect {
    let w = outer.width.min(80);
    let h = 6.min(outer.height);
    Rect {
        x: outer.x + outer.width.saturating_sub(w) / 2,
        y: outer.y + outer.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

#[must_use]
pub fn auth_form_rect_for_height(outer: Rect, required_height: u16) -> Rect {
    centered_rect_fixed(outer, 80, required_height)
}

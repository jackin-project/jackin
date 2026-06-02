//! Top-level console TUI update helpers.

use jackin_tui::runtime::UpdateResult;

pub type ConsoleUpdate<E> = UpdateResult<E>;

#[must_use]
pub fn selection_move_plan(selected: usize, row_count: usize, delta: isize) -> usize {
    crate::focus::moved_selection(selected, row_count, delta)
}

#[must_use]
pub const fn unclamped_scroll_plan(current_scroll: u16, delta: i16) -> u16 {
    let mut scroll = current_scroll;
    jackin_tui::components::apply_scroll_delta_unclamped(&mut scroll, delta);
    scroll
}

#[must_use]
pub fn term_width_scroll_plan(
    current_scroll_x: u16,
    delta: i16,
    term_width: u16,
    content_width: usize,
) -> u16 {
    let mut scroll_x = current_scroll_x;
    jackin_tui::components::apply_term_width_scroll_delta(
        &mut scroll_x,
        delta,
        term_width,
        content_width,
    );
    scroll_x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_width_scroll_plan_updates_and_clamps_offset() {
        assert_eq!(term_width_scroll_plan(0, 8, 10, 40), 8);
        assert_eq!(term_width_scroll_plan(8, -99, 10, 40), 0);
    }

    #[test]
    fn selection_move_plan_clamps_to_rows() {
        assert_eq!(selection_move_plan(0, 3, 99), 2);
        assert_eq!(selection_move_plan(2, 3, -99), 0);
    }

    #[test]
    fn unclamped_scroll_plan_updates_without_upper_clamp() {
        assert_eq!(unclamped_scroll_plan(4, 3), 7);
        assert_eq!(unclamped_scroll_plan(4, -99), 0);
    }
}

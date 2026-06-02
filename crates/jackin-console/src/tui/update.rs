//! Top-level console TUI update helpers.

use jackin_tui::runtime::UpdateResult;

pub type ConsoleUpdate<E> = UpdateResult<E>;

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
}

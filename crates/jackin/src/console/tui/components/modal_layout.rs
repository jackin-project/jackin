//! Modal dialog layout helpers: compute centered rect and backdrop area for overlay dialogs.
//!
//! Single source of truth for modal sizing; delegates per-modal size constants
//! to `jackin_console::tui::components::modal_rects`.
//!
//! Not responsible for: rendering modal content or managing modal open/close state.

use ratatui::layout::Rect;

use crate::console::tui::state::Modal;
use jackin_console::tui::components::modal_rects;

/// Single source of truth for modal size and placement.
pub(crate) fn modal_outer_rect(modal: &Modal<'_>, outer: Rect) -> Rect {
    modal_rects::modal_rect_for_mode(outer, modal.rect_mode(outer))
}

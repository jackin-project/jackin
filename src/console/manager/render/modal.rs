//! Modal dispatcher: per-variant size computation (`modal_outer_rect`)
//! and the widget-dispatch wrapper (`render_modal`) that draws the active
//! modal at the computed geometry.

use ratatui::{Frame, layout::Rect};

use super::super::super::widgets::{
    confirm, confirm_save, error_popup, file_browser, github_picker, mount_dst_choice,
    save_discard, text_input, workdir_pick,
};
use super::super::state::Modal;
use super::centered_rect_fixed;

// ── Modal dispatcher ────────────────────────────────────────────────

/// Compute the outer rect for `modal` inside `outer`. Single source of
/// truth for modal size + placement — `render_modal` dispatches drawing
/// from this rect, and `input::file_browser_modal_rect` re-uses it for
/// mouse hit-testing so both views stay in sync. A layout tweak in one
/// site can't silently desynchronize from the other.
pub(in crate::console::manager) fn modal_outer_rect(modal: &Modal<'_>, outer: Rect) -> Rect {
    // Size by variant: single-line inputs get a compact overlay;
    // lists get a taller one.
    let (pct_w, height_rows) = match modal {
        // TextInput layout: 2 borders + top pad + input + spacer + hint = 6 rows.
        Modal::TextInput { .. } => (60, 6),
        // Confirm height varies with prompt length (e.g. the mount-collapse
        // prompt lists each child/parent pair on its own line).
        Modal::Confirm { state, .. } => (60, confirm::required_height(state)),
        Modal::SaveDiscardCancel { .. } => (70, 7), // three buttons — a bit wider
        // File browser: compact overlay — 70% width, 22 rows (~20 visible
        // entries + banner + nav hint). Rows are an absolute count, not a
        // percentage — centered_rect_fixed takes rows for the height arg.
        Modal::FileBrowser { .. } => (70, 22),
        Modal::WorkdirPick { .. } => (60, 12), // ~6 choices + title + hint
        // Title bar + path + blank + explanation + blank + buttons + blank + hint = 9
        // plus 2 borders handled by centered_rect_fixed; widen to 80% so the
        // explanation sentence fits comfortably on one line.
        Modal::MountDstChoice { .. } => (80, 9),
        // GithubPicker: scale rows with repo count (choices + canonical
        // chrome: top pad + spacer + hint + 2 borders = 5), capped at 15
        // so a sprawling monorepo can't consume the viewport.
        Modal::GithubPicker { state } => {
            let rows = (state.choices.len() as u16).saturating_add(5).min(15);
            (60, rows)
        }
        // ConfirmSave: 80% width, height grows with line count. Clamped
        // to screen height by `centered_rect_fixed`.
        Modal::ConfirmSave { state } => {
            (80, confirm_save::required_height(state).min(outer.height))
        }
        // ErrorPopup: 60% width, word-wrapped message. Height capped at
        // 15 so even a novella error message can't blot out the screen.
        Modal::ErrorPopup { state } => {
            // Estimate inner width from outer width: 60% of frame, minus
            // 2 border columns, minus 2-column left gutter for safety.
            let inner_width = (outer.width * 60 / 100).saturating_sub(4);
            (60, error_popup::required_height(state, inner_width))
        }
    };
    centered_rect_fixed(outer, pct_w, height_rows)
}

pub(super) fn render_modal(frame: &mut Frame, modal: &Modal<'_>) {
    let area = frame.area();
    let modal_area = modal_outer_rect(modal, area);
    match modal {
        Modal::TextInput { state, .. } => text_input::render(frame, modal_area, state),
        Modal::FileBrowser { state, .. } => file_browser::render(frame, modal_area, state),
        Modal::WorkdirPick { state } => workdir_pick::render(frame, modal_area, state),
        Modal::Confirm { state, .. } => confirm::render(frame, modal_area, state),
        Modal::SaveDiscardCancel { state } => save_discard::render(frame, modal_area, state),
        Modal::MountDstChoice { state, .. } => {
            mount_dst_choice::render(frame, modal_area, state);
        }
        Modal::GithubPicker { state } => github_picker::render(frame, modal_area, state),
        Modal::ConfirmSave { state } => confirm_save::render(frame, modal_area, state),
        Modal::ErrorPopup { state } => error_popup::render(frame, modal_area, state),
    }
}

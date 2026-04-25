//! Modal dispatcher: per-variant size computation (`modal_outer_rect`)
//! and the widget-dispatch wrapper (`render_modal`) that draws the active
//! modal at the computed geometry.

use ratatui::{Frame, layout::Rect};

use super::super::super::widgets::{
    agent_picker, confirm, confirm_save, error_popup, file_browser, github_picker,
    mount_dst_choice, op_picker, save_discard, scope_picker, source_picker, text_input,
    workdir_pick,
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
        // OpPicker: 80% width, 22 rows — leaves comfortable space for
        // the breadcrumb header, filter row, ~16 list rows, and chrome.
        Modal::OpPicker { .. } => (80, 22),
        // AgentPicker / AgentOverridePicker: 50% width, height scales
        // with the filtered count (filter row + 2 spacers + footer +
        // 2 borders = 6 chrome rows) capped at 15 so a sprawling agent
        // roster can't blot out the manager. Both variants reuse the
        // same widget — they differ only in host slot and commit
        // handler.
        Modal::AgentPicker { state } | Modal::AgentOverridePicker { state } => {
            let rows = (state.filtered.len() as u16).saturating_add(6).min(15);
            (50, rows)
        }
        // SourcePicker: 50% width is enough for "Source for KEY" plus
        // both buttons; 7 rows give border + top pad + buttons +
        // explainer (always reserved, blank when op is available) +
        // spacer + hint + border.
        //
        // ScopePicker shares the same geometry — same two-button shape,
        // same visual rhythm — so the operator's eye doesn't have to
        // re-anchor when the second modal opens after committing the
        // first.
        Modal::SourcePicker { .. } | Modal::ScopePicker { .. } => (50, 7),
    };
    centered_rect_fixed(outer, pct_w, height_rows)
}

pub(super) fn render_modal(frame: &mut Frame, modal: &mut Modal<'_>) {
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
        Modal::OpPicker { state } => {
            // Advance the spinner glyph and drain any pending background
            // load result before drawing — the picker has no other clock.
            state.tick();
            op_picker::render::render(frame, modal_area, state);
        }
        Modal::AgentPicker { state } | Modal::AgentOverridePicker { state } => {
            agent_picker::render(frame, modal_area, state);
        }
        Modal::SourcePicker { state } => source_picker::render(frame, modal_area, state),
        Modal::ScopePicker { state } => scope_picker::render(frame, modal_area, state),
    }
}

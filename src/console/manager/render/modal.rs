//! Modal dispatcher: per-variant size computation (`modal_outer_rect`)
//! and the widget-dispatch wrapper (`render_modal`) that draws the active
//! modal at the computed geometry.

use ratatui::{Frame, layout::Rect};

use super::super::super::widgets::{
    confirm, confirm_save, error_popup, file_browser, github_picker, mount_dst_choice, op_picker,
    role_picker, save_discard, scope_picker, source_picker, text_input, workdir_pick,
};
use super::super::state::Modal;
use super::centered_rect_fixed;

// ── Modal dispatcher ────────────────────────────────────────────────

/// Single source of truth for modal size + placement;
/// `input::file_browser_modal_rect` re-uses it for mouse hit-testing
/// so the two views stay in sync.
pub(in crate::console::manager) fn modal_outer_rect(modal: &Modal<'_>, outer: Rect) -> Rect {
    if matches!(modal, Modal::MountDstChoice { .. }) {
        let w = outer.width.min(80);
        let h = 8.min(outer.height);
        return Rect {
            x: outer.x + outer.width.saturating_sub(w) / 2,
            y: outer.y + outer.height.saturating_sub(h) / 2,
            width: w,
            height: h,
        };
    }

    let (pct_w, height_rows) = match modal {
        Modal::TextInput { .. } => (60, 6),
        Modal::Confirm { state, .. } => {
            (confirm::width_pct(state), confirm::required_height(state))
        }
        Modal::SaveDiscardCancel { .. } => (70, 7),
        Modal::FileBrowser { .. } => (70, 22),
        Modal::WorkdirPick { .. } => (60, 12),
        Modal::MountDstChoice { .. } => unreachable!("handled above"),
        Modal::GithubPicker { state } => {
            let rows = (state.choices.len() as u16).saturating_add(5).min(15);
            (60, rows)
        }
        Modal::ConfirmSave { state } => {
            (80, confirm_save::required_height(state).min(outer.height))
        }
        Modal::ErrorPopup { state } => {
            // 2 borders + 2-col left gutter for safety.
            let inner_width = (outer.width * 60 / 100).saturating_sub(4);
            (60, error_popup::required_height(state, inner_width))
        }
        Modal::OpPicker { .. } => (80, 22),
        Modal::RolePicker { state } | Modal::RoleOverridePicker { state } => {
            let rows = (state.filtered.len() as u16).saturating_add(6).min(15);
            (50, rows)
        }
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
            // Advance the spinner and drain pending loads — picker
            // has no other clock.
            state.tick();
            op_picker::render::render(frame, modal_area, state);
        }
        Modal::RolePicker { state } | Modal::RoleOverridePicker { state } => {
            role_picker::render(frame, modal_area, state);
        }
        Modal::SourcePicker { state } => source_picker::render(frame, modal_area, state),
        Modal::ScopePicker { state } => scope_picker::render(frame, modal_area, state),
    }
}

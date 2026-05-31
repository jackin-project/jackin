//! Modal dispatcher: per-variant size computation (`modal_outer_rect`)
//! and the widget-dispatch wrapper (`render_modal`) that draws the active
//! modal at the computed geometry.

use ratatui::{Frame, layout::Rect};

use super::super::super::widgets::{
    auth_panel, confirm_save, file_browser, github_picker, mount_dst_choice, op_picker,
    role_picker, scope_picker, source_picker, workdir_pick,
};
use super::super::state::{GlobalMountModal, Modal, SettingsAuthModal, SettingsEnvModal};
use super::centered_rect_fixed;
use jackin_tui::HintSpan;

// ── Modal dispatcher ────────────────────────────────────────────────

pub(in crate::console::manager) fn text_input_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 60, 5)
}

pub(in crate::console::manager) fn source_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 50, 5)
}

pub(in crate::console::manager) fn scope_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 50, 5)
}

pub(in crate::console::manager) fn op_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 80, 22)
}

pub(in crate::console::manager) fn role_picker_rect(
    outer: Rect,
    state: &role_picker::RolePickerState,
) -> Rect {
    let rows = (state.filtered.len() as u16).saturating_add(6).min(15);
    centered_rect_fixed(outer, 50, rows)
}

pub(in crate::console::manager) fn confirm_rect(
    outer: Rect,
    state: &jackin_tui::components::ConfirmState,
) -> Rect {
    centered_rect_fixed(
        outer,
        jackin_tui::components::confirm_width_pct(state),
        jackin_tui::components::confirm_required_height(state),
    )
}

pub(in crate::console::manager) fn mount_choice_rect(outer: Rect) -> Rect {
    let w = outer.width.min(80);
    let h = 6.min(outer.height);
    Rect {
        x: outer.x + outer.width.saturating_sub(w) / 2,
        y: outer.y + outer.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

pub(in crate::console::manager) fn auth_form_rect(
    outer: Rect,
    state: &auth_panel::AuthForm,
) -> Rect {
    centered_rect_fixed(outer, 80, auth_panel::required_height(state))
}

/// Single source of truth for modal size + placement;
/// `input::file_browser_modal_rect` re-uses it for mouse hit-testing
/// so the two views stay in sync.
pub(in crate::console::manager) fn modal_outer_rect(modal: &Modal<'_>, outer: Rect) -> Rect {
    if matches!(modal, Modal::MountDstChoice { .. }) {
        return mount_choice_rect(outer);
    }

    let (pct_w, height_rows) = match modal {
        Modal::TextInput { .. } => return text_input_rect(outer),
        Modal::Confirm { state, .. } => return confirm_rect(outer, state),
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
            // Allow the popup to grow with the terminal so multi-line
            // anyhow chains (the root cause is usually at the bottom)
            // don't get clipped.
            let max_rows = outer.height.saturating_sub(2);
            (
                60,
                jackin_tui::components::error_dialog::required_height(state, inner_width, max_rows),
            )
        }
        Modal::StatusPopup { .. } => (50, 7),
        // A naming sub-stage is a plain labelled input box, sized like
        // every other text-input modal; the drill-down stages use the
        // larger picker rect.
        Modal::OpPicker { state } if state.naming_stage_input().is_some() => {
            return text_input_rect(outer);
        }
        Modal::OpPicker { .. } => return op_picker_rect(outer),
        Modal::RolePicker { state }
        | Modal::RoleOverridePicker { state }
        | Modal::AuthRolePicker { state } => {
            return role_picker_rect(outer, state);
        }
        Modal::SourcePicker { .. } | Modal::AuthSourcePicker { .. } => {
            return source_picker_rect(outer);
        }
        Modal::ScopePicker { .. } => return scope_picker_rect(outer),
        // Hug the content: hide the credential block when the mode
        // doesn't need one so the dialog doesn't leave dead rows
        // below the hint line.
        Modal::AuthForm { state, .. } => return auth_form_rect(outer, state.as_ref()),
    };
    centered_rect_fixed(outer, pct_w, height_rows)
}

pub(super) fn prepare_modal(outer: ratatui::layout::Rect, modal: &mut Modal<'_>) {
    let modal_area = modal_outer_rect(modal, outer);
    match modal {
        Modal::OpPicker { state } => state.tick(),
        Modal::ConfirmSave { state } => confirm_save::prepare_for_render(modal_area, state),
        _ => {}
    }
}

pub(super) fn render_modal(frame: &mut Frame, modal: &Modal<'_>) {
    let area = frame.area();
    let modal_area = modal_outer_rect(modal, area);
    match modal {
        Modal::TextInput { state, .. } => {
            jackin_tui::components::render_text_input(frame, modal_area, state);
        }
        Modal::FileBrowser { state, .. } => file_browser::render(frame, modal_area, state),
        Modal::WorkdirPick { state } => workdir_pick::render(frame, modal_area, state),
        Modal::Confirm { state, .. } => {
            jackin_tui::components::render_confirm_dialog(frame, modal_area, state);
        }
        Modal::SaveDiscardCancel { state } => {
            jackin_tui::components::render_save_discard_dialog(frame, modal_area, state);
        }
        Modal::MountDstChoice { state, .. } => {
            mount_dst_choice::render(frame, modal_area, state);
        }
        Modal::GithubPicker { state } => github_picker::render(frame, modal_area, state),
        Modal::ConfirmSave { state } => confirm_save::render(frame, modal_area, state),
        Modal::ErrorPopup { state } => {
            jackin_tui::components::render_error_dialog(frame, modal_area, state);
        }
        Modal::StatusPopup { state } => {
            jackin_tui::components::render_status_popup(frame, modal_area, state);
        }
        Modal::OpPicker { state } => {
            op_picker::render::render(frame, modal_area, state);
        }
        Modal::RolePicker { state }
        | Modal::RoleOverridePicker { state }
        | Modal::AuthRolePicker { state } => {
            role_picker::render(frame, modal_area, state);
        }
        Modal::SourcePicker { state, .. } | Modal::AuthSourcePicker { state } => {
            source_picker::render(frame, modal_area, state);
        }
        Modal::ScopePicker { state } => scope_picker::render(frame, modal_area, state),
        Modal::AuthForm { state, focus, .. } => {
            auth_panel::render_form(frame, modal_area, state.as_ref(), *focus);
        }
    }
}

// ── Modal footer-item dispatch ──────────────────────────────────────────────
//
// When a modal is open the main footer must show the modal's keys, not the
// "behind" content keys. These functions return `Vec<HintSpan<'static>>` for each
// modal variant. Callers (render_editor, render_settings) check whether a
// modal is open and delegate here before building contextual footer items.

/// Footer items for an editor-stage `Modal`. Returns the keys valid while
/// that modal has focus.
#[allow(clippy::too_many_lines)]
pub(super) fn modal_footer_items(modal: &Modal<'_>) -> Vec<HintSpan<'static>> {
    match modal {
        Modal::AuthForm { state, focus, .. } => auth_form_footer_items(state.as_ref(), *focus),
        Modal::TextInput { .. } => vec![
            HintSpan::Key("↵"),
            HintSpan::Text("confirm"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        Modal::FileBrowser { state, .. } => state.footer_items(),
        Modal::MountDstChoice { .. } => vec![
            HintSpan::Key("M"),
            HintSpan::Text("mount"),
            HintSpan::GroupSep,
            HintSpan::Key("E"),
            HintSpan::Text("edit"),
            HintSpan::GroupSep,
            HintSpan::Key("\u{2190}/\u{2192}"),
            HintSpan::Text("move"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
            HintSpan::GroupSep,
            HintSpan::Key("C/Esc"),
            HintSpan::Text("cancel"),
        ],
        Modal::SourcePicker { .. } | Modal::AuthSourcePicker { .. } | Modal::ScopePicker { .. } => {
            vec![
                HintSpan::Key("\u{2190}/\u{2192}"),
                HintSpan::Text("move"),
                HintSpan::GroupSep,
                HintSpan::Key("↵"),
                HintSpan::Text("select"),
                HintSpan::GroupSep,
                HintSpan::Key("Esc"),
                HintSpan::Text("cancel"),
            ]
        }
        Modal::WorkdirPick { .. } => vec![
            HintSpan::Key("\u{2191}\u{2193}"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        Modal::GithubPicker { .. } => vec![
            HintSpan::Key("\u{2191}\u{2193}"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("confirm"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        Modal::ConfirmSave { state } => {
            let mut items = vec![
                HintSpan::Key("S"),
                HintSpan::Text("save"),
                HintSpan::GroupSep,
                HintSpan::Key("C/Esc"),
                HintSpan::Text("cancel"),
            ];
            if !state.lines.is_empty() {
                items.extend([
                    HintSpan::GroupSep,
                    HintSpan::Key("\u{2191}\u{2193}"),
                    HintSpan::Text("scroll"),
                ]);
            }
            items
        }
        Modal::SaveDiscardCancel { .. } => vec![
            HintSpan::Key("S"),
            HintSpan::Text("save"),
            HintSpan::GroupSep,
            HintSpan::Key("D"),
            HintSpan::Text("discard"),
            HintSpan::GroupSep,
            HintSpan::Key("C/Esc"),
            HintSpan::Text("cancel"),
        ],
        Modal::ErrorPopup { .. } => vec![HintSpan::Key("↵/Esc"), HintSpan::Text("dismiss")],
        Modal::StatusPopup { .. } => vec![HintSpan::Text("working")],
        // A naming sub-stage is a plain input box: confirm / cancel only.
        Modal::OpPicker { state } if state.naming_stage_input().is_some() => vec![
            HintSpan::Key("↵"),
            HintSpan::Text("confirm"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        Modal::OpPicker { state }
            if state.stage == crate::console::widgets::op_picker::OpPickerStage::Section =>
        {
            vec![
                HintSpan::Key("\u{2191}\u{2193}"),
                HintSpan::Text("navigate"),
                HintSpan::GroupSep,
                HintSpan::Key("↵"),
                HintSpan::Text("select"),
                HintSpan::GroupSep,
                HintSpan::Key("Esc"),
                HintSpan::Text("cancel"),
            ]
        }
        Modal::OpPicker { .. } => vec![
            HintSpan::Key("\u{2191}\u{2193}"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            HintSpan::Key("type"),
            HintSpan::Text("filter"),
            HintSpan::GroupSep,
            HintSpan::Key("R"),
            HintSpan::Text("refresh"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        Modal::RolePicker { .. }
        | Modal::RoleOverridePicker { .. }
        | Modal::AuthRolePicker { .. } => vec![
            HintSpan::Key("\u{2191}\u{2193}"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            HintSpan::Key("type"),
            HintSpan::Text("filter"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        Modal::Confirm { .. } => vec![
            HintSpan::Key("Y"),
            HintSpan::Text("yes"),
            HintSpan::GroupSep,
            HintSpan::Key("N/Esc"),
            HintSpan::Text("no"),
        ],
    }
}

/// Footer items for the three settings modal chains.
pub(super) fn settings_mounts_modal_footer_items(
    modal: &GlobalMountModal<'_>,
) -> Vec<HintSpan<'static>> {
    match modal {
        GlobalMountModal::Text { .. } => vec![
            HintSpan::Key("↵"),
            HintSpan::Text("confirm"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        GlobalMountModal::FileBrowser { state } => state.footer_items(),
        GlobalMountModal::MountDstChoice { .. } => vec![
            HintSpan::Key("M"),
            HintSpan::Text("mount"),
            HintSpan::GroupSep,
            HintSpan::Key("E"),
            HintSpan::Text("edit"),
            HintSpan::GroupSep,
            HintSpan::Key("\u{2190}/\u{2192}"),
            HintSpan::Text("move"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
            HintSpan::GroupSep,
            HintSpan::Key("C/Esc"),
            HintSpan::Text("cancel"),
        ],
        GlobalMountModal::ScopePicker { .. } => vec![
            HintSpan::Key("\u{2190}/\u{2192}"),
            HintSpan::Text("move"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        GlobalMountModal::RolePicker { .. } => vec![
            HintSpan::Key("\u{2191}\u{2193}"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            HintSpan::Key("type"),
            HintSpan::Text("filter"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        GlobalMountModal::Confirm { .. } => vec![
            HintSpan::Key("Y"),
            HintSpan::Text("yes"),
            HintSpan::GroupSep,
            HintSpan::Key("N/Esc"),
            HintSpan::Text("no"),
        ],
        GlobalMountModal::PreviewSave { state } => {
            let mut items = vec![
                HintSpan::Key("S"),
                HintSpan::Text("save"),
                HintSpan::GroupSep,
                HintSpan::Key("C/Esc"),
                HintSpan::Text("cancel"),
            ];
            if !state.lines.is_empty() {
                items.extend([
                    HintSpan::GroupSep,
                    HintSpan::Key("\u{2191}\u{2193}"),
                    HintSpan::Text("scroll"),
                ]);
            }
            items
        }
    }
}

pub(super) fn settings_env_modal_footer_items(
    modal: &SettingsEnvModal<'_>,
) -> Vec<HintSpan<'static>> {
    match modal {
        SettingsEnvModal::Text { .. } => vec![
            HintSpan::Key("↵"),
            HintSpan::Text("confirm"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        SettingsEnvModal::SourcePicker { .. } | SettingsEnvModal::ScopePicker { .. } => vec![
            HintSpan::Key("\u{2190}/\u{2192}"),
            HintSpan::Text("move"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        SettingsEnvModal::OpPicker { .. } | SettingsEnvModal::RolePicker { .. } => vec![
            HintSpan::Key("\u{2191}\u{2193}"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            HintSpan::Key("type"),
            HintSpan::Text("filter"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        SettingsEnvModal::Confirm { .. } => vec![
            HintSpan::Key("Y"),
            HintSpan::Text("yes"),
            HintSpan::GroupSep,
            HintSpan::Key("N/Esc"),
            HintSpan::Text("no"),
        ],
    }
}

pub(super) fn settings_auth_modal_footer_items(
    auth: &crate::console::manager::state::SettingsAuthState,
) -> Vec<HintSpan<'static>> {
    let Some(modal) = auth.modal.as_ref() else {
        return Vec::new();
    };
    match modal {
        SettingsAuthModal::AuthForm { state, focus, .. } => {
            let mut items = auth_form_footer_items(state.as_ref(), *focus);
            // The auth-form `g`/`G` generate trigger is gated to the
            // global Claude oauth_token slot; surface the hint only when
            // that gate holds.
            if crate::console::manager::input::global_mounts::settings_auth_can_generate_token(auth)
            {
                items.extend([
                    HintSpan::GroupSep,
                    HintSpan::Key("G"),
                    HintSpan::Text("generate"),
                ]);
            }
            items
        }
        SettingsAuthModal::TextInput { .. } => vec![
            HintSpan::Key("↵"),
            HintSpan::Text("confirm"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        SettingsAuthModal::SourcePicker { .. } => vec![
            HintSpan::Key("\u{2190}/\u{2192}"),
            HintSpan::Text("move"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        // A naming sub-stage is a plain input box: confirm / cancel only.
        SettingsAuthModal::OpPicker { state } if state.naming_stage_input().is_some() => vec![
            HintSpan::Key("↵"),
            HintSpan::Text("confirm"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
        SettingsAuthModal::OpPicker { state }
            if state.stage == crate::console::widgets::op_picker::OpPickerStage::Section =>
        {
            vec![
                HintSpan::Key("\u{2191}\u{2193}"),
                HintSpan::Text("navigate"),
                HintSpan::GroupSep,
                HintSpan::Key("↵"),
                HintSpan::Text("select"),
                HintSpan::GroupSep,
                HintSpan::Key("Esc"),
                HintSpan::Text("cancel"),
            ]
        }
        SettingsAuthModal::OpPicker { .. } => vec![
            HintSpan::Key("\u{2191}\u{2193}"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            HintSpan::Key("type"),
            HintSpan::Text("filter"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
            HintSpan::GroupSep,
            HintSpan::Key("Esc"),
            HintSpan::Text("cancel"),
        ],
    }
}

/// Convert the auth form hint logic into `Vec<HintSpan<'static>>` for the main footer.
fn auth_form_footer_items(
    form: &crate::console::widgets::auth_panel::form::AuthForm,
    focus: crate::console::manager::state::AuthFormFocus,
) -> Vec<HintSpan<'static>> {
    use crate::console::manager::state::AuthFormFocus;
    let mut items: Vec<HintSpan<'static>> = match focus {
        AuthFormFocus::Mode => {
            let mut v = vec![HintSpan::Key("␣"), HintSpan::Text("cycle")];
            if form.shows_credential_block() {
                v.extend([
                    HintSpan::Sep,
                    HintSpan::Key("\u{2193}"),
                    HintSpan::Text("navigate"),
                ]);
            }
            v.extend([
                HintSpan::GroupSep,
                HintSpan::Key("⇥"),
                HintSpan::Text("button row"),
            ]);
            v
        }
        AuthFormFocus::CredentialSource => vec![
            HintSpan::Key("↵"),
            HintSpan::Text("set"),
            HintSpan::Sep,
            HintSpan::Key("\u{2191}"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            HintSpan::Key("⇥"),
            HintSpan::Text("button row"),
        ],
        AuthFormFocus::Save | AuthFormFocus::Cancel | AuthFormFocus::Reset => vec![
            HintSpan::Key("\u{2190}/\u{2192}"),
            HintSpan::Text("move"),
            HintSpan::GroupSep,
            HintSpan::Key("⇥"),
            HintSpan::Text("fields"),
            HintSpan::GroupSep,
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
        ],
    };
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]);
    items
}

//! Modal dispatcher: per-variant size computation (`modal_outer_rect`)
//! and the widget-dispatch wrapper (`render_modal`) that draws the active
//! modal at the computed geometry.

use ratatui::{Frame, layout::Rect};

use super::super::super::widgets::{
    auth_panel, confirm, confirm_save, error_popup, file_browser, github_picker, mount_dst_choice,
    op_picker, role_picker, save_discard, scope_picker, source_picker, text_input, workdir_pick,
};
use super::super::state::{GlobalMountModal, Modal, SettingsAuthModal, SettingsEnvModal};
use super::FooterItem;
use super::centered_rect_fixed;

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
    state: &confirm::ConfirmState,
) -> Rect {
    centered_rect_fixed(
        outer,
        confirm::width_pct(state),
        confirm::required_height(state),
    )
}

pub(in crate::console::manager) fn mount_choice_rect(outer: Rect) -> Rect {
    let w = outer.width.min(80);
    let h = 8.min(outer.height);
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
    centered_rect_fixed(
        outer,
        80,
        auth_panel::required_height(state, outer.width * 80 / 100),
    )
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
                error_popup::required_height(state, inner_width, max_rows),
            )
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
        Modal::RolePicker { state }
        | Modal::RoleOverridePicker { state }
        | Modal::AuthRolePicker { state } => {
            role_picker::render(frame, modal_area, state);
        }
        Modal::SourcePicker { state } | Modal::AuthSourcePicker { state } => {
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
// "behind" content keys. These functions return `Vec<FooterItem>` for each
// modal variant. Callers (render_editor, render_settings) check whether a
// modal is open and delegate here before building contextual footer items.

/// Footer items for an editor-stage `Modal`. Returns the keys valid while
/// that modal has focus.
#[allow(clippy::too_many_lines)]
pub(super) fn modal_footer_items(modal: &Modal<'_>) -> Vec<FooterItem> {
    match modal {
        Modal::AuthForm { state, focus, .. } => auth_form_footer_items(state.as_ref(), *focus),
        Modal::TextInput { .. } => vec![
            FooterItem::Key("Enter"),
            FooterItem::Text("confirm"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        Modal::FileBrowser { .. } => vec![
            FooterItem::Key("\u{2191}\u{2193}"),
            FooterItem::Text("navigate"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("open"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        Modal::MountDstChoice { .. }
        | Modal::SourcePicker { .. }
        | Modal::AuthSourcePicker { .. }
        | Modal::ScopePicker { .. } => vec![
            FooterItem::Key("\u{2190}/\u{2192}"),
            FooterItem::Text("move"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("select"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        Modal::WorkdirPick { .. } => vec![
            FooterItem::Key("\u{2191}\u{2193}"),
            FooterItem::Text("navigate"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("select"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        Modal::GithubPicker { .. } => vec![
            FooterItem::Key("\u{2191}\u{2193}"),
            FooterItem::Text("navigate"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("confirm"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        Modal::ConfirmSave { state } => {
            let mut items = vec![
                FooterItem::Key("S"),
                FooterItem::Text("save"),
                FooterItem::GroupSep,
                FooterItem::Key("C/Esc"),
                FooterItem::Text("cancel"),
            ];
            if !state.lines.is_empty() {
                items.extend([
                    FooterItem::GroupSep,
                    FooterItem::Key("\u{2191}\u{2193}"),
                    FooterItem::Text("scroll"),
                ]);
            }
            items
        }
        Modal::SaveDiscardCancel { .. } => vec![
            FooterItem::Key("S"),
            FooterItem::Text("save"),
            FooterItem::GroupSep,
            FooterItem::Key("D"),
            FooterItem::Text("discard"),
            FooterItem::GroupSep,
            FooterItem::Key("C/Esc"),
            FooterItem::Text("cancel"),
        ],
        Modal::ErrorPopup { .. } => vec![FooterItem::Key("Enter/Esc"), FooterItem::Text("dismiss")],
        Modal::OpPicker { .. }
        | Modal::RolePicker { .. }
        | Modal::RoleOverridePicker { .. }
        | Modal::AuthRolePicker { .. } => vec![
            FooterItem::Key("\u{2191}\u{2193}"),
            FooterItem::Text("navigate"),
            FooterItem::GroupSep,
            FooterItem::Key("type"),
            FooterItem::Text("filter"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("select"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        Modal::Confirm { .. } => vec![
            FooterItem::Key("Y"),
            FooterItem::Text("yes"),
            FooterItem::GroupSep,
            FooterItem::Key("N/Esc"),
            FooterItem::Text("no"),
        ],
    }
}

/// Footer items for the three settings modal chains.
pub(super) fn settings_mounts_modal_footer_items(modal: &GlobalMountModal<'_>) -> Vec<FooterItem> {
    match modal {
        GlobalMountModal::Text { .. } => vec![
            FooterItem::Key("Enter"),
            FooterItem::Text("confirm"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        GlobalMountModal::FileBrowser { .. } => vec![
            FooterItem::Key("\u{2191}\u{2193}"),
            FooterItem::Text("navigate"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("open"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        GlobalMountModal::MountDstChoice { .. } | GlobalMountModal::ScopePicker { .. } => vec![
            FooterItem::Key("\u{2190}/\u{2192}"),
            FooterItem::Text("move"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("select"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        GlobalMountModal::RolePicker { .. } => vec![
            FooterItem::Key("\u{2191}\u{2193}"),
            FooterItem::Text("navigate"),
            FooterItem::GroupSep,
            FooterItem::Key("type"),
            FooterItem::Text("filter"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("select"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        GlobalMountModal::Confirm { .. } => vec![
            FooterItem::Key("Y"),
            FooterItem::Text("yes"),
            FooterItem::GroupSep,
            FooterItem::Key("N/Esc"),
            FooterItem::Text("no"),
        ],
        GlobalMountModal::PreviewSave { state } => {
            let mut items = vec![
                FooterItem::Key("S"),
                FooterItem::Text("save"),
                FooterItem::GroupSep,
                FooterItem::Key("C/Esc"),
                FooterItem::Text("cancel"),
            ];
            if !state.lines.is_empty() {
                items.extend([
                    FooterItem::GroupSep,
                    FooterItem::Key("\u{2191}\u{2193}"),
                    FooterItem::Text("scroll"),
                ]);
            }
            items
        }
    }
}

pub(super) fn settings_env_modal_footer_items(modal: &SettingsEnvModal<'_>) -> Vec<FooterItem> {
    match modal {
        SettingsEnvModal::Text { .. } => vec![
            FooterItem::Key("Enter"),
            FooterItem::Text("confirm"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        SettingsEnvModal::SourcePicker { .. } | SettingsEnvModal::ScopePicker { .. } => vec![
            FooterItem::Key("\u{2190}/\u{2192}"),
            FooterItem::Text("move"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("select"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        SettingsEnvModal::OpPicker { .. } | SettingsEnvModal::RolePicker { .. } => vec![
            FooterItem::Key("\u{2191}\u{2193}"),
            FooterItem::Text("navigate"),
            FooterItem::GroupSep,
            FooterItem::Key("type"),
            FooterItem::Text("filter"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("select"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        SettingsEnvModal::Confirm { .. } => vec![
            FooterItem::Key("Y"),
            FooterItem::Text("yes"),
            FooterItem::GroupSep,
            FooterItem::Key("N/Esc"),
            FooterItem::Text("no"),
        ],
    }
}

pub(super) fn settings_auth_modal_footer_items(modal: &SettingsAuthModal<'_>) -> Vec<FooterItem> {
    match modal {
        SettingsAuthModal::AuthForm { state, focus, .. } => {
            auth_form_footer_items(state.as_ref(), *focus)
        }
        SettingsAuthModal::TextInput { .. } => vec![
            FooterItem::Key("Enter"),
            FooterItem::Text("confirm"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        SettingsAuthModal::SourcePicker { .. } => vec![
            FooterItem::Key("\u{2190}/\u{2192}"),
            FooterItem::Text("move"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("select"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
        SettingsAuthModal::OpPicker { .. } => vec![
            FooterItem::Key("\u{2191}\u{2193}"),
            FooterItem::Text("navigate"),
            FooterItem::GroupSep,
            FooterItem::Key("type"),
            FooterItem::Text("filter"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("select"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ],
    }
}

/// Convert the auth form hint logic into `Vec<FooterItem>` for the main footer.
fn auth_form_footer_items(
    form: &crate::console::widgets::auth_panel::form::AuthForm,
    focus: crate::console::manager::state::AuthFormFocus,
) -> Vec<FooterItem> {
    use crate::console::manager::state::AuthFormFocus;
    let mut items: Vec<FooterItem> = match focus {
        AuthFormFocus::Mode => {
            let mut v = vec![FooterItem::Key("Space"), FooterItem::Text("cycle")];
            if form.shows_credential_block() {
                v.extend([
                    FooterItem::Sep,
                    FooterItem::Key("\u{2193}"),
                    FooterItem::Text("navigate"),
                ]);
            }
            v.extend([
                FooterItem::GroupSep,
                FooterItem::Key("Tab"),
                FooterItem::Text("button row"),
            ]);
            v
        }
        AuthFormFocus::CredentialSource => vec![
            FooterItem::Key("Enter"),
            FooterItem::Text("set"),
            FooterItem::Sep,
            FooterItem::Key("\u{2191}"),
            FooterItem::Text("navigate"),
            FooterItem::GroupSep,
            FooterItem::Key("Tab"),
            FooterItem::Text("button row"),
        ],
        AuthFormFocus::Save | AuthFormFocus::Cancel | AuthFormFocus::Reset => vec![
            FooterItem::Key("\u{2190}/\u{2192}"),
            FooterItem::Text("move"),
            FooterItem::GroupSep,
            FooterItem::Key("Tab"),
            FooterItem::Text("fields"),
            FooterItem::GroupSep,
            FooterItem::Key("Enter"),
            FooterItem::Text("select"),
        ],
    };
    items.extend([
        FooterItem::GroupSep,
        FooterItem::Key("Esc"),
        FooterItem::Text("cancel"),
    ]);
    items
}

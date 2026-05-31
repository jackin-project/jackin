//! Modal dispatcher: widget-dispatch wrapper (`render_modal`) that draws the
//! active modal at the manager-owned computed geometry.

use ratatui::Frame;

use super::super::super::widgets::{
    auth_panel, confirm_save, file_browser, github_picker, mount_dst_choice, op_picker,
    role_picker, scope_picker, source_picker, workdir_pick,
};
use crate::console::manager::modal_layout::modal_outer_rect;
use crate::console::manager::state::{
    GlobalMountModal, Modal, SettingsAuthModal, SettingsEnvModal,
};
use jackin_tui::HintSpan;
use jackin_tui::components::hint_bar::CONFIRM_DISMISS_HINT;

// ── Modal dispatcher ────────────────────────────────────────────────

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
        Modal::ContainerInfo { state } => {
            jackin_tui::components::render_container_info(frame, modal_area, state);
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
        Modal::TextInput { .. } => CONFIRM_DISMISS_HINT.to_vec(),
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
        Modal::ContainerInfo { .. } => vec![
            HintSpan::Key("↵/Esc"),
            HintSpan::Text("dismiss"),
            HintSpan::GroupSep,
            HintSpan::Key("click"),
            HintSpan::Text("copy value"),
        ],
        Modal::StatusPopup { .. } => vec![HintSpan::Text("working")],
        // A naming sub-stage is a plain input box: confirm / cancel only.
        Modal::OpPicker { state } if state.naming_stage_input().is_some() => {
            CONFIRM_DISMISS_HINT.to_vec()
        }
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
        GlobalMountModal::Text { .. } => CONFIRM_DISMISS_HINT.to_vec(),
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
        SettingsEnvModal::Text { .. } => CONFIRM_DISMISS_HINT.to_vec(),
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
        SettingsAuthModal::TextInput { .. } => CONFIRM_DISMISS_HINT.to_vec(),
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
        SettingsAuthModal::OpPicker { state } if state.naming_stage_input().is_some() => {
            CONFIRM_DISMISS_HINT.to_vec()
        }
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

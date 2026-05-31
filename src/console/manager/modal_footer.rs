//! Footer hint items for manager modal states.

use jackin_tui::HintSpan;
use jackin_tui::components::hint_bar::CONFIRM_DISMISS_HINT;

use crate::console::manager::state::{
    AuthFormFocus, GlobalMountModal, Modal, SettingsAuthModal, SettingsAuthState, SettingsEnvModal,
};

#[allow(clippy::too_many_lines)]
pub(crate) fn modal_footer_items(modal: &Modal<'_>) -> Vec<HintSpan<'static>> {
    match modal {
        Modal::AuthForm { state, focus, .. } => auth_form_footer_items(state.as_ref(), *focus),
        Modal::TextInput { .. } => CONFIRM_DISMISS_HINT.to_vec(),
        Modal::FileBrowser { state, .. } => state.footer_items(),
        Modal::MountDstChoice { .. } => mount_destination_footer_items(),
        Modal::SourcePicker { .. } | Modal::AuthSourcePicker { .. } | Modal::ScopePicker { .. } => {
            segmented_choice_footer_items()
        }
        Modal::WorkdirPick { .. } => pick_list_footer_items("select"),
        Modal::GithubPicker { .. } => pick_list_footer_items("confirm"),
        Modal::ConfirmSave { state } => confirm_save_footer_items(!state.lines.is_empty()),
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
        Modal::OpPicker { .. } => filtered_picker_footer_items(true),
        Modal::RolePicker { .. }
        | Modal::RoleOverridePicker { .. }
        | Modal::AuthRolePicker { .. } => filtered_picker_footer_items(false),
        Modal::Confirm { .. } => yes_no_footer_items(),
    }
}

pub(crate) fn settings_mounts_modal_footer_items(
    modal: &GlobalMountModal<'_>,
) -> Vec<HintSpan<'static>> {
    match modal {
        GlobalMountModal::Text { .. } => CONFIRM_DISMISS_HINT.to_vec(),
        GlobalMountModal::FileBrowser { state } => state.footer_items(),
        GlobalMountModal::MountDstChoice { .. } => mount_destination_footer_items(),
        GlobalMountModal::ScopePicker { .. } => segmented_choice_footer_items(),
        GlobalMountModal::RolePicker { .. } => filtered_picker_footer_items(false),
        GlobalMountModal::Confirm { .. } => yes_no_footer_items(),
        GlobalMountModal::PreviewSave { state } => {
            confirm_save_footer_items(!state.lines.is_empty())
        }
    }
}

pub(crate) fn settings_env_modal_footer_items(
    modal: &SettingsEnvModal<'_>,
) -> Vec<HintSpan<'static>> {
    match modal {
        SettingsEnvModal::Text { .. } => CONFIRM_DISMISS_HINT.to_vec(),
        SettingsEnvModal::SourcePicker { .. } | SettingsEnvModal::ScopePicker { .. } => {
            segmented_choice_footer_items()
        }
        SettingsEnvModal::OpPicker { .. } | SettingsEnvModal::RolePicker { .. } => {
            filtered_picker_footer_items(false)
        }
        SettingsEnvModal::Confirm { .. } => yes_no_footer_items(),
    }
}

pub(crate) fn settings_auth_modal_footer_items(auth: &SettingsAuthState) -> Vec<HintSpan<'static>> {
    let Some(modal) = auth.modal.as_ref() else {
        return Vec::new();
    };
    match modal {
        SettingsAuthModal::AuthForm { state, focus, .. } => {
            let mut items = auth_form_footer_items(state.as_ref(), *focus);
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
        SettingsAuthModal::SourcePicker { .. } => segmented_choice_footer_items(),
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
        SettingsAuthModal::OpPicker { .. } => filtered_picker_footer_items(false),
    }
}

fn auth_form_footer_items(
    form: &crate::console::widgets::auth_panel::form::AuthForm,
    focus: AuthFormFocus,
) -> Vec<HintSpan<'static>> {
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

fn mount_destination_footer_items() -> Vec<HintSpan<'static>> {
    vec![
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
    ]
}

fn segmented_choice_footer_items() -> Vec<HintSpan<'static>> {
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

fn pick_list_footer_items(commit_label: &'static str) -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        HintSpan::Key("↵"),
        HintSpan::Text(commit_label),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

fn filtered_picker_footer_items(include_refresh: bool) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        HintSpan::Key("\u{2191}\u{2193}"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        HintSpan::Key("type"),
        HintSpan::Text("filter"),
    ];
    if include_refresh {
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("R"),
            HintSpan::Text("refresh"),
        ]);
    }
    items.extend([
        HintSpan::GroupSep,
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]);
    items
}

fn confirm_save_footer_items(scrollable: bool) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        HintSpan::Key("S"),
        HintSpan::Text("save"),
        HintSpan::GroupSep,
        HintSpan::Key("C/Esc"),
        HintSpan::Text("cancel"),
    ];
    if scrollable {
        items.extend([
            HintSpan::GroupSep,
            HintSpan::Key("\u{2191}\u{2193}"),
            HintSpan::Text("scroll"),
        ]);
    }
    items
}

fn yes_no_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key("Y"),
        HintSpan::Text("yes"),
        HintSpan::GroupSep,
        HintSpan::Key("N/Esc"),
        HintSpan::Text("no"),
    ]
}

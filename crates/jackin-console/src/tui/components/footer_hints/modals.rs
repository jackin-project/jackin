// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Modal footer dispatcher + the per-modal-mode hint-span builders
//! (auth form, confirm save, container info, status popup, op picker).

use jackin_tui::components::{ScrollAxes, error_popup_hint_spans, save_discard_hint_spans};
use jackin_tui::{HintSpan, keymap::glyph};

use crate::tui::components::auth_panel;
use crate::tui::components::confirm_save;
use crate::tui::components::file_browser::FileBrowserState;
use crate::tui::components::op_picker::{OpPickerRenderState, OpPickerStage};
use crate::tui::screens::settings::model::AuthFormFocus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalFooterMode {
    AuthForm {
        focus: AuthFormFocus,
        shows_source_folder: bool,
        shows_credential_block: bool,
        can_generate_token: bool,
    },
    ConfirmDismiss,
    FileBrowser,
    MountDestination,
    SegmentedChoice,
    PickList {
        commit_label: &'static str,
    },
    ConfirmSave {
        scroll_axes: ScrollAxes,
    },
    SaveDiscardCancel,
    ErrorPopup,
    ContainerInfo,
    StatusPopup,
    OpNamingTextInput,
    OpSection,
    FilteredPicker {
        include_refresh: bool,
        include_collapse: bool,
    },
    YesNo,
}

pub trait ModalFileBrowserFooterState {
    fn footer_items(&self) -> Vec<HintSpan<'static>>;
}

impl ModalFileBrowserFooterState for FileBrowserState {
    fn footer_items(&self) -> Vec<HintSpan<'static>> {
        Self::footer_items(self)
    }
}

pub trait ModalAuthFormFooterState<Focus> {
    fn footer_mode(&self, focus: Focus, can_generate_token: bool) -> ModalFooterMode;
}

impl<V: auth_panel::AuthCredential> ModalAuthFormFooterState<AuthFormFocus>
    for auth_panel::AuthForm<V>
{
    fn footer_mode(&self, focus: AuthFormFocus, can_generate_token: bool) -> ModalFooterMode {
        ModalFooterMode::AuthForm {
            focus,
            shows_source_folder: self.shows_source_folder(),
            shows_credential_block: self.shows_credential_block(),
            can_generate_token,
        }
    }
}

pub trait ModalConfirmSaveFooterState {
    fn footer_mode(&self) -> ModalFooterMode;
}

impl<M: Clone> ModalConfirmSaveFooterState for confirm_save::ConfirmSaveState<M> {
    fn footer_mode(&self) -> ModalFooterMode {
        ModalFooterMode::ConfirmSave {
            scroll_axes: self.scroll_axes(),
        }
    }
}

pub trait ModalContainerInfoFooterState {
    fn content_width(&self) -> usize;
    fn content_height(&self) -> usize;
}

impl ModalContainerInfoFooterState for jackin_tui::components::ContainerInfoState {
    fn content_width(&self) -> usize {
        Self::content_width(self)
    }

    fn content_height(&self) -> usize {
        Self::content_height(self)
    }
}

pub trait ModalOpPickerFooterState {
    fn footer_mode(&self, include_refresh: bool) -> ModalFooterMode;
}

impl<T: OpPickerRenderState> ModalOpPickerFooterState for T {
    fn footer_mode(&self, include_refresh: bool) -> ModalFooterMode {
        op_picker_modal_footer_mode(
            self.stage(),
            self.naming_stage_input().is_some(),
            include_refresh,
        )
    }
}

#[must_use]
pub const fn op_picker_modal_footer_mode(
    stage: OpPickerStage,
    has_naming_stage_input: bool,
    include_refresh: bool,
) -> ModalFooterMode {
    if has_naming_stage_input {
        return ModalFooterMode::OpNamingTextInput;
    }
    match stage {
        OpPickerStage::Section => ModalFooterMode::OpSection,
        OpPickerStage::Field => ModalFooterMode::FilteredPicker {
            include_refresh,
            include_collapse: true,
        },
        _ => ModalFooterMode::FilteredPicker {
            include_refresh,
            include_collapse: false,
        },
    }
}

#[must_use]
pub fn modal_footer_items(mode: ModalFooterMode) -> Vec<HintSpan<'static>> {
    match mode {
        ModalFooterMode::AuthForm {
            focus,
            shows_source_folder,
            shows_credential_block,
            can_generate_token,
        } => {
            let mut items =
                auth_form_footer_items(focus, shows_source_folder, shows_credential_block);
            if can_generate_token {
                super::settings::append_generate_token_footer_item(&mut items);
            }
            items
        }
        ModalFooterMode::ConfirmDismiss | ModalFooterMode::OpNamingTextInput => {
            jackin_tui::components::text_input_hint_spans()
        }
        ModalFooterMode::FileBrowser => Vec::new(),
        ModalFooterMode::MountDestination => super::settings::mount_destination_footer_items(),
        ModalFooterMode::SegmentedChoice => super::settings::segmented_choice_footer_items(),
        ModalFooterMode::PickList { commit_label } => {
            super::settings::pick_list_footer_items(commit_label)
        }
        ModalFooterMode::ConfirmSave { scroll_axes } => confirm_save_footer_items(scroll_axes),
        ModalFooterMode::SaveDiscardCancel => save_discard_cancel_footer_items(),
        ModalFooterMode::ErrorPopup => error_popup_footer_items(),
        // Generic default: no scroll segment. The actual render path (the host
        // console's frame builder) has the dialog rect and re-derives the real
        // axes, so this `none()` only guards a path that never reaches the
        // screen — and even then it never claims an axis the body cannot move.
        ModalFooterMode::ContainerInfo => container_info_footer_items(ScrollAxes::none()),
        ModalFooterMode::StatusPopup => status_popup_footer_items(),
        ModalFooterMode::OpSection => super::settings::op_section_footer_items(),
        ModalFooterMode::FilteredPicker {
            include_refresh,
            include_collapse,
        } => super::settings::filtered_picker_footer_items(include_refresh, include_collapse),
        ModalFooterMode::YesNo => jackin_tui::components::confirm_hint_spans(),
    }
}

#[must_use]
pub fn confirm_save_footer_items(scroll_axes: ScrollAxes) -> Vec<HintSpan<'static>> {
    confirm_save::confirm_save_hint_spans_for_axes(scroll_axes)
}

/// Hint spans for inline yes/no confirm modals (`Modal::Confirm`,
/// `SettingsModal::MountConfirm`, `SettingsModal::EnvConfirm`).
///
/// Delegates to [`jackin_tui::components::confirm_hint_spans`] so this matches
#[must_use]
pub fn save_discard_cancel_footer_items() -> Vec<HintSpan<'static>> {
    save_discard_hint_spans()
}

#[must_use]
pub fn error_popup_footer_items() -> Vec<HintSpan<'static>> {
    error_popup_hint_spans()
}

/// Debug-info modal footer: the *available* scroll axes (per `axes`), dismiss,
/// and click-to-copy. The scroll segment is omitted when the body fits and
/// shows only the axis/axes that actually overflow, so the footer never
/// advertises a scroll direction the operator cannot move.
#[must_use]
pub fn container_info_footer_items(axes: ScrollAxes) -> Vec<HintSpan<'static>> {
    // Delegate to the shared Debug-info hint builder so the console list modal,
    // the launch cockpit, and any future surface render byte-identical hint bars
    // for the same dialog. The UNREGISTERABLE annotations live at the shared
    // definition in `jackin_tui::components::debug_info_hint_spans`.
    jackin_tui::components::debug_info_hint_spans(axes)
}

#[must_use]
pub fn container_info_footer_items_for_dialog(
    content_width: usize,
    content_height: usize,
    dialog_rect: ratatui::layout::Rect,
) -> Vec<HintSpan<'static>> {
    let axes =
        jackin_tui::components::dialog_scroll_axes(content_width, content_height, dialog_rect);
    container_info_footer_items(axes)
}

#[must_use]
pub fn status_popup_footer_items() -> Vec<HintSpan<'static>> {
    vec![HintSpan::Text("working")]
}

#[must_use]
pub fn auth_form_footer_items(
    focus: AuthFormFocus,
    shows_source_folder: bool,
    shows_credential_block: bool,
) -> Vec<HintSpan<'static>> {
    let mut items: Vec<HintSpan<'static>> = match focus {
        AuthFormFocus::Mode => {
            let mut v = vec![
                // UNREGISTERABLE(auth-form-no-keymap): Space cycles mode inline.
                HintSpan::Key("␣"),
                HintSpan::Text("cycle"),
            ];
            if shows_source_folder || shows_credential_block {
                v.extend([
                    HintSpan::Sep,
                    // UNREGISTERABLE(auth-form-no-keymap): Down navigates fields inline.
                    HintSpan::Key("↓"),
                    HintSpan::Text("navigate"),
                ]);
            }
            v.extend([
                HintSpan::GroupSep,
                // UNREGISTERABLE(auth-form-no-keymap): Tab moves to button row inline.
                HintSpan::Key("⇥"),
                HintSpan::Text("button row"),
            ]);
            v
        }
        AuthFormFocus::SourceFolder => vec![
            // UNREGISTERABLE(auth-form-no-keymap): Enter handled inline.
            HintSpan::Key("↵"),
            HintSpan::Text("browse"),
            HintSpan::Sep,
            // UNREGISTERABLE(multi-key-display-group): combined navigate display.
            HintSpan::Key(glyph::UP_DOWN),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            // UNREGISTERABLE(auth-form-no-keymap): Tab moves to button row inline.
            HintSpan::Key("⇥"),
            HintSpan::Text("button row"),
        ],
        AuthFormFocus::CredentialSource => vec![
            // UNREGISTERABLE(auth-form-no-keymap): Enter confirms the field inline.
            HintSpan::Key("↵"),
            HintSpan::Text("set"),
            HintSpan::Sep,
            // UNREGISTERABLE(auth-form-no-keymap): ↑↓ navigates credential source list inline.
            HintSpan::Key("↑"),
            HintSpan::Text("navigate"),
            HintSpan::GroupSep,
            // UNREGISTERABLE(auth-form-no-keymap): Tab moves to button row inline.
            HintSpan::Key("⇥"),
            HintSpan::Text("button row"),
        ],
        AuthFormFocus::Save | AuthFormFocus::Cancel | AuthFormFocus::Reset => vec![
            // UNREGISTERABLE(multi-key-display-group): combined left/right display.
            HintSpan::Key(glyph::LEFT_RIGHT),
            HintSpan::Text("move"),
            HintSpan::GroupSep,
            // UNREGISTERABLE(auth-form-no-keymap): Tab moves to button row inline.
            HintSpan::Key("⇥"),
            HintSpan::Text("fields"),
            HintSpan::GroupSep,
            // UNREGISTERABLE(auth-form-no-keymap): Enter handled inline.
            HintSpan::Key("↵"),
            HintSpan::Text("select"),
        ],
    };
    items.extend([
        HintSpan::GroupSep,
        // UNREGISTERABLE(auth-form-no-keymap): Esc cancels inline.
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]);
    items
}

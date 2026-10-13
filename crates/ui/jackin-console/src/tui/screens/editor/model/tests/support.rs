// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) type TestEditor = EditorState<(), (), (), jackin_config::EnvValue, (), (), (), ()>;

#[derive(Debug)]
pub(super) enum TestStatusModal {
    Status,
    Other,
}

impl EditorStatusPopupModal for TestStatusModal {
    fn is_status_popup(&self) -> bool {
        matches!(self, Self::Status)
    }
}

impl EditorRoleOverridePickerModal for TestStatusModal {
    fn is_role_override_picker(&self) -> bool {
        matches!(self, Self::Other)
    }
}

impl EditorSaveDiscardModal<u8> for TestStatusModal {
    fn save_discard_cancel_modal(state: u8) -> Self {
        if state == 0 {
            Self::Status
        } else {
            Self::Other
        }
    }
}

impl EditorErrorPopupModal<u8> for TestStatusModal {
    fn error_popup_modal(state: u8) -> Self {
        if state == 0 {
            Self::Status
        } else {
            Self::Other
        }
    }
}

pub(super) type TestEditorWithStatusModal =
    EditorState<(), TestStatusModal, (), jackin_config::EnvValue, (), (), (), ()>;

#[derive(Debug)]
pub(super) enum TestAuthModal {
    Auth {
        focus: crate::tui::screens::settings::model::AuthFormFocus,
    },
    Other,
}

impl
    crate::tui::auth_config::ModalAuthFormFocusInspect<
        crate::tui::screens::settings::model::AuthFormFocus,
    > for TestAuthModal
{
    fn active_auth_form_focus(
        &self,
    ) -> Option<crate::tui::screens::settings::model::AuthFormFocus> {
        match self {
            Self::Auth { focus } => Some(*focus),
            Self::Other => None,
        }
    }
}

impl crate::tui::auth_config::ModalAuthFormParentInspect for TestAuthModal {
    fn is_auth_form_parent(&self) -> bool {
        matches!(self, Self::Auth { .. })
    }
}

pub(super) type TestEditorWithAuthModal =
    EditorState<(), TestAuthModal, (), jackin_config::EnvValue, (), (), (), ()>;

pub(super) type TestEditorWithMountCache = EditorState<
    crate::mount_info_cache::MountInfoCache,
    (),
    (),
    jackin_config::EnvValue,
    (),
    (),
    (),
    (),
>;

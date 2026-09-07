// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `ConsoleModal` enum and its core impl methods.
//!
//! Spec carve-out (plan 009): the flow enum and its open/close/result flow
//! stay product-owned — upstream `OverlayStack`/`DismissPolicy` carry
//! geometry, stacking, and dismiss policy only (see `display.rs`).
//!
//! Coordinator — declares the sibling modules `auth_impls` (auth-related
//! trait impls) and `display` (`rect/footer_items` impls). All public
//! types stay reachable from `crate::tui::model::modal::*` — sibling
//! impls use `super::ConsoleModal`.

#[expect(
    unused_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
mod auth_impls;
mod display;

#[derive(Debug)]
pub enum ConsoleModal<
    TextInputTarget,
    TextInputState,
    FileBrowserTarget,
    FileBrowserState,
    MountDstChoiceState,
    WorkdirPickState,
    ConfirmTarget,
    ConfirmState,
    SaveDiscardState,
    GithubPickerState,
    ConfirmSaveState,
    ErrorPopupState,
    ContainerInfoState,
    StatusPopupState,
    OpPickerState,
    RolePickerState,
    SourcePickerState,
    ScopePickerState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
    SecretsScopeTag,
> {
    TextInput {
        target: TextInputTarget,
        state: TextInputState,
    },
    FileBrowser {
        target: FileBrowserTarget,
        state: FileBrowserState,
    },
    MountDstChoice {
        target: FileBrowserTarget,
        state: MountDstChoiceState,
    },
    WorkdirPick {
        state: WorkdirPickState,
    },
    Confirm {
        target: ConfirmTarget,
        state: ConfirmState,
    },
    SaveDiscardCancel {
        state: SaveDiscardState,
    },
    GithubPicker {
        state: GithubPickerState,
    },
    ConfirmSave {
        state: ConfirmSaveState,
    },
    ErrorPopup {
        state: ErrorPopupState,
    },
    ContainerInfo {
        state: ContainerInfoState,
    },
    StatusPopup {
        state: StatusPopupState,
    },
    OpPicker {
        secrets_target: Option<SecretsPickerTarget<SecretsScopeTag>>,
        state: Box<OpPickerState>,
    },
    RolePicker {
        state: RolePickerState,
    },
    RoleOverridePicker {
        state: RolePickerState,
    },
    SourcePicker {
        state: SourcePickerState,
        env_key: Option<(SecretsScopeTag, String)>,
    },
    AuthSourcePicker {
        state: SourcePickerState,
    },
    ScopePicker {
        state: ScopePickerState,
    },
    AuthForm {
        target: AuthFormTarget,
        state: Box<AuthForm>,
        focus: AuthFormFocus,
        literal_buffer: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretsPickerTarget<SecretsScopeTag> {
    Existing { scope: SecretsScopeTag, key: String },
    NewKey { scope: SecretsScopeTag },
}
impl<
    TextInputTarget,
    TextInputState,
    FileBrowserTarget,
    FileBrowserState,
    MountDstChoiceState,
    WorkdirPickState,
    ConfirmTarget,
    ConfirmState,
    SaveDiscardState,
    GithubPickerState,
    ConfirmSaveState,
    ErrorPopupState,
    ContainerInfoState,
    StatusPopupState,
    OpPickerState,
    RolePickerState,
    SourcePickerState,
    ScopePickerState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
    SecretsScopeTag,
>
    ConsoleModal<
        TextInputTarget,
        TextInputState,
        FileBrowserTarget,
        FileBrowserState,
        MountDstChoiceState,
        WorkdirPickState,
        ConfirmTarget,
        ConfirmState,
        SaveDiscardState,
        GithubPickerState,
        ConfirmSaveState,
        ErrorPopupState,
        ContainerInfoState,
        StatusPopupState,
        OpPickerState,
        RolePickerState,
        SourcePickerState,
        ScopePickerState,
        AuthFormTarget,
        AuthForm,
        AuthFormFocus,
        SecretsScopeTag,
    >
{
    #[must_use]
    pub const fn debug_kind(&self) -> crate::tui::debug::ModalDebugKind {
        use crate::tui::debug::ModalDebugKind;
        match self {
            Self::TextInput { .. } => ModalDebugKind::TextInput,
            Self::FileBrowser { .. } => ModalDebugKind::FileBrowser,
            Self::MountDstChoice { .. } => ModalDebugKind::MountDstChoice,
            Self::WorkdirPick { .. } => ModalDebugKind::WorkdirPick,
            Self::Confirm { .. } => ModalDebugKind::Confirm,
            Self::SaveDiscardCancel { .. } => ModalDebugKind::SaveDiscardCancel,
            Self::GithubPicker { .. } => ModalDebugKind::GithubPicker,
            Self::ConfirmSave { .. } => ModalDebugKind::ConfirmSave,
            Self::ErrorPopup { .. } => ModalDebugKind::ErrorPopup,
            Self::StatusPopup { .. } => ModalDebugKind::StatusPopup,
            Self::ContainerInfo { .. } => ModalDebugKind::ContainerInfo,
            Self::OpPicker { .. } => ModalDebugKind::OpPicker,
            Self::RolePicker { .. } => ModalDebugKind::RolePicker,
            Self::RoleOverridePicker { .. } => ModalDebugKind::RoleOverridePicker,
            Self::SourcePicker { .. } => ModalDebugKind::SourcePicker,
            Self::AuthSourcePicker { .. } => ModalDebugKind::AuthSourcePicker,
            Self::ScopePicker { .. } => ModalDebugKind::ScopePicker,
            Self::AuthForm { .. } => ModalDebugKind::AuthForm,
        }
    }

    #[must_use]
    pub const fn list_scroll_target(&self) -> crate::tui::update::ListModalScrollTarget {
        use crate::tui::update::ListModalScrollTarget;
        match self {
            Self::GithubPicker { .. } => ListModalScrollTarget::GithubPicker,
            Self::RolePicker { .. } => ListModalScrollTarget::RolePicker,
            Self::OpPicker { .. } => ListModalScrollTarget::OpPicker,
            _ => ListModalScrollTarget::None,
        }
    }

    #[must_use]
    pub const fn list_key_target(&self) -> crate::tui::update::ListModalKeyTarget {
        use crate::tui::update::ListModalKeyTarget;
        match self {
            Self::GithubPicker { .. } => ListModalKeyTarget::GithubPicker,
            Self::RolePicker { .. } => ListModalKeyTarget::RolePicker,
            Self::ErrorPopup { .. } => ListModalKeyTarget::ErrorPopup,
            Self::ContainerInfo { .. } => ListModalKeyTarget::ContainerInfo,
            _ => ListModalKeyTarget::Dismiss,
        }
    }

    #[must_use]
    pub const fn shared_scroll_target(&self) -> crate::tui::update::SharedModalScrollTarget {
        use crate::tui::update::SharedModalScrollTarget;
        match self {
            Self::WorkdirPick { .. } => SharedModalScrollTarget::WorkdirPick,
            Self::RolePicker { .. } | Self::RoleOverridePicker { .. } => {
                SharedModalScrollTarget::RolePicker
            }
            Self::OpPicker { .. } => SharedModalScrollTarget::OpPicker,
            _ => SharedModalScrollTarget::None,
        }
    }

    #[must_use]
    pub const fn letter_input_kind(&self) -> Option<crate::tui::run::LetterInputModalKind> {
        crate::tui::run::letter_input_modal_kind(
            matches!(self, Self::TextInput { .. }),
            matches!(
                self,
                Self::OpPicker { .. } | Self::RolePicker { .. } | Self::RoleOverridePicker { .. }
            ),
            true,
        )
    }
}

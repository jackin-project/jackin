//! `ConsoleModal` enum and all its trait implementations.

use std::path::PathBuf;

use ratatui::layout::Rect;

use super::create_prelude::{
    CreatePreludeFileBrowserTarget, CreatePreludeModalStep, CreatePreludeTextInputTarget,
    create_prelude_modal_step,
};
use crate::tui::components::footer_hints::{
    ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalContainerInfoFooterState,
    ModalFileBrowserFooterState, ModalFooterMode, ModalOpPickerFooterState,
};
use crate::tui::components::modal_rects::{
    ModalAuthFormState, ModalConfirmSavePrepareState, ModalConfirmSaveState, ModalConfirmState,
    ModalContainerInfoState, ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState,
    ModalRectMode, ModalRolePickerState,
};
use crate::tui::debug::ConsoleModalDebugKind;
use crate::tui::screens::editor::model::{
    EditorErrorPopupModal, EditorRoleOverridePickerModal, EditorSaveDiscardModal,
    EditorStatusPopupModal,
};

#[allow(clippy::large_enum_variant)]
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
        state: Box<OpPickerState>,
    },
    RolePicker {
        state: RolePickerState,
    },
    RoleOverridePicker {
        state: RolePickerState,
    },
    AuthRolePicker {
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
            Self::AuthRolePicker { .. } => ModalDebugKind::AuthRolePicker,
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
            Self::RolePicker { .. }
            | Self::RoleOverridePicker { .. }
            | Self::AuthRolePicker { .. } => SharedModalScrollTarget::RolePicker,
            Self::OpPicker { .. } => SharedModalScrollTarget::OpPicker,
            _ => SharedModalScrollTarget::None,
        }
    }

    #[must_use]
    pub fn create_prelude_step(&self) -> CreatePreludeModalStep
    where
        TextInputTarget: CreatePreludeTextInputTarget,
        FileBrowserTarget: CreatePreludeFileBrowserTarget,
    {
        create_prelude_modal_step(
            matches!(
                self,
                Self::FileBrowser { target, .. } if target.is_create_first_mount_src()
            ),
            matches!(
                self,
                Self::MountDstChoice { target, .. } if target.is_create_first_mount_src()
            ),
            matches!(
                self,
                Self::TextInput { target, .. } if target.is_create_mount_dst()
            ),
            matches!(self, Self::WorkdirPick { .. }),
            matches!(
                self,
                Self::TextInput { target, .. } if target.is_create_workspace_name()
            ),
        )
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

    #[must_use]
    pub fn auth_form_can_generate_token(&self, editing_existing_workspace: bool) -> bool
    where
        AuthFormTarget: crate::tui::auth_config::AuthFormGenerateTarget,
        AuthForm: crate::tui::auth_config::AuthFormGenerateState,
    {
        crate::tui::auth_config::ModalAuthFormGenerate::auth_form_can_generate_token(
            self,
            editing_existing_workspace,
        )
    }
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
> EditorErrorPopupModal<ErrorPopupState>
    for ConsoleModal<
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
    fn error_popup_modal(state: ErrorPopupState) -> Self {
        Self::ErrorPopup { state }
    }
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
> EditorSaveDiscardModal<SaveDiscardState>
    for ConsoleModal<
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
    fn save_discard_cancel_modal(state: SaveDiscardState) -> Self {
        Self::SaveDiscardCancel { state }
    }
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
> EditorRoleOverridePickerModal
    for ConsoleModal<
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
    fn is_role_override_picker(&self) -> bool {
        matches!(self, Self::RoleOverridePicker { .. })
    }
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
> crate::tui::auth_config::ModalAuthFormParentInspect
    for ConsoleModal<
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
    fn is_auth_form_parent(&self) -> bool {
        matches!(self, Self::AuthForm { .. })
    }
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
> crate::tui::auth_config::ModalAuthFormFocusInspect<AuthFormFocus>
    for ConsoleModal<
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
where
    AuthFormFocus: Copy,
{
    fn active_auth_form_focus(&self) -> Option<AuthFormFocus> {
        let Self::AuthForm { focus, .. } = self else {
            return None;
        };
        Some(*focus)
    }
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
> EditorStatusPopupModal
    for ConsoleModal<
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
    fn is_status_popup(&self) -> bool {
        matches!(self, Self::StatusPopup { .. })
    }
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
> ConsoleModalDebugKind
    for ConsoleModal<
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
    fn modal_debug_kind(&self) -> crate::tui::debug::ModalDebugKind {
        self.debug_kind()
    }
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
    crate::tui::auth_config::ModalAuthSourceFolderBrowserOpen<
        FileBrowserTarget,
        FileBrowserState,
        AuthFormFocus,
    >
    for ConsoleModal<
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
where
    AuthForm: crate::tui::auth_config::AuthFormSourceFolderState,
{
    fn open_auth_source_folder_browser<E>(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        source_folder_focus: AuthFormFocus,
        file_browser_target: FileBrowserTarget,
        make_browser: impl FnOnce() -> Result<FileBrowserState, E>,
    ) -> crate::tui::auth_config::AuthSourceFolderBrowserOpenResult<E> {
        let Some(Self::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        }) = modal.take()
        else {
            return crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::NotAvailable;
        };

        if !state.shows_auth_source_folder() {
            *modal = Some(Self::AuthForm {
                target,
                state,
                focus,
                literal_buffer,
            });
            return crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::NotAvailable;
        }

        match make_browser() {
            Ok(browser) => {
                modal_parents.push(Self::AuthForm {
                    target,
                    state,
                    focus: source_folder_focus,
                    literal_buffer,
                });
                *modal = Some(Self::FileBrowser {
                    target: file_browser_target,
                    state: browser,
                });
                crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::Opened
            }
            Err(error) => {
                *modal = Some(Self::AuthForm {
                    target,
                    state,
                    focus,
                    literal_buffer,
                });
                crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::BrowserError(error)
            }
        }
    }
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
> crate::tui::auth_config::ModalAuthOpPickerOpen<OpPickerState, AuthFormFocus>
    for ConsoleModal<
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
    fn open_auth_op_picker(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        credential_focus: AuthFormFocus,
        make_op_picker: impl FnOnce() -> OpPickerState,
    ) -> bool {
        let Some(Self::AuthForm { focus, .. }) = modal_parents.last_mut() else {
            *modal = None;
            return false;
        };
        *focus = credential_focus;
        *modal = Some(Self::OpPicker {
            state: Box::new(make_op_picker()),
        });
        true
    }
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
    OpRef,
> crate::tui::auth_config::ModalAuthFormOpRefApply<AuthFormFocus, OpRef>
    for ConsoleModal<
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
where
    AuthForm: crate::tui::auth_config::AuthFormCredentialEdit<OpRef = OpRef>,
{
    fn apply_auth_op_ref(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        save_focus: AuthFormFocus,
        value: OpRef,
    ) -> bool {
        let Some(Self::AuthForm {
            target,
            mut state,
            literal_buffer,
            ..
        }) = modal_parents.pop()
        else {
            return false;
        };
        state.set_auth_op_ref(value);
        *modal = Some(Self::AuthForm {
            target,
            state,
            focus: save_focus,
            literal_buffer,
        });
        true
    }
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
> crate::tui::auth_config::ModalAuthSourcePickerOpen<SourcePickerState>
    for ConsoleModal<
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
where
    AuthForm: crate::tui::auth_config::AuthFormCredentialSourceState,
{
    fn open_auth_source_picker(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        make_source_picker: impl FnOnce(&'static str) -> SourcePickerState,
    ) -> bool {
        let Some(Self::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        }) = modal.take()
        else {
            return false;
        };

        let Some(env_var) = state.required_credential_env_var() else {
            *modal = Some(Self::AuthForm {
                target,
                state,
                focus,
                literal_buffer,
            });
            return false;
        };

        modal_parents.push(Self::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        });
        *modal = Some(Self::AuthSourcePicker {
            state: make_source_picker(env_var),
        });
        true
    }
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
> crate::tui::auth_config::ModalAuthFormCredentialApply<AuthFormFocus>
    for ConsoleModal<
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
where
    AuthForm: crate::tui::auth_config::AuthFormCredentialEdit,
{
    fn apply_auth_plain_text(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        save_focus: AuthFormFocus,
        value: &str,
    ) -> bool {
        let Some(Self::AuthForm {
            target, mut state, ..
        }) = modal_parents.pop()
        else {
            return false;
        };
        state.set_auth_literal(value.to_owned());
        *modal = Some(Self::AuthForm {
            target,
            state,
            focus: save_focus,
            literal_buffer: value.to_owned(),
        });
        true
    }

    fn apply_auth_source_folder(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        save_focus: AuthFormFocus,
        value: PathBuf,
    ) -> bool {
        let Some(Self::AuthForm {
            target,
            mut state,
            literal_buffer,
            ..
        }) = modal_parents.pop()
        else {
            return false;
        };
        state.set_auth_source_folder(value);
        *modal = Some(Self::AuthForm {
            target,
            state,
            focus: save_focus,
            literal_buffer,
        });
        true
    }

    fn restore_auth_form_modal(modal: &mut Option<Self>, modal_parents: &mut Vec<Self>) -> bool {
        let Some(Self::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        }) = modal_parents.pop()
        else {
            return false;
        };
        *modal = Some(Self::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        });
        true
    }
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
> crate::tui::auth_config::ModalAuthPlainSourceOpen<TextInputTarget, TextInputState, AuthFormFocus>
    for ConsoleModal<
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
    fn open_auth_plain_source_text_input(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        credential_focus: AuthFormFocus,
        text_input_target: TextInputTarget,
        make_text_input: impl FnOnce(String) -> TextInputState,
    ) -> bool {
        let Some(Self::AuthForm {
            target,
            state,
            literal_buffer,
            ..
        }) = modal_parents.pop()
        else {
            return false;
        };
        modal_parents.push(Self::AuthForm {
            target,
            state,
            focus: credential_focus,
            literal_buffer: literal_buffer.clone(),
        });
        *modal = Some(Self::TextInput {
            target: text_input_target,
            state: make_text_input(literal_buffer),
        });
        true
    }
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
> crate::tui::auth_config::ModalAuthTokenGenerateStart<AuthFormTarget, SourcePickerState>
    for ConsoleModal<
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
where
    AuthFormTarget: Clone,
{
    fn open_auth_generate_source_picker(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        source_picker_state: SourcePickerState,
    ) -> Option<AuthFormTarget> {
        let Some(Self::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        }) = modal.take()
        else {
            return None;
        };
        let generate_target = target.clone();
        modal_parents.push(Self::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        });
        *modal = Some(Self::AuthSourcePicker {
            state: source_picker_state,
        });
        Some(generate_target)
    }
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
> crate::tui::auth_config::ModalAuthFormGenerate
    for ConsoleModal<
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
where
    AuthFormTarget: crate::tui::auth_config::AuthFormGenerateTarget,
    AuthForm: crate::tui::auth_config::AuthFormGenerateState,
{
    fn auth_form_can_generate_token(&self, editing_existing_workspace: bool) -> bool {
        let Self::AuthForm { target, state, .. } = self else {
            return false;
        };
        crate::tui::auth_config::auth_form_generate_eligible(
            editing_existing_workspace,
            target,
            state.as_ref(),
        )
    }
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
    pub fn rect_mode(&self, outer: Rect) -> ModalRectMode
    where
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState,
        OpPickerState: ModalOpPickerState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
    {
        match self {
            Self::TextInput { .. } => ModalRectMode::TextInput,
            Self::Confirm { state, .. } => ModalRectMode::Confirm {
                width_pct: state.width_pct(),
                height: state.required_height(),
            },
            Self::SaveDiscardCancel { .. } => ModalRectMode::SaveDiscardCancel,
            Self::FileBrowser { .. } => ModalRectMode::FileBrowser,
            Self::WorkdirPick { .. } => ModalRectMode::WorkdirPick,
            Self::MountDstChoice { .. } => ModalRectMode::MountChoice,
            Self::GithubPicker { state } => ModalRectMode::GithubPicker {
                choice_len: state.choice_len(),
            },
            Self::ConfirmSave { state } => ModalRectMode::ConfirmSave {
                required_height: state.required_height(),
            },
            Self::ErrorPopup { state } => {
                let inner_width = (outer.width * 60 / 100).saturating_sub(4);
                let max_rows = outer.height.saturating_sub(2);
                ModalRectMode::ErrorPopup {
                    required_height: state.required_height(inner_width, max_rows),
                }
            }
            Self::ContainerInfo { state } => ModalRectMode::ContainerInfo {
                required_height: state.required_height(),
            },
            Self::StatusPopup { .. } => ModalRectMode::StatusPopup,
            Self::OpPicker { state } if state.has_naming_stage_input() => ModalRectMode::TextInput,
            Self::OpPicker { .. } => ModalRectMode::OpPicker,
            Self::RolePicker { state }
            | Self::RoleOverridePicker { state }
            | Self::AuthRolePicker { state } => ModalRectMode::RolePicker {
                filtered_len: state.filtered_len(),
            },
            Self::SourcePicker { .. } | Self::AuthSourcePicker { .. } => {
                ModalRectMode::SourcePicker
            }
            Self::ScopePicker { .. } => ModalRectMode::ScopePicker,
            Self::AuthForm { state, .. } => ModalRectMode::AuthForm {
                required_height: state.required_height(),
            },
        }
    }

    #[must_use]
    pub fn rect(&self, outer: Rect) -> Rect
    where
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState,
        OpPickerState: ModalOpPickerState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
    {
        crate::tui::components::modal_rects::modal_rect_for_mode(outer, self.rect_mode(outer))
    }

    #[must_use]
    pub fn container_info_rect(&self, outer: Rect) -> Option<Rect>
    where
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState,
        OpPickerState: ModalOpPickerState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
    {
        if matches!(self, Self::ContainerInfo { .. }) {
            Some(self.rect(outer))
        } else {
            None
        }
    }

    pub fn prepare_for_render(&mut self, outer: Rect)
    where
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState + ModalConfirmSavePrepareState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState,
        OpPickerState: ModalOpPickerState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
    {
        let modal_area = self.rect(outer);
        if let Self::ConfirmSave { state } = self {
            state.prepare_for_render(modal_area);
        }
    }

    #[must_use]
    pub fn footer_items(&self, can_generate_token: bool) -> Vec<jackin_tui::HintSpan<'static>>
    where
        FileBrowserState: ModalFileBrowserFooterState,
        ConfirmSaveState: ModalConfirmSaveFooterState,
        OpPickerState: ModalOpPickerFooterState,
        AuthForm: ModalAuthFormFooterState<AuthFormFocus>,
        AuthFormFocus: Copy,
    {
        match self {
            Self::AuthForm { state, focus, .. } => {
                crate::tui::components::footer_hints::modal_footer_items(
                    state.footer_mode(*focus, can_generate_token),
                )
            }
            Self::FileBrowser { state, .. } => state.footer_items(),
            Self::TextInput { .. } => footer_items_for_mode(ModalFooterMode::ConfirmDismiss),
            Self::MountDstChoice { .. } => footer_items_for_mode(ModalFooterMode::MountDestination),
            Self::SourcePicker { .. }
            | Self::AuthSourcePicker { .. }
            | Self::ScopePicker { .. } => footer_items_for_mode(ModalFooterMode::SegmentedChoice),
            Self::WorkdirPick { .. } => footer_items_for_mode(ModalFooterMode::PickList {
                commit_label: crate::tui::components::footer_hints::pick_list_select_footer_label(),
            }),
            Self::GithubPicker { .. } => footer_items_for_mode(ModalFooterMode::PickList {
                commit_label: crate::tui::components::footer_hints::pick_list_confirm_footer_label(
                ),
            }),
            Self::ConfirmSave { state } => footer_items_for_mode(state.footer_mode()),
            Self::SaveDiscardCancel { .. } => {
                footer_items_for_mode(ModalFooterMode::SaveDiscardCancel)
            }
            Self::ErrorPopup { .. } => footer_items_for_mode(ModalFooterMode::ErrorPopup),
            Self::ContainerInfo { .. } => footer_items_for_mode(ModalFooterMode::ContainerInfo),
            Self::StatusPopup { .. } => footer_items_for_mode(ModalFooterMode::StatusPopup),
            Self::OpPicker { state } => footer_items_for_mode(state.footer_mode(true)),
            Self::RolePicker { .. }
            | Self::RoleOverridePicker { .. }
            | Self::AuthRolePicker { .. } => {
                footer_items_for_mode(ModalFooterMode::FilteredPicker {
                    include_refresh: false,
                    include_collapse: false,
                })
            }
            Self::Confirm { .. } => footer_items_for_mode(ModalFooterMode::YesNo),
        }
    }

    #[must_use]
    pub fn footer_items_for_area(
        &self,
        can_generate_token: bool,
        outer: Rect,
    ) -> Vec<jackin_tui::HintSpan<'static>>
    where
        FileBrowserState: ModalFileBrowserFooterState,
        ConfirmSaveState: ModalConfirmSaveFooterState,
        OpPickerState: ModalOpPickerFooterState,
        AuthForm: ModalAuthFormFooterState<AuthFormFocus>,
        AuthFormFocus: Copy,
        ConfirmState: ModalConfirmState,
        GithubPickerState: ModalGithubPickerState,
        ConfirmSaveState: ModalConfirmSaveState,
        ErrorPopupState: ModalErrorPopupState,
        ContainerInfoState: ModalContainerInfoState + ModalContainerInfoFooterState,
        RolePickerState: ModalRolePickerState,
        AuthForm: ModalAuthFormState,
        OpPickerState: ModalOpPickerState,
    {
        if let Self::ContainerInfo { state } = self {
            return crate::tui::components::footer_hints::container_info_footer_items_for_dialog(
                state.content_width(),
                state.content_height(),
                self.rect(outer),
            );
        }
        self.footer_items(can_generate_token)
    }
}

fn footer_items_for_mode(mode: ModalFooterMode) -> Vec<jackin_tui::HintSpan<'static>> {
    crate::tui::components::footer_hints::modal_footer_items(mode)
}

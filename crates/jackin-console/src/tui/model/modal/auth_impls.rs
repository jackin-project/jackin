//! Auth-related trait impls on `ConsoleModal`. Each impl carries the full
//! 22-type-parameter list and the same where-clause bundle — moved here
//! during the Ledger 2B decomposition so the modal enum stays a thin
//! coordinator and the per-trait dispatch lives next to the trait it
//! implements.

use std::path::PathBuf;

use crate::tui::components::modal_rects::{
    ModalAuthFormState, ModalConfirmSavePrepareState, ModalConfirmSaveState, ModalConfirmState,
    ModalContainerInfoState, ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState,
    ModalRolePickerState,
};
use crate::tui::debug::ConsoleModalDebugKind;
use crate::tui::screens::editor::model::{
    EditorErrorPopupModal, EditorRoleOverridePickerModal, EditorSaveDiscardModal,
    EditorStatusPopupModal,
};

use super::ConsoleModal;
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

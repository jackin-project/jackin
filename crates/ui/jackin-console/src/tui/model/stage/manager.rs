// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ConsoleManagerStage` state and methods.

use super::super::modal::ConsoleModal;
use super::{
    ConsoleAnimationTick, ConsoleCreatePreludeModalPresence, ConsoleEditorFooterHeight,
    ConsoleEditorModalPresence, ConsolePendingDriftCheck, ConsolePendingIsolationCleanup,
    ConsolePendingOpCommit, ConsolePendingOpCommitOrigin, ConsolePendingOpCommitResolution,
    ConsolePendingRoleLoad, ConsoleSettingsFooterHeight, ConsoleSettingsModalPresence,
    ConsoleStageModalFacts,
};
use crate::tui::debug::{
    ConsoleCreatePreludeDebugFacts, ConsoleEditorDebugFacts, ConsoleSettingsDebugFacts,
    ConsoleStageDebug,
};

#[derive(Debug)]
pub enum ConsoleManagerStage<CreatePrelude, Editor, Settings> {
    List,
    Editor(Editor),
    Settings(Settings),
    CreatePrelude(CreatePrelude),
    ConfirmDelete {
        name: String,
        state: crate::tui::components::ConfirmState,
    },
    ConfirmInstancePurge {
        container: String,
        label: String,
        state: crate::tui::components::ConfirmState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleManagerStageRoute {
    List,
    Editor,
    Settings,
    CreatePrelude,
    ConfirmDelete,
    ConfirmInstancePurge,
}

pub trait ConsoleManagerStageState<Stage> {
    fn set_manager_stage(&mut self, stage: Stage);
}

pub fn apply_manager_stage<Stage>(state: &mut impl ConsoleManagerStageState<Stage>, stage: Stage) {
    state.set_manager_stage(stage);
}

impl<CreatePrelude, Editor, Settings> ConsoleManagerStage<CreatePrelude, Editor, Settings> {
    #[must_use]
    pub const fn route(&self) -> ConsoleManagerStageRoute {
        match self {
            Self::List => ConsoleManagerStageRoute::List,
            Self::Editor(_) => ConsoleManagerStageRoute::Editor,
            Self::Settings(_) => ConsoleManagerStageRoute::Settings,
            Self::CreatePrelude(_) => ConsoleManagerStageRoute::CreatePrelude,
            Self::ConfirmDelete { .. } => ConsoleManagerStageRoute::ConfirmDelete,
            Self::ConfirmInstancePurge { .. } => ConsoleManagerStageRoute::ConfirmInstancePurge,
        }
    }
}

impl<CreatePrelude, Editor, Settings> ConsoleManagerStage<CreatePrelude, Editor, Settings>
where
    CreatePrelude: ConsoleCreatePreludeModalPresence,
    Editor: ConsoleEditorModalPresence,
    Settings: ConsoleSettingsModalPresence,
{
    #[must_use]
    pub fn modal_facts(&self) -> ConsoleStageModalFacts {
        match self {
            Self::List => ConsoleStageModalFacts::default(),
            Self::Editor(editor) => ConsoleStageModalFacts {
                editor_modal_open: editor.editor_modal_open(),
                ..ConsoleStageModalFacts::default()
            },
            Self::Settings(settings) => settings.settings_modal_facts(),
            Self::CreatePrelude(prelude) => ConsoleStageModalFacts {
                create_prelude_modal_open: prelude.create_prelude_modal_open(),
                ..ConsoleStageModalFacts::default()
            },
            Self::ConfirmDelete { .. } | Self::ConfirmInstancePurge { .. } => {
                ConsoleStageModalFacts {
                    destructive_confirm_open: true,
                    ..ConsoleStageModalFacts::default()
                }
            }
        }
    }
}

impl<CreatePrelude, Editor, Settings> ConsoleManagerStage<CreatePrelude, Editor, Settings>
where
    Editor: ConsoleEditorFooterHeight,
    Settings: ConsoleSettingsFooterHeight,
{
    #[must_use]
    pub fn footer_height_facts(
        &self,
        workspace_footer_height: u16,
    ) -> crate::tui::view::StageFooterHeightFacts {
        crate::tui::view::StageFooterHeightFacts {
            route: self.route(),
            workspace_footer_height,
            editor_footer_height: match self {
                Self::Editor(editor) => editor.editor_cached_footer_height(),
                _ => 0,
            },
            settings_footer_height: match self {
                Self::Settings(settings) => settings.settings_cached_footer_height(),
                _ => 0,
            },
        }
    }
}

impl<CreatePrelude, Editor, Settings> ConsoleManagerStage<CreatePrelude, Editor, Settings>
where
    Editor: ConsolePendingRoleLoad,
{
    pub fn poll_pending_role_load(
        &mut self,
    ) -> Option<(Editor::PendingRoleLoad, anyhow::Result<()>)> {
        match self {
            Self::Editor(editor) => editor.poll_pending_role_load(),
            Self::List
            | Self::Settings(_)
            | Self::CreatePrelude(_)
            | Self::ConfirmDelete { .. }
            | Self::ConfirmInstancePurge { .. } => None,
        }
    }
}

impl<CreatePrelude, Editor, Settings> ConsoleManagerStage<CreatePrelude, Editor, Settings>
where
    Editor: ConsolePendingDriftCheck,
{
    pub fn poll_pending_drift_check(
        &mut self,
    ) -> Option<(
        Editor::PendingDriftCheck,
        anyhow::Result<Editor::DriftDetection>,
    )> {
        match self {
            Self::Editor(editor) => editor.poll_pending_drift_check(),
            Self::List
            | Self::Settings(_)
            | Self::CreatePrelude(_)
            | Self::ConfirmDelete { .. }
            | Self::ConfirmInstancePurge { .. } => None,
        }
    }
}

impl<CreatePrelude, Editor, Settings> ConsoleManagerStage<CreatePrelude, Editor, Settings>
where
    Editor: ConsolePendingIsolationCleanup,
{
    pub fn poll_pending_isolation_cleanup(
        &mut self,
    ) -> Option<(Editor::PendingIsolationCleanup, anyhow::Result<()>)> {
        match self {
            Self::Editor(editor) => editor.poll_pending_isolation_cleanup(),
            Self::List
            | Self::Settings(_)
            | Self::CreatePrelude(_)
            | Self::ConfirmDelete { .. }
            | Self::ConfirmInstancePurge { .. } => None,
        }
    }
}

impl<CreatePrelude, Editor, Settings, OpRef> ConsoleManagerStage<CreatePrelude, Editor, Settings>
where
    Editor: ConsolePendingOpCommit<OpRef = OpRef>,
    Settings: ConsolePendingOpCommit<OpRef = OpRef>,
{
    pub fn poll_pending_op_commit(&mut self) -> Option<ConsolePendingOpCommitResolution<OpRef>> {
        match self {
            Self::Editor(editor) => editor.poll_pending_op_commit().map(|(op_ref, result)| {
                ConsolePendingOpCommitResolution {
                    op_ref,
                    result,
                    origin: ConsolePendingOpCommitOrigin::Editor,
                }
            }),
            Self::Settings(settings) => {
                settings.poll_pending_op_commit().map(|(op_ref, result)| {
                    ConsolePendingOpCommitResolution {
                        op_ref,
                        result,
                        origin: ConsolePendingOpCommitOrigin::Settings,
                    }
                })
            }
            Self::List
            | Self::CreatePrelude(_)
            | Self::ConfirmDelete { .. }
            | Self::ConfirmInstancePurge { .. } => None,
        }
    }
}

impl<CreatePrelude, Editor, Settings> ConsoleManagerStage<CreatePrelude, Editor, Settings>
where
    Editor: ConsoleAnimationTick,
    Settings: ConsoleAnimationTick,
{
    pub fn tick_active_animation(&mut self) -> bool {
        match self {
            Self::Editor(editor) => editor.tick_active_animation(),
            Self::Settings(settings) => settings.tick_active_animation(),
            Self::List
            | Self::CreatePrelude(_)
            | Self::ConfirmDelete { .. }
            | Self::ConfirmInstancePurge { .. } => false,
        }
    }
}

impl<CreatePrelude, Editor, Settings> ConsoleManagerStage<CreatePrelude, Editor, Settings>
where
    CreatePrelude: ConsoleCreatePreludeDebugFacts,
    Editor: ConsoleEditorDebugFacts,
    Settings: ConsoleSettingsDebugFacts,
{
    #[must_use]
    pub fn debug_stage(&self) -> ConsoleStageDebug {
        match self {
            Self::List => ConsoleStageDebug::List,
            Self::Editor(editor) => editor.editor_stage_debug(),
            Self::Settings(settings) => settings.settings_stage_debug(),
            Self::CreatePrelude(prelude) => prelude.create_prelude_stage_debug(),
            Self::ConfirmDelete { .. } => ConsoleStageDebug::ConfirmDelete,
            Self::ConfirmInstancePurge { .. } => ConsoleStageDebug::ConfirmInstancePurge,
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
> ConsoleAnimationTick
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
    OpPickerState: ConsoleAnimationTick,
{
    fn tick_active_animation(&mut self) -> bool {
        match self {
            Self::OpPicker { state, .. } => state.tick_active_animation(),
            Self::TextInput { .. }
            | Self::FileBrowser { .. }
            | Self::MountDstChoice { .. }
            | Self::WorkdirPick { .. }
            | Self::Confirm { .. }
            | Self::SaveDiscardCancel { .. }
            | Self::GithubPicker { .. }
            | Self::ConfirmSave { .. }
            | Self::ErrorPopup { .. }
            | Self::ContainerInfo { .. }
            | Self::StatusPopup { .. }
            | Self::RolePicker { .. }
            | Self::RoleOverridePicker { .. }
            | Self::SourcePicker { .. }
            | Self::AuthSourcePicker { .. }
            | Self::ScopePicker { .. }
            | Self::AuthForm { .. } => false,
        }
    }
}

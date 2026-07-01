//! `ConsoleManagerStage` and the input-dispatch plan types.
use super::modal::ConsoleModal;
use crate::tui::debug::{
    ConsoleCreatePreludeDebugFacts, ConsoleEditorDebugFacts, ConsoleSettingsDebugFacts,
    ConsoleStageDebug,
};

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ConsoleManagerStage<CreatePrelude, Editor, Settings> {
    List,
    Editor(Editor),
    Settings(Settings),
    CreatePrelude(CreatePrelude),
    ConfirmDelete {
        name: String,
        state: jackin_tui::components::ConfirmState,
    },
    ConfirmInstancePurge {
        container: String,
        label: String,
        state: jackin_tui::components::ConfirmState,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleInputDispatchPlan {
    ListModal,
    InlineNewSessionPicker,
    InlineProviderPicker,
    LaunchProviderPicker,
    InlineAgentPicker,
    InlineRolePicker,
    EditorModal,
    SettingsErrorPopup,
    SettingsMountsModal,
    SettingsEnvModal,
    SettingsAuthModal,
    CreatePreludeModal,
    Stage(ConsoleManagerStageRoute),
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "Twelve orthogonal console-modal-open flags (list_modal, inline \
              pickers, editor_modal, settings pickers, create_prelude_modal) — \
              each is an independent picker-open signal the input dispatch \
              planner reads individually to pick the right dispatch arm. \
              Named-field reads match the per-modal dispatch routing idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsoleInputDispatchFacts {
    pub list_modal_open: bool,
    pub inline_new_session_picker_open: bool,
    pub inline_provider_picker_open: bool,
    pub launch_provider_picker_open: bool,
    pub inline_agent_picker_open: bool,
    pub inline_role_picker_open: bool,
    pub editor_modal_open: bool,
    pub settings_error_popup_open: bool,
    pub settings_mounts_modal_open: bool,
    pub settings_env_modal_open: bool,
    pub settings_auth_modal_open: bool,
    pub create_prelude_modal_open: bool,
    pub stage_route: ConsoleManagerStageRoute,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "Seven orthogonal stage-modal-open flags (editor_modal, settings \
              pickers, create_prelude_modal, destructive_confirm) — each is an \
              independent picker-open signal the stage-modal resolver reads to \
              build the visible stage modal set. Named-field reads match the \
              per-picker stage-modal routing idiom."
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsoleStageModalFacts {
    pub editor_modal_open: bool,
    pub settings_error_popup_open: bool,
    pub settings_mounts_modal_open: bool,
    pub settings_env_modal_open: bool,
    pub settings_auth_modal_open: bool,
    pub create_prelude_modal_open: bool,
    pub destructive_confirm_open: bool,
}

pub trait ConsoleEditorModalPresence {
    fn editor_modal_open(&self) -> bool;
}

pub trait ConsoleEditorFooterHeight {
    fn editor_cached_footer_height(&self) -> u16;
}

pub trait ConsoleSettingsModalPresence {
    fn settings_modal_facts(&self) -> ConsoleStageModalFacts;
}

pub trait ConsoleSettingsFooterHeight {
    fn settings_cached_footer_height(&self) -> u16;
}

pub trait ConsolePendingTokenGenerate {
    type PendingTokenGenerate;

    fn take_pending_token_generate(&mut self) -> Option<Self::PendingTokenGenerate>;
}

pub trait ConsolePendingRoleLoad {
    type PendingRoleLoad;

    fn poll_pending_role_load(&mut self) -> Option<(Self::PendingRoleLoad, anyhow::Result<()>)>;
}

pub trait ConsolePendingDriftCheck {
    type PendingDriftCheck;
    type DriftDetection;

    fn poll_pending_drift_check(
        &mut self,
    ) -> Option<(
        Self::PendingDriftCheck,
        anyhow::Result<Self::DriftDetection>,
    )>;
}

pub trait ConsolePendingIsolationCleanup {
    type PendingIsolationCleanup;

    fn poll_pending_isolation_cleanup(
        &mut self,
    ) -> Option<(Self::PendingIsolationCleanup, anyhow::Result<()>)>;
}

pub trait ConsolePendingOpCommit {
    type OpRef;

    fn poll_pending_op_commit(&mut self) -> Option<(Self::OpRef, anyhow::Result<()>)>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolePendingOpCommitOrigin {
    Editor,
    Settings,
}

#[derive(Debug)]
pub struct ConsolePendingOpCommitResolution<OpRef> {
    pub op_ref: OpRef,
    pub result: anyhow::Result<()>,
    pub origin: ConsolePendingOpCommitOrigin,
}

pub trait ConsoleAnimationTick {
    fn tick_active_animation(&mut self) -> bool;
}

impl<T> ConsoleAnimationTick for Box<T>
where
    T: ConsoleAnimationTick + ?Sized,
{
    fn tick_active_animation(&mut self) -> bool {
        self.as_mut().tick_active_animation()
    }
}

pub trait ConsoleCreatePreludeModalPresence {
    fn create_prelude_modal_open(&self) -> bool;
}

pub trait ConsoleManagerModalBlockPresence {
    fn list_modal_open(&self) -> bool;
    fn editor_modal_open(&self) -> bool;
}

#[must_use]
pub const fn console_input_dispatch_plan(
    facts: ConsoleInputDispatchFacts,
) -> ConsoleInputDispatchPlan {
    if facts.list_modal_open {
        return ConsoleInputDispatchPlan::ListModal;
    }
    if facts.inline_new_session_picker_open {
        return ConsoleInputDispatchPlan::InlineNewSessionPicker;
    }
    if facts.inline_provider_picker_open {
        return ConsoleInputDispatchPlan::InlineProviderPicker;
    }
    if facts.launch_provider_picker_open {
        return ConsoleInputDispatchPlan::LaunchProviderPicker;
    }
    if facts.inline_agent_picker_open {
        return ConsoleInputDispatchPlan::InlineAgentPicker;
    }
    if facts.inline_role_picker_open {
        return ConsoleInputDispatchPlan::InlineRolePicker;
    }
    if facts.editor_modal_open {
        return ConsoleInputDispatchPlan::EditorModal;
    }
    if facts.settings_error_popup_open {
        return ConsoleInputDispatchPlan::SettingsErrorPopup;
    }
    if facts.settings_mounts_modal_open {
        return ConsoleInputDispatchPlan::SettingsMountsModal;
    }
    if facts.settings_env_modal_open {
        return ConsoleInputDispatchPlan::SettingsEnvModal;
    }
    if facts.settings_auth_modal_open {
        return ConsoleInputDispatchPlan::SettingsAuthModal;
    }
    if facts.create_prelude_modal_open {
        return ConsoleInputDispatchPlan::CreatePreludeModal;
    }
    ConsoleInputDispatchPlan::Stage(facts.stage_route)
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

impl<CreatePrelude, Editor, Settings, PendingTokenGenerate>
    ConsoleManagerStage<CreatePrelude, Editor, Settings>
where
    Editor: ConsolePendingTokenGenerate<PendingTokenGenerate = PendingTokenGenerate>,
    Settings: ConsolePendingTokenGenerate<PendingTokenGenerate = PendingTokenGenerate>,
{
    pub fn take_pending_token_generate(&mut self) -> Option<PendingTokenGenerate> {
        match self {
            Self::Editor(editor) => editor.take_pending_token_generate(),
            Self::Settings(settings) => settings.take_pending_token_generate(),
            Self::List
            | Self::CreatePrelude(_)
            | Self::ConfirmDelete { .. }
            | Self::ConfirmInstancePurge { .. } => None,
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
            Self::OpPicker { state } => state.tick_active_animation(),
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
            | Self::AuthRolePicker { .. }
            | Self::SourcePicker { .. }
            | Self::AuthSourcePicker { .. }
            | Self::ScopePicker { .. }
            | Self::AuthForm { .. } => false,
        }
    }
}

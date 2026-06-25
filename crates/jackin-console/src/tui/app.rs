//! Top-level console TUI app model.

use std::path::PathBuf;

use ratatui::layout::Rect;

use crate::tui::components::footer_hints::{
    ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalContainerInfoFooterState,
    ModalFileBrowserFooterState, ModalFooterMode, ModalOpPickerFooterState,
};
use crate::tui::components::modal_rects::{
    ModalAuthFormState, ModalConfirmSavePrepareState, ModalConfirmSaveState, ModalConfirmState,
    ModalContainerInfoState, ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState,
    ModalRectMode, ModalRolePickerState,
};
use crate::tui::debug::{
    ConsoleCreatePreludeDebugFacts, ConsoleEditorDebugFacts, ConsoleModalDebugKind,
    ConsoleSettingsDebugFacts, ConsoleStageDebug,
};
use crate::tui::screens::editor::model::{
    CreateStep, EditorErrorPopupModal, EditorRoleOverridePickerModal, EditorSaveDiscardModal,
    EditorStatusPopupModal,
};

/// Single-variant today; kept as `enum` so future stages can land without
/// churning every match site.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ConsoleAppStage<Manager> {
    Manager(Manager),
}

pub trait LaunchAgentPromptManagerState<RoleSelector, Agent>
where
    Agent: crate::tui::components::agent_choice::AgentChoice,
{
    fn open_launch_agent_prompt(
        &mut self,
        role: RoleSelector,
        picker: crate::tui::components::agent_choice::AgentChoiceState<Agent>,
    );

    fn clear_launch_role_prompt(&mut self);
}

pub fn open_launch_agent_prompt_plan<RoleSelector, Agent>(
    state: &mut impl LaunchAgentPromptState<RoleSelector, Agent>,
    role: RoleSelector,
    choices: Vec<Agent>,
) where
    Agent: crate::tui::components::agent_choice::AgentChoice,
{
    state.open_launch_agent_prompt(role, choices);
}

pub trait LaunchAgentPromptState<RoleSelector, Agent>
where
    Agent: crate::tui::components::agent_choice::AgentChoice,
{
    fn open_launch_agent_prompt(&mut self, role: RoleSelector, choices: Vec<Agent>);
}

impl<Manager, LaunchInput, RoleSelector, OpCache, Agent> LaunchAgentPromptState<RoleSelector, Agent>
    for ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>
where
    Manager: LaunchAgentPromptManagerState<RoleSelector, Agent>,
    RoleSelector: Clone,
    Agent: crate::tui::components::agent_choice::AgentChoice,
{
    fn open_launch_agent_prompt(&mut self, role: RoleSelector, choices: Vec<Agent>) {
        let ConsoleAppStage::Manager(manager) = &mut self.stage;
        manager.open_launch_agent_prompt(
            role.clone(),
            crate::tui::components::agent_choice::AgentChoiceState::with_choices(choices),
        );
        manager.clear_launch_role_prompt();
        self.pending_launch_role = Some(role);
    }
}

pub trait LaunchRolePromptManagerState<RoleSelector>
where
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    fn open_launch_role_prompt(
        &mut self,
        picker: crate::tui::components::role_picker::RolePickerState<RoleSelector>,
    );
}

pub trait LaunchProviderPickerManagerState<RoleSelector, Agent, Provider>
where
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    fn open_launch_provider_picker(
        &mut self,
        picker: crate::tui::components::provider_picker::ProviderPickerState<
            RoleSelector,
            Agent,
            Provider,
        >,
    );
}

pub fn open_launch_role_prompt_plan<LaunchInput, RoleSelector>(
    state: &mut impl LaunchRolePromptState<LaunchInput, RoleSelector>,
    input: LaunchInput,
    roles: Vec<RoleSelector>,
    selected: Option<usize>,
) where
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    state.open_launch_role_prompt(input, roles, selected);
}

pub fn clear_pending_launch_plan<LaunchInput, RoleSelector>(
    state: &mut impl LaunchRolePromptState<LaunchInput, RoleSelector>,
) where
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    state.clear_pending_launch();
}

pub fn clear_pending_launch_role_plan<Manager, LaunchInput, RoleSelector, OpCache>(
    state: &mut ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>,
) {
    state.pending_launch_role = None;
}

pub fn take_pending_launch_plan<Manager, LaunchInput, RoleSelector, OpCache>(
    state: &mut ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>,
) -> Option<LaunchInput> {
    state.pending_launch.take()
}

pub fn take_pending_launch_and_role_plan<Manager, LaunchInput, RoleSelector, OpCache>(
    state: &mut ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>,
) -> Option<(LaunchInput, RoleSelector)> {
    Some((
        state.pending_launch.take()?,
        state.pending_launch_role.take()?,
    ))
}

pub fn store_pending_launch_plan<LaunchInput, RoleSelector>(
    state: &mut impl LaunchRolePromptState<LaunchInput, RoleSelector>,
    input: LaunchInput,
) where
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    state.store_pending_launch(input);
}

pub fn open_launch_provider_picker_plan<LaunchInput, RoleSelector, Agent, Provider>(
    state: &mut impl LaunchProviderPickerState<LaunchInput, RoleSelector, Agent, Provider>,
    input: LaunchInput,
    role: RoleSelector,
    agent: Agent,
    providers: Vec<Provider>,
) where
    RoleSelector: crate::tui::components::role_picker::RoleChoice + Clone,
{
    state.open_launch_provider_picker(input, role, agent, providers);
}

pub trait LaunchRolePromptState<LaunchInput, RoleSelector>
where
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    fn open_launch_role_prompt(
        &mut self,
        input: LaunchInput,
        roles: Vec<RoleSelector>,
        selected: Option<usize>,
    );

    fn clear_pending_launch(&mut self);

    fn store_pending_launch(&mut self, input: LaunchInput);
}

pub trait LaunchProviderPickerState<LaunchInput, RoleSelector, Agent, Provider>
where
    RoleSelector: crate::tui::components::role_picker::RoleChoice + Clone,
{
    fn open_launch_provider_picker(
        &mut self,
        input: LaunchInput,
        role: RoleSelector,
        agent: Agent,
        providers: Vec<Provider>,
    );
}

impl<Manager, LaunchInput, RoleSelector, OpCache> LaunchRolePromptState<LaunchInput, RoleSelector>
    for ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>
where
    Manager: LaunchRolePromptManagerState<RoleSelector>,
    RoleSelector: crate::tui::components::role_picker::RoleChoice,
{
    fn open_launch_role_prompt(
        &mut self,
        input: LaunchInput,
        roles: Vec<RoleSelector>,
        selected: Option<usize>,
    ) {
        let mut picker = crate::tui::components::role_picker::RolePickerState::launch(roles);
        if let Some(selected) = selected {
            picker.list_state.select(Some(selected));
        }
        let ConsoleAppStage::Manager(manager) = &mut self.stage;
        manager.open_launch_role_prompt(picker);
        self.pending_launch = Some(input);
        self.pending_launch_role = None;
    }

    fn clear_pending_launch(&mut self) {
        self.pending_launch = None;
        self.pending_launch_role = None;
    }

    fn store_pending_launch(&mut self, input: LaunchInput) {
        self.pending_launch = Some(input);
    }
}

impl<Manager, LaunchInput, RoleSelector, OpCache, Agent, Provider>
    LaunchProviderPickerState<LaunchInput, RoleSelector, Agent, Provider>
    for ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>
where
    Manager: LaunchProviderPickerManagerState<RoleSelector, Agent, Provider>,
    RoleSelector: crate::tui::components::role_picker::RoleChoice + Clone,
{
    fn open_launch_provider_picker(
        &mut self,
        input: LaunchInput,
        role: RoleSelector,
        agent: Agent,
        providers: Vec<Provider>,
    ) {
        let picker = crate::tui::components::provider_picker::ProviderPickerState::new(
            role.clone(),
            agent,
            providers,
        );
        let ConsoleAppStage::Manager(manager) = &mut self.stage;
        manager.open_launch_provider_picker(picker);
        self.pending_launch = Some(input);
        self.pending_launch_role = Some(role);
    }
}

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

#[derive(Debug)]
pub struct ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache> {
    pub stage: ConsoleAppStage<Manager>,
    /// Launch input is stored as a value, not as a selected row index, so each
    /// dispatch can rebuild its current workspace choice from current config.
    pub pending_launch: Option<LaunchInput>,
    pub pending_launch_role: Option<RoleSelector>,
    /// Process-lifetime op metadata cache shared by picker instances.
    pub op_cache: OpCache,
    /// Probed once at startup; mid-session installs require restart.
    pub op_available: bool,
    /// Overlay above any sub-stage.
    pub quit_confirm: Option<jackin_tui::components::ConfirmState>,
}

#[derive(Debug)]
pub struct ConsoleCreatePreludeState<Modal> {
    pub step: CreateStep,
    pub pending_mount_src: Option<PathBuf>,
    pub pending_mount_dst: Option<String>,
    pub pending_readonly: bool,
    pub pending_workdir: Option<String>,
    pub pending_name: Option<String>,
    pub modal: Option<Modal>,
    /// Captured so Esc on `MountDstChoice` re-opens `FileBrowser` at the same
    /// directory instead of `$HOME`.
    pub last_browser_cwd: Option<PathBuf>,
    /// Picks Esc-on-`WorkdirPick` rewind target: `TextInputDst` when the
    /// Edit-destination branch was used, else `MountDstChoice`.
    pub used_edit_dst: bool,
}

impl<Modal> ConsoleCreatePreludeModalPresence for ConsoleCreatePreludeState<Modal> {
    fn create_prelude_modal_open(&self) -> bool {
        self.modal.is_some()
    }
}

impl<Modal> ConsoleCreatePreludeDebugFacts for ConsoleCreatePreludeState<Modal>
where
    Modal: ConsoleModalDebugKind,
{
    fn create_prelude_stage_debug(&self) -> ConsoleStageDebug {
        ConsoleStageDebug::CreatePrelude {
            step: format!("{:?}", self.step),
            modal: self
                .modal
                .as_ref()
                .map(ConsoleModalDebugKind::modal_debug_kind),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeCompletionStatus {
    InProgress,
    Complete,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeKeyPlan {
    Continue,
    ReturnToList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeModalStep {
    FileBrowserSrc,
    MountDstChoice,
    TextInputDst,
    WorkdirPick,
    TextInputName,
    Other,
}

pub trait CreatePreludeFileBrowserTarget {
    fn is_create_first_mount_src(&self) -> bool;
}

pub trait CreatePreludeTextInputTarget {
    fn is_create_mount_dst(&self) -> bool;
    fn is_create_workspace_name(&self) -> bool;
}

impl CreatePreludeFileBrowserTarget for crate::tui::screens::editor::model::FileBrowserTarget {
    fn is_create_first_mount_src(&self) -> bool {
        matches!(self, Self::CreateFirstMountSrc)
    }
}

impl CreatePreludeTextInputTarget for crate::tui::screens::editor::model::TextInputTarget {
    fn is_create_mount_dst(&self) -> bool {
        matches!(self, Self::MountDst)
    }

    fn is_create_workspace_name(&self) -> bool {
        matches!(self, Self::Name)
    }
}

#[must_use]
pub const fn create_prelude_modal_step(
    file_browser_src: bool,
    mount_dst_choice: bool,
    text_input_dst: bool,
    workdir_pick: bool,
    text_input_name: bool,
) -> CreatePreludeModalStep {
    if file_browser_src {
        CreatePreludeModalStep::FileBrowserSrc
    } else if mount_dst_choice {
        CreatePreludeModalStep::MountDstChoice
    } else if text_input_dst {
        CreatePreludeModalStep::TextInputDst
    } else if workdir_pick {
        CreatePreludeModalStep::WorkdirPick
    } else if text_input_name {
        CreatePreludeModalStep::TextInputName
    } else {
        CreatePreludeModalStep::Other
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatePreludeFileBrowserPlan<T> {
    CancelPrelude,
    ResolveGitUrl(PathBuf),
    OpenUrl(String),
    ApplyFileBrowserOutcome(crate::tui::components::file_browser::FileBrowserOutcome<T>),
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeWorkdirCancelPlan {
    ReopenTextInputDst,
    ReopenMountDstChoice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatePreludeWorkdirPickPlan<T> {
    Commit(T),
    ReopenTextInputDst,
    ReopenMountDstChoice,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatePreludeMountDstChoicePlan {
    CommitSamePath,
    OpenEditInput,
    ReopenFileBrowserAtLastCwd,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatePreludeTextInputDstPlan<T> {
    Commit(T),
    ReopenMountDstChoice,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatePreludeTextInputNamePlan<T> {
    Commit(T),
    ReopenWorkdirPick,
    Continue,
}

#[must_use]
pub fn create_prelude_text_input_dst_plan<T>(
    outcome: jackin_tui::ModalOutcome<T>,
) -> CreatePreludeTextInputDstPlan<T> {
    match outcome {
        jackin_tui::ModalOutcome::Commit(dst) => CreatePreludeTextInputDstPlan::Commit(dst),
        jackin_tui::ModalOutcome::Cancel => CreatePreludeTextInputDstPlan::ReopenMountDstChoice,
        jackin_tui::ModalOutcome::Continue => CreatePreludeTextInputDstPlan::Continue,
    }
}

#[must_use]
pub fn create_prelude_text_input_name_plan<T>(
    outcome: jackin_tui::ModalOutcome<T>,
) -> CreatePreludeTextInputNamePlan<T> {
    match outcome {
        jackin_tui::ModalOutcome::Commit(name) => CreatePreludeTextInputNamePlan::Commit(name),
        jackin_tui::ModalOutcome::Cancel => CreatePreludeTextInputNamePlan::ReopenWorkdirPick,
        jackin_tui::ModalOutcome::Continue => CreatePreludeTextInputNamePlan::Continue,
    }
}

#[must_use]
pub fn create_prelude_workdir_pick_plan<T>(
    outcome: jackin_tui::ModalOutcome<T>,
    used_edit_dst: bool,
) -> CreatePreludeWorkdirPickPlan<T> {
    match outcome {
        jackin_tui::ModalOutcome::Commit(workdir) => CreatePreludeWorkdirPickPlan::Commit(workdir),
        jackin_tui::ModalOutcome::Cancel => match create_prelude_workdir_cancel_plan(used_edit_dst)
        {
            CreatePreludeWorkdirCancelPlan::ReopenTextInputDst => {
                CreatePreludeWorkdirPickPlan::ReopenTextInputDst
            }
            CreatePreludeWorkdirCancelPlan::ReopenMountDstChoice => {
                CreatePreludeWorkdirPickPlan::ReopenMountDstChoice
            }
        },
        jackin_tui::ModalOutcome::Continue => CreatePreludeWorkdirPickPlan::Continue,
    }
}

#[must_use]
pub fn create_prelude_file_browser_plan<T>(
    outcome: crate::tui::components::file_browser::FileBrowserOutcome<T>,
) -> CreatePreludeFileBrowserPlan<T> {
    match outcome {
        crate::tui::components::file_browser::FileBrowserOutcome::Cancel => {
            CreatePreludeFileBrowserPlan::CancelPrelude
        }
        crate::tui::components::file_browser::FileBrowserOutcome::ResolveGitUrl(path) => {
            CreatePreludeFileBrowserPlan::ResolveGitUrl(path)
        }
        crate::tui::components::file_browser::FileBrowserOutcome::OpenGitUrl(url) => {
            CreatePreludeFileBrowserPlan::OpenUrl(url)
        }
        crate::tui::components::file_browser::FileBrowserOutcome::Continue => {
            CreatePreludeFileBrowserPlan::Continue
        }
        crate::tui::components::file_browser::FileBrowserOutcome::Commit(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::NavigateTo(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::NavigateUp
        | crate::tui::components::file_browser::FileBrowserOutcome::RequestCommit(_) => {
            CreatePreludeFileBrowserPlan::ApplyFileBrowserOutcome(outcome)
        }
    }
}

#[must_use]
pub const fn create_prelude_mount_dst_choice_plan(
    outcome: jackin_tui::ModalOutcome<crate::tui::components::mount_dst_choice::MountDstChoice>,
) -> CreatePreludeMountDstChoicePlan {
    match outcome {
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::mount_dst_choice::MountDstChoice::SamePath,
        ) => CreatePreludeMountDstChoicePlan::CommitSamePath,
        jackin_tui::ModalOutcome::Commit(
            crate::tui::components::mount_dst_choice::MountDstChoice::Edit,
        ) => CreatePreludeMountDstChoicePlan::OpenEditInput,
        jackin_tui::ModalOutcome::Cancel => {
            CreatePreludeMountDstChoicePlan::ReopenFileBrowserAtLastCwd
        }
        jackin_tui::ModalOutcome::Continue => CreatePreludeMountDstChoicePlan::Continue,
    }
}

#[must_use]
pub const fn create_prelude_workdir_cancel_plan(
    used_edit_dst: bool,
) -> CreatePreludeWorkdirCancelPlan {
    if used_edit_dst {
        CreatePreludeWorkdirCancelPlan::ReopenTextInputDst
    } else {
        CreatePreludeWorkdirCancelPlan::ReopenMountDstChoice
    }
}

#[must_use]
pub const fn create_prelude_key_plan(key: crossterm::event::KeyCode) -> CreatePreludeKeyPlan {
    match key {
        crossterm::event::KeyCode::Esc => CreatePreludeKeyPlan::ReturnToList,
        _ => CreatePreludeKeyPlan::Continue,
    }
}

#[must_use]
pub const fn create_prelude_completion_status(
    modal_open: bool,
    completed: bool,
) -> CreatePreludeCompletionStatus {
    if modal_open {
        CreatePreludeCompletionStatus::InProgress
    } else if completed {
        CreatePreludeCompletionStatus::Complete
    } else {
        CreatePreludeCompletionStatus::Cancelled
    }
}

impl<Modal> Default for ConsoleCreatePreludeState<Modal> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Modal> ConsoleCreatePreludeState<Modal> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            step: CreateStep::PickFirstMountSrc,
            pending_mount_src: None,
            pending_mount_dst: None,
            pending_readonly: false,
            pending_workdir: None,
            pending_name: None,
            modal: None,
            last_browser_cwd: None,
            used_edit_dst: false,
        }
    }

    pub fn accept_mount_src(&mut self, src: PathBuf) {
        self.pending_mount_src = Some(src);
        self.step = CreateStep::PickFirstMountDst;
    }

    /// Default mount dst = same absolute path as host src. Operator can
    /// overwrite in the dst modal.
    #[must_use]
    pub fn default_mount_dst(&self) -> String {
        let src_display = self
            .pending_mount_src
            .as_ref()
            .map(|path| path.display().to_string());
        crate::tui::screens::workspaces::view::create_prelude_mount_destination_default(
            src_display.as_deref(),
        )
    }

    pub fn accept_mount_dst(&mut self, dst: String, readonly: bool) {
        self.pending_mount_dst = Some(dst);
        self.pending_readonly = readonly;
        self.step = CreateStep::PickWorkdir;
    }

    pub fn accept_workdir(&mut self, workdir: String) {
        self.pending_workdir = Some(workdir);
        self.step = CreateStep::NameWorkspace;
    }

    /// Default name = mount dst basename.
    #[must_use]
    pub fn default_name(&self) -> String {
        crate::tui::screens::workspaces::view::create_prelude_workspace_name_default(
            self.pending_mount_dst.as_deref(),
        )
    }

    pub fn accept_name(&mut self, name: String) {
        self.pending_name = Some(name);
    }

    pub fn open_workdir_pick_from_pending_mount(
        &mut self,
        make_modal: impl FnOnce(jackin_config::MountConfig) -> Modal,
    ) -> bool {
        let Some(mount) = self.pending_first_mount() else {
            return false;
        };
        self.modal = Some(make_modal(mount));
        true
    }

    pub fn reopen_mount_dst_choice(&mut self, make_modal: impl FnOnce(String) -> Modal) {
        let src = self.default_mount_dst();
        self.modal = Some(make_modal(src));
    }

    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.pending_name.as_deref()
    }

    #[must_use]
    pub fn pending_first_mount(&self) -> Option<jackin_config::MountConfig> {
        Some(crate::services::workspace::shared_mount_config(
            self.pending_mount_src.as_ref()?.display().to_string(),
            self.pending_mount_dst.clone()?,
            self.pending_readonly,
        ))
    }

    #[must_use]
    pub fn build_workspace(&self) -> Option<jackin_config::WorkspaceConfig> {
        let workdir = self.pending_workdir.as_ref()?;

        Some(jackin_config::WorkspaceConfig {
            workdir: workdir.clone(),
            mounts: vec![self.pending_first_mount()?],
            ..jackin_config::WorkspaceConfig::default()
        })
    }

    #[must_use]
    pub fn completed(&self) -> Option<(String, jackin_config::WorkspaceConfig)> {
        let name = self.pending_name.clone()?;
        let workspace = self.build_workspace()?;
        Some((name, workspace))
    }
}

impl<Manager, LaunchInput, RoleSelector, OpCache>
    ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>
{
    pub fn new(stage: ConsoleAppStage<Manager>, op_cache: OpCache, op_available: bool) -> Self {
        Self {
            stage,
            pending_launch: None,
            pending_launch_role: None,
            op_cache,
            op_available,
            quit_confirm: None,
        }
    }

    #[must_use]
    pub fn quit_confirm_state(&self) -> Option<&jackin_tui::components::ConfirmState> {
        self.quit_confirm.as_ref()
    }

    #[must_use]
    pub fn quit_confirm_open(&self) -> bool {
        self.quit_confirm.is_some()
    }

    pub fn open_quit_confirm(&mut self) {
        self.quit_confirm = Some(crate::tui::run::quit_confirm_state());
    }

    pub fn dismiss_quit_confirm(&mut self) {
        self.quit_confirm = None;
    }

    pub fn handle_quit_confirm_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<crate::tui::run::QuitConfirmPlan> {
        let confirm = self.quit_confirm.as_mut()?;
        let plan = crate::tui::run::quit_confirm_plan(confirm.handle_key(key));
        if matches!(plan, crate::tui::run::QuitConfirmPlan::Dismiss) {
            self.dismiss_quit_confirm();
        }
        Some(plan)
    }
}

impl<Manager, LaunchInput, RoleSelector, OpCache>
    ConsoleApp<Manager, LaunchInput, RoleSelector, OpCache>
where
    Manager: ConsoleManagerModalBlockPresence,
{
    #[must_use]
    pub fn base_surface_unblocked(&self) -> bool {
        match &self.stage {
            ConsoleAppStage::Manager(manager) => {
                crate::tui::run::no_modal_blocks_base_surface(crate::tui::run::ModalBlockState {
                    quit_confirm: self.quit_confirm.is_some(),
                    list_modal: manager.list_modal_open(),
                    editor_modal: manager.editor_modal_open(),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use jackin_config::MountIsolation;
    use ratatui::layout::Rect;

    use crate::tui::components::footer_hints::{
        ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalContainerInfoFooterState,
        ModalFileBrowserFooterState, ModalFooterMode, ModalOpPickerFooterState,
    };
    use crate::tui::components::modal_rects::{
        ModalAuthFormState, ModalConfirmSaveState, ModalConfirmState, ModalContainerInfoState,
        ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState, ModalRectMode,
        ModalRolePickerState,
    };
    use crate::tui::debug::{ConsoleStageDebug, ModalDebugKind};
    use crate::tui::screens::editor::model::CreateStep;

    use super::{
        ConsoleAnimationTick, ConsoleApp, ConsoleAppStage, ConsoleCreatePreludeState,
        ConsoleInputDispatchFacts, ConsoleInputDispatchPlan, ConsoleManagerStage,
        ConsoleManagerStageRoute, ConsoleManagerStageState, ConsoleModal, ConsoleStageModalFacts,
        CreatePreludeCompletionStatus, CreatePreludeFileBrowserPlan, CreatePreludeKeyPlan,
        CreatePreludeModalStep, CreatePreludeMountDstChoicePlan, CreatePreludeTextInputDstPlan,
        CreatePreludeTextInputNamePlan, CreatePreludeWorkdirCancelPlan,
        CreatePreludeWorkdirPickPlan, apply_manager_stage, console_input_dispatch_plan,
        create_prelude_completion_status, create_prelude_file_browser_plan,
        create_prelude_key_plan, create_prelude_modal_step, create_prelude_mount_dst_choice_plan,
        create_prelude_text_input_dst_plan, create_prelude_text_input_name_plan,
        create_prelude_workdir_cancel_plan, create_prelude_workdir_pick_plan,
    };

    struct TestConfirm;

    struct TestEditor {
        modal_open: bool,
        footer_height: u16,
    }

    impl super::ConsoleEditorModalPresence for TestEditor {
        fn editor_modal_open(&self) -> bool {
            self.modal_open
        }
    }

    impl super::ConsoleEditorFooterHeight for TestEditor {
        fn editor_cached_footer_height(&self) -> u16 {
            self.footer_height
        }
    }

    impl super::ConsoleEditorDebugFacts for TestEditor {
        fn editor_stage_debug(&self) -> ConsoleStageDebug {
            ConsoleStageDebug::Editor {
                mode: "TestMode".to_owned(),
                tab: "TestTab".to_owned(),
                field: "TestField".to_owned(),
                modal: self.modal_open.then_some(ModalDebugKind::TextInput),
            }
        }
    }

    struct TestSettings {
        facts: ConsoleStageModalFacts,
        footer_height: u16,
    }

    impl super::ConsoleSettingsModalPresence for TestSettings {
        fn settings_modal_facts(&self) -> ConsoleStageModalFacts {
            self.facts
        }
    }

    impl super::ConsoleSettingsFooterHeight for TestSettings {
        fn settings_cached_footer_height(&self) -> u16 {
            self.footer_height
        }
    }

    impl super::ConsoleSettingsDebugFacts for TestSettings {
        fn settings_stage_debug(&self) -> ConsoleStageDebug {
            ConsoleStageDebug::Settings {
                tab: "Mounts".to_owned(),
                selected: 2,
                modal: None,
            }
        }
    }

    struct TestTokenDrain {
        pending: Option<u8>,
    }

    impl super::ConsolePendingTokenGenerate for TestTokenDrain {
        type PendingTokenGenerate = u8;

        fn take_pending_token_generate(&mut self) -> Option<Self::PendingTokenGenerate> {
            self.pending.take()
        }
    }

    struct TestRoleLoad {
        pending: Option<u8>,
    }

    impl super::ConsolePendingRoleLoad for TestRoleLoad {
        type PendingRoleLoad = u8;

        fn poll_pending_role_load(
            &mut self,
        ) -> Option<(Self::PendingRoleLoad, anyhow::Result<()>)> {
            self.pending.take().map(|pending| (pending, Ok(())))
        }
    }

    struct TestDriftCheck {
        pending: Option<(u8, &'static str)>,
    }

    impl super::ConsolePendingDriftCheck for TestDriftCheck {
        type PendingDriftCheck = u8;
        type DriftDetection = &'static str;

        fn poll_pending_drift_check(
            &mut self,
        ) -> Option<(
            Self::PendingDriftCheck,
            anyhow::Result<Self::DriftDetection>,
        )> {
            self.pending
                .take()
                .map(|(pending, detection)| (pending, Ok(detection)))
        }
    }

    struct TestIsolationCleanup {
        pending: Option<u8>,
    }

    impl super::ConsolePendingIsolationCleanup for TestIsolationCleanup {
        type PendingIsolationCleanup = u8;

        fn poll_pending_isolation_cleanup(
            &mut self,
        ) -> Option<(Self::PendingIsolationCleanup, anyhow::Result<()>)> {
            self.pending.take().map(|pending| (pending, Ok(())))
        }
    }

    struct TestOpCommit {
        pending: Option<(u8, anyhow::Result<()>)>,
    }

    impl super::ConsolePendingOpCommit for TestOpCommit {
        type OpRef = u8;

        fn poll_pending_op_commit(&mut self) -> Option<(Self::OpRef, anyhow::Result<()>)> {
            self.pending.take()
        }
    }

    struct TestDebugModal;

    impl super::ConsoleModalDebugKind for TestDebugModal {
        fn modal_debug_kind(&self) -> ModalDebugKind {
            ModalDebugKind::ErrorPopup
        }
    }

    #[derive(Debug)]
    struct TestManager {
        list_modal_open: bool,
        editor_modal_open: bool,
    }

    impl super::ConsoleManagerModalBlockPresence for TestManager {
        fn list_modal_open(&self) -> bool {
            self.list_modal_open
        }

        fn editor_modal_open(&self) -> bool {
            self.editor_modal_open
        }
    }

    #[derive(Debug, Default)]
    struct TestLaunchPromptManager {
        opened_role: Option<&'static str>,
        picker_choices: Vec<jackin_core::Agent>,
        role_prompt_cleared: bool,
        role_picker_keys: Vec<&'static str>,
        role_picker_selected: Option<usize>,
        role_picker_confirm_label: String,
        provider_picker_role: Option<TestPromptRole>,
        provider_picker_agent: Option<jackin_core::Agent>,
        provider_picker_providers: Vec<&'static str>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestPromptRole(&'static str);

    impl crate::tui::components::role_picker::RoleChoice for TestPromptRole {
        fn key(&self) -> String {
            self.0.to_owned()
        }
    }

    impl super::LaunchAgentPromptManagerState<&'static str, jackin_core::Agent>
        for TestLaunchPromptManager
    {
        fn open_launch_agent_prompt(
            &mut self,
            role: &'static str,
            picker: crate::tui::components::agent_choice::AgentChoiceState<jackin_core::Agent>,
        ) {
            self.opened_role = Some(role);
            self.picker_choices = picker.choices;
        }

        fn clear_launch_role_prompt(&mut self) {
            self.role_prompt_cleared = true;
        }
    }

    impl super::LaunchRolePromptManagerState<TestPromptRole> for TestLaunchPromptManager {
        fn open_launch_role_prompt(
            &mut self,
            picker: crate::tui::components::role_picker::RolePickerState<TestPromptRole>,
        ) {
            self.role_picker_keys = picker.roles.iter().map(|role| role.0).collect();
            self.role_picker_selected = picker.list_state.selected;
            self.role_picker_confirm_label = picker.confirm_label;
        }
    }

    impl super::LaunchProviderPickerManagerState<TestPromptRole, jackin_core::Agent, &'static str>
        for TestLaunchPromptManager
    {
        fn open_launch_provider_picker(
            &mut self,
            picker: crate::tui::components::provider_picker::ProviderPickerState<
                TestPromptRole,
                jackin_core::Agent,
                &'static str,
            >,
        ) {
            let providers = picker.providers().to_vec();
            self.provider_picker_role = Some(picker.context);
            self.provider_picker_agent = Some(picker.agent);
            self.provider_picker_providers = providers;
        }
    }

    #[test]
    fn open_launch_agent_prompt_plan_updates_app_and_manager() {
        let mut app: ConsoleApp<TestLaunchPromptManager, (), &'static str, ()> = ConsoleApp::new(
            ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
            (),
            false,
        );

        super::open_launch_agent_prompt_plan(
            &mut app,
            "architect",
            vec![jackin_core::Agent::Claude],
        );

        assert_eq!(app.pending_launch_role, Some("architect"));
        let ConsoleAppStage::Manager(manager) = app.stage;
        assert_eq!(manager.opened_role, Some("architect"));
        assert_eq!(manager.picker_choices, vec![jackin_core::Agent::Claude]);
        assert!(manager.role_prompt_cleared);
    }

    #[test]
    fn open_launch_role_prompt_plan_updates_app_and_manager() {
        let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
            ConsoleApp::new(
                ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
                (),
                false,
            );

        super::open_launch_role_prompt_plan(
            &mut app,
            "workspace-input",
            vec![TestPromptRole("architect"), TestPromptRole("reviewer")],
            Some(1),
        );

        assert_eq!(app.pending_launch, Some("workspace-input"));
        assert_eq!(app.pending_launch_role, None);
        let ConsoleAppStage::Manager(manager) = app.stage;
        assert_eq!(manager.role_picker_keys, vec!["architect", "reviewer"]);
        assert_eq!(manager.role_picker_selected, Some(1));
        assert_eq!(manager.role_picker_confirm_label, "launch");
    }

    #[test]
    fn clear_pending_launch_plan_clears_launch_state() {
        let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
            ConsoleApp::new(
                ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
                (),
                false,
            );
        app.pending_launch = Some("workspace-input");
        app.pending_launch_role = Some(TestPromptRole("architect"));

        super::clear_pending_launch_plan(&mut app);

        assert_eq!(app.pending_launch, None);
        assert_eq!(app.pending_launch_role, None);
    }

    #[test]
    fn store_pending_launch_plan_sets_launch_input() {
        let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
            ConsoleApp::new(
                ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
                (),
                false,
            );

        super::store_pending_launch_plan(&mut app, "workspace-input");

        assert_eq!(app.pending_launch, Some("workspace-input"));
    }

    #[test]
    fn clear_pending_launch_role_plan_clears_only_role() {
        let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
            ConsoleApp::new(
                ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
                (),
                false,
            );
        app.pending_launch = Some("workspace-input");
        app.pending_launch_role = Some(TestPromptRole("architect"));

        super::clear_pending_launch_role_plan(&mut app);

        assert_eq!(app.pending_launch, Some("workspace-input"));
        assert_eq!(app.pending_launch_role, None);
    }

    #[test]
    fn take_pending_launch_plan_takes_input() {
        let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
            ConsoleApp::new(
                ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
                (),
                false,
            );
        app.pending_launch = Some("workspace-input");

        assert_eq!(
            super::take_pending_launch_plan(&mut app),
            Some("workspace-input")
        );
        assert_eq!(app.pending_launch, None);
    }

    #[test]
    fn take_pending_launch_and_role_plan_takes_pair() {
        let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
            ConsoleApp::new(
                ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
                (),
                false,
            );
        app.pending_launch = Some("workspace-input");
        app.pending_launch_role = Some(TestPromptRole("architect"));

        assert_eq!(
            super::take_pending_launch_and_role_plan(&mut app),
            Some(("workspace-input", TestPromptRole("architect")))
        );
        assert_eq!(app.pending_launch, None);
        assert_eq!(app.pending_launch_role, None);
    }

    #[test]
    fn open_launch_provider_picker_plan_updates_app_and_manager() {
        let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
            ConsoleApp::new(
                ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
                (),
                false,
            );

        super::open_launch_provider_picker_plan(
            &mut app,
            "workspace-input",
            TestPromptRole("architect"),
            jackin_core::Agent::Claude,
            vec!["anthropic", "zai"],
        );

        assert_eq!(app.pending_launch, Some("workspace-input"));
        assert_eq!(app.pending_launch_role, Some(TestPromptRole("architect")));
        let ConsoleAppStage::Manager(manager) = app.stage;
        assert_eq!(
            manager.provider_picker_role,
            Some(TestPromptRole("architect"))
        );
        assert_eq!(
            manager.provider_picker_agent,
            Some(jackin_core::Agent::Claude)
        );
        assert_eq!(manager.provider_picker_providers, vec!["anthropic", "zai"]);
    }

    impl ModalConfirmState for TestConfirm {
        fn width_pct(&self) -> u16 {
            42
        }

        fn required_height(&self) -> u16 {
            9
        }
    }

    #[test]
    fn console_app_base_surface_unblocked_respects_modal_blockers() {
        let mut app: ConsoleApp<TestManager, (), (), ()> = ConsoleApp::new(
            ConsoleAppStage::Manager(TestManager {
                list_modal_open: false,
                editor_modal_open: false,
            }),
            (),
            false,
        );

        assert!(app.base_surface_unblocked());

        app.open_quit_confirm();
        assert!(!app.base_surface_unblocked());

        app.dismiss_quit_confirm();
        app.stage = ConsoleAppStage::Manager(TestManager {
            list_modal_open: true,
            editor_modal_open: false,
        });
        assert!(!app.base_surface_unblocked());

        app.stage = ConsoleAppStage::Manager(TestManager {
            list_modal_open: false,
            editor_modal_open: true,
        });
        assert!(!app.base_surface_unblocked());
    }

    #[test]
    fn console_app_quit_confirm_key_dismisses_dialog() {
        let mut app: ConsoleApp<TestManager, (), (), ()> = ConsoleApp::new(
            ConsoleAppStage::Manager(TestManager {
                list_modal_open: false,
                editor_modal_open: false,
            }),
            (),
            false,
        );

        app.open_quit_confirm();

        let plan = app.handle_quit_confirm_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ));

        assert_eq!(plan, Some(crate::tui::run::QuitConfirmPlan::Dismiss));
        assert!(!app.quit_confirm_open());
    }

    #[test]
    fn console_manager_stage_routes_by_variant() {
        assert_eq!(
            ConsoleManagerStage::<(), (), ()>::List.route(),
            ConsoleManagerStageRoute::List
        );
        assert_eq!(
            ConsoleManagerStage::<(), (), ()>::Editor(()).route(),
            ConsoleManagerStageRoute::Editor
        );
        assert_eq!(
            ConsoleManagerStage::<(), (), ()>::Settings(()).route(),
            ConsoleManagerStageRoute::Settings
        );
        assert_eq!(
            ConsoleManagerStage::<(), (), ()>::CreatePrelude(()).route(),
            ConsoleManagerStageRoute::CreatePrelude
        );
        assert_eq!(
            ConsoleManagerStage::<(), (), ()>::ConfirmDelete {
                name: "workspace".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Delete?"),
            }
            .route(),
            ConsoleManagerStageRoute::ConfirmDelete
        );
        assert_eq!(
            ConsoleManagerStage::<(), (), ()>::ConfirmInstancePurge {
                container: "container".to_owned(),
                label: "label".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Purge?"),
            }
            .route(),
            ConsoleManagerStageRoute::ConfirmInstancePurge
        );
    }

    #[derive(Default)]
    struct TestStageState {
        stage: Option<ConsoleManagerStage<(), (), ()>>,
    }

    impl ConsoleManagerStageState<ConsoleManagerStage<(), (), ()>> for TestStageState {
        fn set_manager_stage(&mut self, stage: ConsoleManagerStage<(), (), ()>) {
            self.stage = Some(stage);
        }
    }

    #[test]
    fn apply_manager_stage_updates_storage() {
        let mut state = TestStageState::default();

        apply_manager_stage(&mut state, ConsoleManagerStage::List);

        assert_eq!(
            state.stage.as_ref().map(ConsoleManagerStage::route),
            Some(ConsoleManagerStageRoute::List)
        );
    }

    #[test]
    fn console_manager_stage_reports_modal_facts() {
        type Stage = ConsoleManagerStage<ConsoleCreatePreludeState<()>, TestEditor, TestSettings>;

        assert_eq!(Stage::List.modal_facts(), ConsoleStageModalFacts::default());
        assert_eq!(
            Stage::Editor(TestEditor {
                modal_open: true,
                footer_height: 4,
            })
            .modal_facts(),
            ConsoleStageModalFacts {
                editor_modal_open: true,
                ..ConsoleStageModalFacts::default()
            }
        );

        let settings_facts = ConsoleStageModalFacts {
            settings_error_popup_open: true,
            settings_auth_modal_open: true,
            ..ConsoleStageModalFacts::default()
        };
        assert_eq!(
            Stage::Settings(TestSettings {
                facts: settings_facts,
                footer_height: 6,
            })
            .modal_facts(),
            settings_facts
        );

        assert_eq!(
            Stage::CreatePrelude(ConsoleCreatePreludeState {
                step: CreateStep::PickFirstMountSrc,
                pending_mount_src: None,
                pending_mount_dst: None,
                pending_readonly: false,
                pending_workdir: None,
                pending_name: None,
                modal: Some(()),
                last_browser_cwd: None,
                used_edit_dst: false,
            })
            .modal_facts(),
            ConsoleStageModalFacts {
                create_prelude_modal_open: true,
                ..ConsoleStageModalFacts::default()
            }
        );

        assert_eq!(
            Stage::ConfirmDelete {
                name: "workspace".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Delete?"),
            }
            .modal_facts(),
            ConsoleStageModalFacts {
                destructive_confirm_open: true,
                ..ConsoleStageModalFacts::default()
            }
        );
    }

    #[test]
    fn console_manager_stage_reports_footer_height_facts() {
        type Stage = ConsoleManagerStage<(), TestEditor, TestSettings>;

        assert_eq!(
            Stage::Editor(TestEditor {
                modal_open: false,
                footer_height: 4,
            })
            .footer_height_facts(2),
            crate::tui::view::StageFooterHeightFacts {
                route: ConsoleManagerStageRoute::Editor,
                workspace_footer_height: 2,
                editor_footer_height: 4,
                settings_footer_height: 0,
            }
        );
        assert_eq!(
            Stage::Settings(TestSettings {
                facts: ConsoleStageModalFacts::default(),
                footer_height: 6,
            })
            .footer_height_facts(2),
            crate::tui::view::StageFooterHeightFacts {
                route: ConsoleManagerStageRoute::Settings,
                workspace_footer_height: 2,
                editor_footer_height: 0,
                settings_footer_height: 6,
            }
        );
        assert_eq!(
            Stage::List.footer_height_facts(2),
            crate::tui::view::StageFooterHeightFacts {
                route: ConsoleManagerStageRoute::List,
                workspace_footer_height: 2,
                editor_footer_height: 0,
                settings_footer_height: 0,
            }
        );
    }

    #[test]
    fn console_manager_stage_takes_pending_token_generate_from_editor_or_settings() {
        type Stage = ConsoleManagerStage<(), TestTokenDrain, TestTokenDrain>;

        let mut editor = Stage::Editor(TestTokenDrain { pending: Some(7) });
        assert_eq!(editor.take_pending_token_generate(), Some(7));
        assert_eq!(editor.take_pending_token_generate(), None);

        let mut settings = Stage::Settings(TestTokenDrain { pending: Some(9) });
        assert_eq!(settings.take_pending_token_generate(), Some(9));
        assert_eq!(settings.take_pending_token_generate(), None);

        let mut list = Stage::List;
        assert_eq!(list.take_pending_token_generate(), None);

        let mut create = Stage::CreatePrelude(());
        assert_eq!(create.take_pending_token_generate(), None);

        let mut delete = Stage::ConfirmDelete {
            name: "workspace".to_owned(),
            state: jackin_tui::components::ConfirmState::new("Delete?"),
        };
        assert_eq!(delete.take_pending_token_generate(), None);

        let mut purge = Stage::ConfirmInstancePurge {
            container: "container".to_owned(),
            label: "label".to_owned(),
            state: jackin_tui::components::ConfirmState::new("Purge?"),
        };
        assert_eq!(purge.take_pending_token_generate(), None);
    }

    #[test]
    fn console_manager_stage_polls_pending_role_load_from_editor_only() {
        type Stage = ConsoleManagerStage<(), TestRoleLoad, ()>;

        let mut editor = Stage::Editor(TestRoleLoad { pending: Some(3) });
        let Some((load, result)) = editor.poll_pending_role_load() else {
            panic!("expected pending role load");
        };
        assert_eq!(load, 3);
        assert!(result.is_ok());
        assert!(editor.poll_pending_role_load().is_none());

        assert!(Stage::List.poll_pending_role_load().is_none());
        assert!(Stage::Settings(()).poll_pending_role_load().is_none());
        assert!(Stage::CreatePrelude(()).poll_pending_role_load().is_none());
        assert!(
            Stage::ConfirmDelete {
                name: "workspace".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Delete?"),
            }
            .poll_pending_role_load()
            .is_none()
        );
        assert!(
            Stage::ConfirmInstancePurge {
                container: "container".to_owned(),
                label: "label".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Purge?"),
            }
            .poll_pending_role_load()
            .is_none()
        );
    }

    #[test]
    fn console_manager_stage_polls_pending_drift_check_from_editor_only() {
        type Stage = ConsoleManagerStage<(), TestDriftCheck, ()>;

        let mut editor = Stage::Editor(TestDriftCheck {
            pending: Some((3, "drift")),
        });
        let Some((check, result)) = editor.poll_pending_drift_check() else {
            panic!("expected pending drift check");
        };
        assert_eq!(check, 3);
        assert_eq!(result.ok(), Some("drift"));
        assert!(editor.poll_pending_drift_check().is_none());

        assert!(Stage::List.poll_pending_drift_check().is_none());
        assert!(Stage::Settings(()).poll_pending_drift_check().is_none());
        assert!(
            Stage::CreatePrelude(())
                .poll_pending_drift_check()
                .is_none()
        );
        assert!(
            Stage::ConfirmDelete {
                name: "workspace".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Delete?"),
            }
            .poll_pending_drift_check()
            .is_none()
        );
        assert!(
            Stage::ConfirmInstancePurge {
                container: "container".to_owned(),
                label: "label".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Purge?"),
            }
            .poll_pending_drift_check()
            .is_none()
        );
    }

    #[test]
    fn console_manager_stage_polls_pending_isolation_cleanup_from_editor_only() {
        type Stage = ConsoleManagerStage<(), TestIsolationCleanup, ()>;

        let mut editor = Stage::Editor(TestIsolationCleanup { pending: Some(5) });
        let Some((cleanup, result)) = editor.poll_pending_isolation_cleanup() else {
            panic!("expected pending isolation cleanup");
        };
        assert_eq!(cleanup, 5);
        assert!(result.is_ok());
        assert!(editor.poll_pending_isolation_cleanup().is_none());

        assert!(Stage::List.poll_pending_isolation_cleanup().is_none());
        assert!(
            Stage::Settings(())
                .poll_pending_isolation_cleanup()
                .is_none()
        );
        assert!(
            Stage::CreatePrelude(())
                .poll_pending_isolation_cleanup()
                .is_none()
        );
        assert!(
            Stage::ConfirmDelete {
                name: "workspace".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Delete?"),
            }
            .poll_pending_isolation_cleanup()
            .is_none()
        );
        assert!(
            Stage::ConfirmInstancePurge {
                container: "container".to_owned(),
                label: "label".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Purge?"),
            }
            .poll_pending_isolation_cleanup()
            .is_none()
        );
    }

    #[test]
    fn console_manager_stage_polls_pending_op_commit_with_origin() {
        type Stage = ConsoleManagerStage<(), TestOpCommit, TestOpCommit>;

        let mut editor = Stage::Editor(TestOpCommit {
            pending: Some((3, Ok(()))),
        });
        let Some(resolution) = editor.poll_pending_op_commit() else {
            panic!("expected pending editor op commit");
        };
        assert_eq!(resolution.op_ref, 3);
        assert!(resolution.result.is_ok());
        assert_eq!(
            resolution.origin,
            super::ConsolePendingOpCommitOrigin::Editor
        );
        assert!(editor.poll_pending_op_commit().is_none());

        let mut settings = Stage::Settings(TestOpCommit {
            pending: Some((5, Ok(()))),
        });
        let Some(resolution) = settings.poll_pending_op_commit() else {
            panic!("expected pending settings op commit");
        };
        assert_eq!(resolution.op_ref, 5);
        assert!(resolution.result.is_ok());
        assert_eq!(
            resolution.origin,
            super::ConsolePendingOpCommitOrigin::Settings
        );
        assert!(settings.poll_pending_op_commit().is_none());

        assert!(Stage::List.poll_pending_op_commit().is_none());
        assert!(Stage::CreatePrelude(()).poll_pending_op_commit().is_none());
        assert!(
            Stage::ConfirmDelete {
                name: "workspace".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Delete?"),
            }
            .poll_pending_op_commit()
            .is_none()
        );
        assert!(
            Stage::ConfirmInstancePurge {
                container: "container".to_owned(),
                label: "label".to_owned(),
                state: jackin_tui::components::ConfirmState::new("Purge?"),
            }
            .poll_pending_op_commit()
            .is_none()
        );
    }

    #[test]
    fn console_manager_stage_reports_debug_stage() {
        type Stage = ConsoleManagerStage<
            ConsoleCreatePreludeState<TestDebugModal>,
            TestEditor,
            TestSettings,
        >;

        assert_eq!(Stage::List.debug_stage(), ConsoleStageDebug::List);
        assert_eq!(
            Stage::Editor(TestEditor {
                modal_open: true,
                footer_height: 4,
            })
            .debug_stage(),
            ConsoleStageDebug::Editor {
                mode: "TestMode".to_owned(),
                tab: "TestTab".to_owned(),
                field: "TestField".to_owned(),
                modal: Some(ModalDebugKind::TextInput),
            }
        );
        assert_eq!(
            Stage::CreatePrelude(ConsoleCreatePreludeState {
                step: CreateStep::PickFirstMountSrc,
                pending_mount_src: None,
                pending_mount_dst: None,
                pending_readonly: false,
                pending_workdir: None,
                pending_name: None,
                modal: Some(TestDebugModal),
                last_browser_cwd: None,
                used_edit_dst: false,
            })
            .debug_stage(),
            ConsoleStageDebug::CreatePrelude {
                step: "PickFirstMountSrc".to_owned(),
                modal: Some(ModalDebugKind::ErrorPopup),
            }
        );
        assert_eq!(
            Stage::Settings(TestSettings {
                facts: ConsoleStageModalFacts::default(),
                footer_height: 6,
            })
            .debug_stage(),
            ConsoleStageDebug::Settings {
                tab: "Mounts".to_owned(),
                selected: 2,
                modal: None,
            }
        );
    }

    #[test]
    fn console_input_dispatch_plan_routes_modal_precedence_before_stage() {
        let base = ConsoleInputDispatchFacts {
            list_modal_open: false,
            inline_new_session_picker_open: false,
            inline_provider_picker_open: false,
            launch_provider_picker_open: false,
            inline_agent_picker_open: false,
            inline_role_picker_open: false,
            editor_modal_open: false,
            settings_error_popup_open: false,
            settings_mounts_modal_open: false,
            settings_env_modal_open: false,
            settings_auth_modal_open: false,
            create_prelude_modal_open: false,
            stage_route: ConsoleManagerStageRoute::Settings,
        };

        assert_eq!(
            console_input_dispatch_plan(base),
            ConsoleInputDispatchPlan::Stage(ConsoleManagerStageRoute::Settings)
        );
        assert_eq!(
            console_input_dispatch_plan(ConsoleInputDispatchFacts {
                list_modal_open: true,
                editor_modal_open: true,
                ..base
            }),
            ConsoleInputDispatchPlan::ListModal
        );
        assert_eq!(
            console_input_dispatch_plan(ConsoleInputDispatchFacts {
                inline_new_session_picker_open: true,
                inline_role_picker_open: true,
                ..base
            }),
            ConsoleInputDispatchPlan::InlineNewSessionPicker
        );
        assert_eq!(
            console_input_dispatch_plan(ConsoleInputDispatchFacts {
                inline_provider_picker_open: true,
                launch_provider_picker_open: true,
                ..base
            }),
            ConsoleInputDispatchPlan::InlineProviderPicker
        );
        assert_eq!(
            console_input_dispatch_plan(ConsoleInputDispatchFacts {
                launch_provider_picker_open: true,
                inline_agent_picker_open: true,
                ..base
            }),
            ConsoleInputDispatchPlan::LaunchProviderPicker
        );
        assert_eq!(
            console_input_dispatch_plan(ConsoleInputDispatchFacts {
                inline_agent_picker_open: true,
                inline_role_picker_open: true,
                ..base
            }),
            ConsoleInputDispatchPlan::InlineAgentPicker
        );
        assert_eq!(
            console_input_dispatch_plan(ConsoleInputDispatchFacts {
                inline_role_picker_open: true,
                editor_modal_open: true,
                ..base
            }),
            ConsoleInputDispatchPlan::InlineRolePicker
        );
    }

    #[test]
    fn console_input_dispatch_plan_routes_stage_modal_precedence() {
        let base = ConsoleInputDispatchFacts {
            list_modal_open: false,
            inline_new_session_picker_open: false,
            inline_provider_picker_open: false,
            launch_provider_picker_open: false,
            inline_agent_picker_open: false,
            inline_role_picker_open: false,
            editor_modal_open: false,
            settings_error_popup_open: false,
            settings_mounts_modal_open: false,
            settings_env_modal_open: false,
            settings_auth_modal_open: false,
            create_prelude_modal_open: false,
            stage_route: ConsoleManagerStageRoute::CreatePrelude,
        };

        assert_eq!(
            console_input_dispatch_plan(ConsoleInputDispatchFacts {
                editor_modal_open: true,
                settings_error_popup_open: true,
                ..base
            }),
            ConsoleInputDispatchPlan::EditorModal
        );
        assert_eq!(
            console_input_dispatch_plan(ConsoleInputDispatchFacts {
                settings_error_popup_open: true,
                settings_mounts_modal_open: true,
                ..base
            }),
            ConsoleInputDispatchPlan::SettingsErrorPopup
        );
        assert_eq!(
            console_input_dispatch_plan(ConsoleInputDispatchFacts {
                settings_mounts_modal_open: true,
                settings_env_modal_open: true,
                ..base
            }),
            ConsoleInputDispatchPlan::SettingsMountsModal
        );
        assert_eq!(
            console_input_dispatch_plan(ConsoleInputDispatchFacts {
                settings_env_modal_open: true,
                settings_auth_modal_open: true,
                ..base
            }),
            ConsoleInputDispatchPlan::SettingsEnvModal
        );
        assert_eq!(
            console_input_dispatch_plan(ConsoleInputDispatchFacts {
                settings_auth_modal_open: true,
                create_prelude_modal_open: true,
                ..base
            }),
            ConsoleInputDispatchPlan::SettingsAuthModal
        );
        assert_eq!(
            console_input_dispatch_plan(ConsoleInputDispatchFacts {
                create_prelude_modal_open: true,
                ..base
            }),
            ConsoleInputDispatchPlan::CreatePreludeModal
        );
    }

    #[test]
    fn create_prelude_completion_status_routes_modal_complete_and_cancel() {
        assert_eq!(
            create_prelude_completion_status(true, true),
            CreatePreludeCompletionStatus::InProgress
        );
        assert_eq!(
            create_prelude_completion_status(false, true),
            CreatePreludeCompletionStatus::Complete
        );
        assert_eq!(
            create_prelude_completion_status(false, false),
            CreatePreludeCompletionStatus::Cancelled
        );
    }

    #[test]
    fn create_prelude_key_plan_routes_escape_to_list() {
        assert_eq!(
            create_prelude_key_plan(crossterm::event::KeyCode::Esc),
            CreatePreludeKeyPlan::ReturnToList
        );
        assert_eq!(
            create_prelude_key_plan(crossterm::event::KeyCode::Enter),
            CreatePreludeKeyPlan::Continue
        );
    }

    #[test]
    fn create_prelude_modal_step_routes_modal_facts_by_precedence() {
        assert_eq!(
            create_prelude_modal_step(true, true, true, true, true),
            CreatePreludeModalStep::FileBrowserSrc
        );
        assert_eq!(
            create_prelude_modal_step(false, true, true, true, true),
            CreatePreludeModalStep::MountDstChoice
        );
        assert_eq!(
            create_prelude_modal_step(false, false, true, true, true),
            CreatePreludeModalStep::TextInputDst
        );
        assert_eq!(
            create_prelude_modal_step(false, false, false, true, true),
            CreatePreludeModalStep::WorkdirPick
        );
        assert_eq!(
            create_prelude_modal_step(false, false, false, false, true),
            CreatePreludeModalStep::TextInputName
        );
        assert_eq!(
            create_prelude_modal_step(false, false, false, false, false),
            CreatePreludeModalStep::Other
        );
    }

    #[test]
    fn create_prelude_workdir_cancel_plan_reopens_prior_dst_step() {
        assert_eq!(
            create_prelude_workdir_cancel_plan(true),
            CreatePreludeWorkdirCancelPlan::ReopenTextInputDst
        );
        assert_eq!(
            create_prelude_workdir_cancel_plan(false),
            CreatePreludeWorkdirCancelPlan::ReopenMountDstChoice
        );
    }

    #[test]
    fn create_prelude_file_browser_plan_routes_browser_outcomes() {
        use crate::tui::components::file_browser::FileBrowserOutcome;

        let path = PathBuf::from("/tmp/workspace");
        assert_eq!(
            create_prelude_file_browser_plan::<PathBuf>(FileBrowserOutcome::Cancel),
            CreatePreludeFileBrowserPlan::CancelPrelude
        );
        assert_eq!(
            create_prelude_file_browser_plan::<PathBuf>(FileBrowserOutcome::ResolveGitUrl(
                path.clone()
            )),
            CreatePreludeFileBrowserPlan::ResolveGitUrl(path.clone())
        );
        assert_eq!(
            create_prelude_file_browser_plan::<PathBuf>(FileBrowserOutcome::OpenGitUrl(
                "file:///tmp/workspace".to_owned()
            )),
            CreatePreludeFileBrowserPlan::OpenUrl("file:///tmp/workspace".to_owned())
        );
        assert_eq!(
            create_prelude_file_browser_plan::<PathBuf>(FileBrowserOutcome::Continue),
            CreatePreludeFileBrowserPlan::Continue
        );
        assert_eq!(
            create_prelude_file_browser_plan(FileBrowserOutcome::<PathBuf>::NavigateTo(
                path.clone()
            )),
            CreatePreludeFileBrowserPlan::ApplyFileBrowserOutcome(FileBrowserOutcome::NavigateTo(
                path
            ))
        );
    }

    #[test]
    fn create_prelude_mount_dst_choice_plan_routes_choice_outcomes() {
        use crate::tui::components::mount_dst_choice::MountDstChoice;

        assert_eq!(
            create_prelude_mount_dst_choice_plan(jackin_tui::ModalOutcome::Commit(
                MountDstChoice::SamePath
            )),
            CreatePreludeMountDstChoicePlan::CommitSamePath
        );
        assert_eq!(
            create_prelude_mount_dst_choice_plan(jackin_tui::ModalOutcome::Commit(
                MountDstChoice::Edit
            )),
            CreatePreludeMountDstChoicePlan::OpenEditInput
        );
        assert_eq!(
            create_prelude_mount_dst_choice_plan(jackin_tui::ModalOutcome::Cancel),
            CreatePreludeMountDstChoicePlan::ReopenFileBrowserAtLastCwd
        );
        assert_eq!(
            create_prelude_mount_dst_choice_plan(jackin_tui::ModalOutcome::Continue),
            CreatePreludeMountDstChoicePlan::Continue
        );
    }

    #[test]
    fn create_prelude_text_input_dst_plan_routes_input_outcomes() {
        assert_eq!(
            create_prelude_text_input_dst_plan(jackin_tui::ModalOutcome::Commit(
                "/workspace".to_owned()
            )),
            CreatePreludeTextInputDstPlan::Commit("/workspace".to_owned())
        );
        assert_eq!(
            create_prelude_text_input_dst_plan::<String>(jackin_tui::ModalOutcome::Cancel),
            CreatePreludeTextInputDstPlan::ReopenMountDstChoice
        );
        assert_eq!(
            create_prelude_text_input_dst_plan::<String>(jackin_tui::ModalOutcome::Continue),
            CreatePreludeTextInputDstPlan::Continue
        );
    }

    #[test]
    fn create_prelude_text_input_name_plan_routes_input_outcomes() {
        assert_eq!(
            create_prelude_text_input_name_plan(jackin_tui::ModalOutcome::Commit(
                "workspace".to_owned()
            )),
            CreatePreludeTextInputNamePlan::Commit("workspace".to_owned())
        );
        assert_eq!(
            create_prelude_text_input_name_plan::<String>(jackin_tui::ModalOutcome::Cancel),
            CreatePreludeTextInputNamePlan::ReopenWorkdirPick
        );
        assert_eq!(
            create_prelude_text_input_name_plan::<String>(jackin_tui::ModalOutcome::Continue),
            CreatePreludeTextInputNamePlan::Continue
        );
    }

    #[test]
    fn create_prelude_workdir_pick_plan_routes_input_outcomes() {
        assert_eq!(
            create_prelude_workdir_pick_plan(
                jackin_tui::ModalOutcome::Commit("src".to_owned()),
                true
            ),
            CreatePreludeWorkdirPickPlan::Commit("src".to_owned())
        );
        assert_eq!(
            create_prelude_workdir_pick_plan::<String>(jackin_tui::ModalOutcome::Cancel, true),
            CreatePreludeWorkdirPickPlan::ReopenTextInputDst
        );
        assert_eq!(
            create_prelude_workdir_pick_plan::<String>(jackin_tui::ModalOutcome::Cancel, false),
            CreatePreludeWorkdirPickPlan::ReopenMountDstChoice
        );
        assert_eq!(
            create_prelude_workdir_pick_plan::<String>(jackin_tui::ModalOutcome::Continue, true),
            CreatePreludeWorkdirPickPlan::Continue
        );
    }

    struct TestGithubPicker(usize);

    impl ModalGithubPickerState for TestGithubPicker {
        fn choice_len(&self) -> usize {
            self.0
        }
    }

    struct TestConfirmSave;

    impl ModalConfirmSaveState for TestConfirmSave {
        fn required_height(&self) -> u16 {
            12
        }
    }

    impl ModalConfirmSaveFooterState for TestConfirmSave {
        fn footer_mode(&self) -> ModalFooterMode {
            ModalFooterMode::ConfirmSave {
                scroll_axes: jackin_tui::components::ScrollAxes::none(),
            }
        }
    }

    struct TestError;

    impl ModalErrorPopupState for TestError {
        fn required_height(&self, _inner_width: u16, _max_rows: u16) -> u16 {
            14
        }
    }

    struct TestContainerInfo;

    impl ModalContainerInfoState for TestContainerInfo {
        fn required_height(&self) -> u16 {
            15
        }
    }

    impl ModalContainerInfoFooterState for TestContainerInfo {
        fn content_width(&self) -> usize {
            80
        }

        fn content_height(&self) -> usize {
            40
        }
    }

    struct TestOpPicker(bool);

    impl ModalOpPickerState for TestOpPicker {
        fn has_naming_stage_input(&self) -> bool {
            self.0
        }
    }

    impl ConsoleAnimationTick for TestOpPicker {
        fn tick_active_animation(&mut self) -> bool {
            self.0
        }
    }

    impl ModalOpPickerFooterState for TestOpPicker {
        fn footer_mode(&self, include_refresh: bool) -> ModalFooterMode {
            ModalFooterMode::FilteredPicker {
                include_refresh,
                include_collapse: false,
            }
        }
    }

    struct TestRolePicker(usize);

    impl ModalRolePickerState for TestRolePicker {
        fn filtered_len(&self) -> usize {
            self.0
        }
    }

    struct TestAuthForm;

    impl ModalAuthFormState for TestAuthForm {
        fn required_height(&self) -> u16 {
            13
        }
    }

    impl ModalAuthFormFooterState<()> for TestAuthForm {
        fn footer_mode(&self, _focus: (), can_generate_token: bool) -> ModalFooterMode {
            ModalFooterMode::AuthForm {
                focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
                shows_source_folder: false,
                shows_credential_block: false,
                can_generate_token,
            }
        }
    }

    struct TestFileBrowser;

    impl ModalFileBrowserFooterState for TestFileBrowser {
        fn footer_items(&self) -> Vec<jackin_tui::HintSpan<'static>> {
            vec![jackin_tui::HintSpan::Text("file")]
        }
    }

    type RectTestModal = ConsoleModal<
        (),
        (),
        (),
        TestFileBrowser,
        (),
        (),
        (),
        TestConfirm,
        (),
        TestGithubPicker,
        TestConfirmSave,
        TestError,
        TestContainerInfo,
        (),
        TestOpPicker,
        TestRolePicker,
        (),
        (),
        (),
        TestAuthForm,
        (),
        (),
    >;

    type PreludeStepTestModal = ConsoleModal<
        crate::tui::screens::editor::model::TextInputTarget,
        (),
        crate::tui::screens::editor::model::FileBrowserTarget,
        TestFileBrowser,
        (),
        (),
        (),
        TestConfirm,
        (),
        TestGithubPicker,
        TestConfirmSave,
        TestError,
        TestContainerInfo,
        (),
        TestOpPicker,
        TestRolePicker,
        (),
        (),
        (),
        TestAuthForm,
        (),
        (),
    >;

    #[test]
    fn console_modal_create_prelude_step_maps_create_modal_targets() {
        use crate::tui::screens::editor::model::{FileBrowserTarget, TextInputTarget};

        assert_eq!(
            PreludeStepTestModal::FileBrowser {
                target: FileBrowserTarget::CreateFirstMountSrc,
                state: TestFileBrowser,
            }
            .create_prelude_step(),
            CreatePreludeModalStep::FileBrowserSrc
        );
        assert_eq!(
            PreludeStepTestModal::MountDstChoice {
                target: FileBrowserTarget::CreateFirstMountSrc,
                state: (),
            }
            .create_prelude_step(),
            CreatePreludeModalStep::MountDstChoice
        );
        assert_eq!(
            PreludeStepTestModal::TextInput {
                target: TextInputTarget::MountDst,
                state: (),
            }
            .create_prelude_step(),
            CreatePreludeModalStep::TextInputDst
        );
        assert_eq!(
            PreludeStepTestModal::WorkdirPick { state: () }.create_prelude_step(),
            CreatePreludeModalStep::WorkdirPick
        );
        assert_eq!(
            PreludeStepTestModal::TextInput {
                target: TextInputTarget::Name,
                state: (),
            }
            .create_prelude_step(),
            CreatePreludeModalStep::TextInputName
        );
        assert_eq!(
            PreludeStepTestModal::FileBrowser {
                target: FileBrowserTarget::EditAddMountSrc,
                state: TestFileBrowser,
            }
            .create_prelude_step(),
            CreatePreludeModalStep::Other
        );
    }

    #[test]
    fn console_modal_letter_input_kind_maps_text_filters_and_other_modals() {
        assert_eq!(
            RectTestModal::TextInput {
                target: (),
                state: (),
            }
            .letter_input_kind(),
            Some(crate::tui::run::LetterInputModalKind::TextInput)
        );
        assert_eq!(
            RectTestModal::RolePicker {
                state: TestRolePicker(2),
            }
            .letter_input_kind(),
            Some(crate::tui::run::LetterInputModalKind::FilterPicker)
        );
        assert_eq!(
            RectTestModal::RoleOverridePicker {
                state: TestRolePicker(2),
            }
            .letter_input_kind(),
            Some(crate::tui::run::LetterInputModalKind::FilterPicker)
        );
        assert_eq!(
            RectTestModal::OpPicker {
                state: Box::new(TestOpPicker(false)),
            }
            .letter_input_kind(),
            Some(crate::tui::run::LetterInputModalKind::FilterPicker)
        );
        assert_eq!(
            RectTestModal::ErrorPopup { state: TestError }.letter_input_kind(),
            Some(crate::tui::run::LetterInputModalKind::Other)
        );
    }

    #[test]
    fn console_modal_list_key_target_maps_list_modal_key_handlers() {
        assert_eq!(
            RectTestModal::GithubPicker {
                state: TestGithubPicker(2)
            }
            .list_key_target(),
            crate::tui::update::ListModalKeyTarget::GithubPicker
        );
        assert_eq!(
            RectTestModal::RolePicker {
                state: TestRolePicker(2)
            }
            .list_key_target(),
            crate::tui::update::ListModalKeyTarget::RolePicker
        );
        assert_eq!(
            RectTestModal::ErrorPopup { state: TestError }.list_key_target(),
            crate::tui::update::ListModalKeyTarget::ErrorPopup
        );
        assert_eq!(
            RectTestModal::ContainerInfo {
                state: TestContainerInfo
            }
            .list_key_target(),
            crate::tui::update::ListModalKeyTarget::ContainerInfo
        );
        assert_eq!(
            RectTestModal::StatusPopup { state: () }.list_key_target(),
            crate::tui::update::ListModalKeyTarget::Dismiss
        );
    }

    #[test]
    fn console_modal_list_scroll_target_maps_scrollable_list_modals() {
        assert_eq!(
            RectTestModal::GithubPicker {
                state: TestGithubPicker(2)
            }
            .list_scroll_target(),
            crate::tui::update::ListModalScrollTarget::GithubPicker
        );
        assert_eq!(
            RectTestModal::RolePicker {
                state: TestRolePicker(2)
            }
            .list_scroll_target(),
            crate::tui::update::ListModalScrollTarget::RolePicker
        );
        assert_eq!(
            RectTestModal::OpPicker {
                state: Box::new(TestOpPicker(false))
            }
            .list_scroll_target(),
            crate::tui::update::ListModalScrollTarget::OpPicker
        );
        assert_eq!(
            RectTestModal::ErrorPopup { state: TestError }.list_scroll_target(),
            crate::tui::update::ListModalScrollTarget::None
        );
    }

    #[test]
    fn console_modal_shared_scroll_target_maps_reused_picker_modals() {
        assert_eq!(
            RectTestModal::WorkdirPick { state: () }.shared_scroll_target(),
            crate::tui::update::SharedModalScrollTarget::WorkdirPick
        );
        assert_eq!(
            RectTestModal::RoleOverridePicker {
                state: TestRolePicker(2)
            }
            .shared_scroll_target(),
            crate::tui::update::SharedModalScrollTarget::RolePicker
        );
        assert_eq!(
            RectTestModal::AuthRolePicker {
                state: TestRolePicker(2)
            }
            .shared_scroll_target(),
            crate::tui::update::SharedModalScrollTarget::RolePicker
        );
        assert_eq!(
            RectTestModal::OpPicker {
                state: Box::new(TestOpPicker(false))
            }
            .shared_scroll_target(),
            crate::tui::update::SharedModalScrollTarget::OpPicker
        );
        assert_eq!(
            RectTestModal::ErrorPopup { state: TestError }.shared_scroll_target(),
            crate::tui::update::SharedModalScrollTarget::None
        );
    }

    #[test]
    fn console_modal_ticks_op_picker_animation_only() {
        let mut op_picker = RectTestModal::OpPicker {
            state: Box::new(TestOpPicker(true)),
        };
        assert!(op_picker.tick_active_animation());

        let mut idle_op_picker = RectTestModal::OpPicker {
            state: Box::new(TestOpPicker(false)),
        };
        assert!(!idle_op_picker.tick_active_animation());

        let mut error = RectTestModal::ErrorPopup { state: TestError };
        assert!(!error.tick_active_animation());
    }

    struct TestAnimationTick(bool);

    impl ConsoleAnimationTick for TestAnimationTick {
        fn tick_active_animation(&mut self) -> bool {
            self.0
        }
    }

    #[test]
    fn console_manager_stage_ticks_editor_and_settings_only() {
        type Stage = ConsoleManagerStage<(), TestAnimationTick, TestAnimationTick>;

        let mut editor = Stage::Editor(TestAnimationTick(true));
        assert!(editor.tick_active_animation());

        let mut settings = Stage::Settings(TestAnimationTick(true));
        assert!(settings.tick_active_animation());

        let mut idle_editor = Stage::Editor(TestAnimationTick(false));
        assert!(!idle_editor.tick_active_animation());

        let mut list = Stage::List;
        assert!(!list.tick_active_animation());

        let mut create = Stage::CreatePrelude(());
        assert!(!create.tick_active_animation());

        let mut delete = Stage::ConfirmDelete {
            name: "workspace".to_owned(),
            state: jackin_tui::components::ConfirmState::new("Delete?"),
        };
        assert!(!delete.tick_active_animation());
    }

    #[test]
    fn create_prelude_completed_requires_name_and_mount_fields() {
        let mut prelude = ConsoleCreatePreludeState::<()>::new();
        prelude.accept_mount_src(PathBuf::from("/host/proj"));
        prelude.accept_mount_dst("/work/proj".into(), true);
        prelude.accept_workdir("/work/proj".into());

        assert!(prelude.completed().is_none());

        prelude.accept_name("proj".into());
        let (name, workspace) = prelude.completed().expect("complete prelude");

        assert_eq!(name, "proj");
        assert_eq!(workspace.workdir, "/work/proj");
        assert_eq!(workspace.mounts.len(), 1);
        assert_eq!(workspace.mounts[0].src, "/host/proj");
        assert_eq!(workspace.mounts[0].dst, "/work/proj");
        assert!(workspace.mounts[0].readonly);
        assert_eq!(workspace.mounts[0].isolation, MountIsolation::Shared);
    }

    #[test]
    fn create_prelude_builds_pending_first_mount() {
        let mut prelude = ConsoleCreatePreludeState::<()>::new();
        assert!(prelude.pending_first_mount().is_none());

        prelude.accept_mount_src(PathBuf::from("/host/proj"));
        prelude.accept_mount_dst("/work/proj".into(), true);
        let mount = prelude
            .pending_first_mount()
            .expect("src and dst should build mount");

        assert_eq!(mount.src, "/host/proj");
        assert_eq!(mount.dst, "/work/proj");
        assert!(mount.readonly);
        assert_eq!(mount.isolation, MountIsolation::Shared);
    }

    #[test]
    fn create_prelude_opens_workdir_pick_from_pending_mount() {
        let mut prelude = ConsoleCreatePreludeState::<jackin_config::MountConfig>::new();
        assert!(!prelude.open_workdir_pick_from_pending_mount(|mount| mount));
        assert!(prelude.modal.is_none());

        prelude.accept_mount_src(PathBuf::from("/host/proj"));
        prelude.accept_mount_dst("/work/proj".into(), false);

        assert!(prelude.open_workdir_pick_from_pending_mount(|mount| mount));

        let Some(mount) = prelude.modal else {
            panic!("expected workdir pick modal payload");
        };
        assert_eq!(mount.src, "/host/proj");
        assert_eq!(mount.dst, "/work/proj");
        assert!(!mount.readonly);
    }

    #[test]
    fn create_prelude_reopens_mount_dst_choice_from_source() {
        let mut prelude = ConsoleCreatePreludeState::<String>::new();
        prelude.accept_mount_src(PathBuf::from("/host/proj"));

        prelude.reopen_mount_dst_choice(|src| src);

        assert_eq!(prelude.modal.as_deref(), Some("/host/proj"));
    }

    #[test]
    fn console_modal_reports_debug_kind() {
        type TestModal = ConsoleModal<
            &'static str,
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
        >;

        let modal = TestModal::TextInput {
            target: "name",
            state: (),
        };

        assert_eq!(modal.debug_kind(), ModalDebugKind::TextInput);
    }

    #[test]
    fn console_modal_reports_auth_form_generate_eligibility() {
        type TestModal = ConsoleModal<
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
            crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
            crate::tui::screens::settings::model::AuthFormFocus,
            (),
        >;

        let mut form =
            crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Claude);
        form.set_mode(crate::tui::auth::AuthMode::OAuthToken);
        let modal = TestModal::AuthForm {
            target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Claude,
            },
            state: Box::new(form),
            focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
            literal_buffer: String::new(),
        };

        assert!(modal.auth_form_can_generate_token(true));
        assert!(!modal.auth_form_can_generate_token(false));
    }

    #[test]
    fn console_modal_opens_auth_generate_source_picker() {
        type TestModal = ConsoleModal<
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            &'static str,
            (),
            crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
            crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
            crate::tui::screens::settings::model::AuthFormFocus,
            (),
        >;

        let mut modal = Some(TestModal::AuthForm {
            target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Claude,
            },
            state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
                crate::tui::auth::AuthKind::Claude,
            )),
            focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
            literal_buffer: String::new(),
        });
        let mut parents = Vec::new();

        let target =
            crate::tui::auth_config::ModalAuthTokenGenerateStart::open_auth_generate_source_picker(
                &mut modal,
                &mut parents,
                "source-picker",
            )
            .expect("open auth form should move to source picker");

        assert!(matches!(
            target,
            crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Claude
            }
        ));
        assert_eq!(parents.len(), 1);
        assert!(matches!(modal, Some(TestModal::AuthSourcePicker { .. })));
    }

    #[test]
    fn console_modal_opens_auth_source_picker_from_form() {
        type TestModal = ConsoleModal<
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            &'static str,
            (),
            crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
            crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
            crate::tui::screens::settings::model::AuthFormFocus,
            (),
        >;

        let mut form =
            crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Claude);
        form.set_mode(crate::tui::auth::AuthMode::ApiKey);
        let mut modal = Some(TestModal::AuthForm {
            target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Claude,
            },
            state: Box::new(form),
            focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
            literal_buffer: "existing".into(),
        });
        let mut parents = Vec::new();

        let opened = crate::tui::auth_config::ModalAuthSourcePickerOpen::open_auth_source_picker(
            &mut modal,
            &mut parents,
            |env_var| env_var,
        );

        assert!(opened);
        assert_eq!(parents.len(), 1);
        let expected_env_var = crate::tui::auth::AuthKind::Claude
            .required_env_var(crate::tui::auth::AuthMode::ApiKey)
            .expect("Claude API key mode requires env var");
        assert!(matches!(
            modal,
            Some(TestModal::AuthSourcePicker { state }) if state == expected_env_var
        ));
    }

    #[test]
    fn console_modal_opens_auth_source_folder_browser() {
        type TestModal = ConsoleModal<
            (),
            (),
            &'static str,
            &'static str,
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
            crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
            crate::tui::screens::settings::model::AuthFormFocus,
            (),
        >;

        let mut form =
            crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Claude)
                .with_source_folder(
                    None,
                    Some(
                        crate::tui::components::editor_rows::AuthSourceFolderDisplay {
                            kind:
                                crate::tui::components::editor_rows::AuthSourceFolderKind::Default,
                            path: "~/.claude".into(),
                        },
                    ),
                );
        form.set_mode(crate::tui::auth::AuthMode::Sync);
        let mut modal = Some(TestModal::AuthForm {
            target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Claude,
            },
            state: Box::new(form),
            focus: crate::tui::screens::settings::model::AuthFormFocus::SourceFolder,
            literal_buffer: String::new(),
        });
        let mut parents = Vec::new();

        let opened =
            crate::tui::auth_config::ModalAuthSourceFolderBrowserOpen::open_auth_source_folder_browser(
                &mut modal,
                &mut parents,
                crate::tui::screens::settings::model::AuthFormFocus::SourceFolder,
                "auth-source-folder",
                || Ok::<_, ()>("browser"),
            );

        assert_eq!(
            opened,
            crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::Opened
        );
        assert_eq!(parents.len(), 1);
        assert!(matches!(
            modal,
            Some(TestModal::FileBrowser {
                target: "auth-source-folder",
                state: "browser"
            })
        ));
    }

    #[test]
    fn console_modal_opens_plain_source_text_input() {
        type TestModal = ConsoleModal<
            &'static str,
            String,
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
            crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
            crate::tui::screens::settings::model::AuthFormFocus,
            (),
        >;

        let mut modal = None;
        let mut parents = vec![TestModal::AuthForm {
            target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Claude,
            },
            state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
                crate::tui::auth::AuthKind::Claude,
            )),
            focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
            literal_buffer: "existing".into(),
        }];

        let opened =
            crate::tui::auth_config::ModalAuthPlainSourceOpen::open_auth_plain_source_text_input(
                &mut modal,
                &mut parents,
                crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
                "auth",
                |literal| literal,
            );

        assert!(opened);
        assert_eq!(parents.len(), 1);
        assert!(
            matches!(modal, Some(TestModal::TextInput { target: "auth", state }) if state == "existing")
        );
    }

    #[test]
    fn console_modal_opens_auth_op_picker() {
        type TestModal = ConsoleModal<
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            &'static str,
            (),
            (),
            (),
            crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
            crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
            crate::tui::screens::settings::model::AuthFormFocus,
            (),
        >;

        let mut modal = None;
        let mut parents = vec![TestModal::AuthForm {
            target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Claude,
            },
            state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
                crate::tui::auth::AuthKind::Claude,
            )),
            focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
            literal_buffer: String::new(),
        }];

        let opened = crate::tui::auth_config::ModalAuthOpPickerOpen::open_auth_op_picker(
            &mut modal,
            &mut parents,
            crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
            || "op-picker",
        );

        assert!(opened);
        assert!(matches!(
            parents.last(),
            Some(TestModal::AuthForm {
                focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
                ..
            })
        ));
        assert!(matches!(modal, Some(TestModal::OpPicker { state }) if *state == "op-picker"));
    }

    #[test]
    fn console_modal_applies_auth_plain_text() {
        type TestModal = ConsoleModal<
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
            crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
            crate::tui::screens::settings::model::AuthFormFocus,
            (),
        >;

        let mut modal = None;
        let mut parents = vec![TestModal::AuthForm {
            target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Claude,
            },
            state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
                crate::tui::auth::AuthKind::Claude,
            )),
            focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
            literal_buffer: String::new(),
        }];

        let applied = crate::tui::auth_config::ModalAuthFormCredentialApply::apply_auth_plain_text(
            &mut modal,
            &mut parents,
            crate::tui::screens::settings::model::AuthFormFocus::Save,
            "token",
        );

        assert!(applied);
        assert!(parents.is_empty());
        assert!(matches!(
            modal,
            Some(TestModal::AuthForm {
                state,
                focus: crate::tui::screens::settings::model::AuthFormFocus::Save,
                literal_buffer,
                ..
            }) if state.literal_buffer() == "token" && literal_buffer == "token"
        ));
    }

    #[test]
    fn console_modal_restores_auth_form_modal() {
        type TestModal = ConsoleModal<
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
            crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
            crate::tui::screens::settings::model::AuthFormFocus,
            (),
        >;

        let mut modal = None;
        let mut parents = vec![TestModal::AuthForm {
            target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Claude,
            },
            state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
                crate::tui::auth::AuthKind::Claude,
            )),
            focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
            literal_buffer: "existing".into(),
        }];

        let restored =
            crate::tui::auth_config::ModalAuthFormCredentialApply::restore_auth_form_modal(
                &mut modal,
                &mut parents,
            );

        assert!(restored);
        assert!(parents.is_empty());
        assert!(matches!(
            modal,
            Some(TestModal::AuthForm {
                focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
                literal_buffer,
                ..
            }) if literal_buffer == "existing"
        ));
    }

    #[test]
    fn console_modal_applies_auth_op_ref() {
        type TestModal = ConsoleModal<
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            (),
            crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
            crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
            crate::tui::screens::settings::model::AuthFormFocus,
            (),
        >;

        let mut form =
            crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Claude);
        form.set_mode(crate::tui::auth::AuthMode::ApiKey);
        let op_ref = jackin_core::OpRef {
            op: "op://vault/item/field".into(),
            path: "Vault/Item/Field".into(),
            account: None,
            on_demand: false,
        };
        let mut modal = None;
        let mut parents = vec![TestModal::AuthForm {
            target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Claude,
            },
            state: Box::new(form),
            focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
            literal_buffer: String::new(),
        }];

        let applied = crate::tui::auth_config::ModalAuthFormOpRefApply::apply_auth_op_ref(
            &mut modal,
            &mut parents,
            crate::tui::screens::settings::model::AuthFormFocus::Save,
            op_ref.clone(),
        );

        assert!(applied);
        assert!(parents.is_empty());
        assert!(matches!(
            modal,
            Some(TestModal::AuthForm {
                state,
                focus: crate::tui::screens::settings::model::AuthFormFocus::Save,
                ..
            }) if matches!(
                &state.credential,
                crate::tui::components::auth_panel::CredentialInput::OpRef(value)
                    if *value == op_ref
            )
        ));
    }

    #[test]
    fn console_modal_reports_rect_mode() {
        let modal = RectTestModal::RolePicker {
            state: TestRolePicker(5),
        };

        assert_eq!(
            modal.rect_mode(Rect::new(0, 0, 100, 40)),
            ModalRectMode::RolePicker { filtered_len: 5 }
        );
    }

    #[test]
    fn console_modal_error_rect_mode_uses_required_height() {
        let modal = RectTestModal::ErrorPopup { state: TestError };

        assert_eq!(
            modal.rect_mode(Rect::new(0, 0, 100, 40)),
            ModalRectMode::ErrorPopup {
                required_height: 14
            }
        );
    }

    #[test]
    fn console_modal_container_info_rect_reports_only_container_info_area() {
        let outer = Rect::new(0, 0, 100, 40);
        let modal = RectTestModal::ContainerInfo {
            state: TestContainerInfo,
        };

        assert_eq!(modal.container_info_rect(outer), Some(modal.rect(outer)));
        assert_eq!(
            RectTestModal::ErrorPopup { state: TestError }.container_info_rect(outer),
            None
        );
    }

    #[test]
    fn console_modal_reports_footer_items() {
        let modal = RectTestModal::RolePicker {
            state: TestRolePicker(5),
        };

        assert!(
            modal
                .footer_items(false)
                .contains(&jackin_tui::HintSpan::Text("filter"))
        );
    }

    #[test]
    fn console_modal_footer_items_for_area_reflects_container_info_overflow() {
        let modal = RectTestModal::ContainerInfo {
            state: TestContainerInfo,
        };

        assert!(
            modal
                .footer_items_for_area(false, Rect::new(0, 0, 100, 20))
                .contains(&jackin_tui::HintSpan::Text("scroll"))
        );
    }
}

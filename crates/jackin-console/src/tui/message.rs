//! Top-level console TUI message helpers.
//!
//! Product-specific manager messages still live in the root crate while the
//! workspace console owns root-only config/runtime types. Generic message
//! carriers live here so the top-level TUI vocabulary has a home in the
//! surface crate.

use std::path::PathBuf;

use ratatui::layout::Rect;

#[derive(Debug)]
pub enum ConsoleManagerMessage<
    AuthKind,
    CreatePrelude,
    Editor,
    Settings,
    InstanceRefreshSnapshot,
    MountInfoRefresh,
    OpRef,
    AppConfig,
    WorkspaceConfig,
    EditorTab,
    SettingsTab,
    SecretsScopeTag,
    MountScrollFocus,
    DragState,
    ContainerInfoState,
    GithubPickerState,
> {
    CollapseSelectedTree,
    ClearEditorAuthKind,
    EnterPreview,
    EnterConfirmDelete {
        name: String,
    },
    EnterConfirmInstancePurge {
        container: String,
        label: String,
    },
    EnterCreateEditor {
        name: String,
        workspace: WorkspaceConfig,
    },
    EnterCreatePrelude(CreatePrelude),
    EnterEditor(Editor),
    EnterEditorAuthKind {
        kind: AuthKind,
    },
    EnterSettings(Settings),
    InstancesRefreshed(Result<InstanceRefreshSnapshot, String>),
    MountInfoRefreshed(MountInfoRefresh),
    OpCommitResolved {
        op_ref: OpRef,
        result: anyhow::Result<()>,
        is_settings: bool,
    },
    PollFileBrowserGitUrls,
    PollPickerLoads,
    FocusEditorContent,
    FocusEditorTabBar,
    FocusSettingsContent,
    FocusSettingsTabBar,
    ExitPreview,
    ExpandSelectedTree,
    ClearSettingsAuthKind,
    DismissSettingsErrorPopup,
    OpenSettingsErrorPopup {
        title: String,
        message: String,
    },
    EnterSettingsAuthKind,
    ScrollEditorTabHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    SelectEditorMountRow(usize),
    SelectEditorTab(EditorTab),
    SelectListRow(usize),
    SelectSettingsTab(SettingsTab),
    SelectSettingsTrustRow(usize),
    ScrollEditorWorkspaceMountsHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    ScrollSettingsGlobalMountsHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    ScrollSettingsTrustHorizontal {
        delta: i16,
        term_width: u16,
        content_width: usize,
    },
    MoveSettingsGlobalMountsSelection {
        delta: isize,
        term: Rect,
        footer_h: u16,
    },
    MoveSettingsEnvSelection {
        delta: isize,
        term: Rect,
        footer_h: u16,
    },
    MoveSettingsTrustSelection {
        delta: isize,
        term: Rect,
        footer_h: u16,
    },
    MoveEditorTab {
        delta: isize,
        focus_tab_bar: bool,
    },
    MoveEditorFieldSelection {
        delta: isize,
        max_row: usize,
        skipped_rows: Vec<usize>,
        term: Rect,
        footer_h: u16,
    },
    MoveSettingsTab {
        delta: isize,
        focus_tab_bar: bool,
    },
    MoveSettingsGeneralSelection {
        delta: isize,
    },
    MoveSettingsAuthSelection {
        delta: isize,
    },
    SetSettingsEnvRoleExpanded {
        role: String,
        expanded: bool,
    },
    SetEditorAuthRoleExpanded {
        role: String,
        expanded: bool,
    },
    SetEditorSecretsRoleExpanded {
        role: String,
        expanded: bool,
    },
    ToggleSettingsGlobalMountReadonly,
    ToggleEditorGeneralSelected,
    ToggleEditorMountReadonlySelected,
    ToggleEditorSecretMask {
        scope: SecretsScopeTag,
        key: String,
    },
    ToggleSettingsGeneralSelected,
    ToggleSettingsTrustSelected,
    MoveListSelection(isize),
    MovePreviewPane {
        container: String,
        delta: isize,
    },
    ReloadFromConfig {
        config: Box<AppConfig>,
        cwd: PathBuf,
    },
    ReturnToList,
    ScrollListHorizontal(i16),
    ScrollFocusedListBlockVertical(i16),
    SetListScrollFocus(Option<MountScrollFocus>),
    SetListNamesFocused(bool),
    SetDragState(Option<DragState>),
    SetListSplitPct(u16),
    OpenListErrorPopup {
        title: String,
        message: String,
    },
    OpenStatusPopup {
        title: String,
        message: String,
    },
    DismissStatusPopup,
    OpenListContainerInfo {
        state: ContainerInfoState,
    },
    OpenListGithubPicker {
        state: GithubPickerState,
    },
    DismissListModal,
    DismissInlineSessionPicker,
    DismissInlineRolePicker,
    DismissInlineAgentPicker,
    DismissInlineProviderPicker,
    DismissLaunchProviderPicker,
}

#[derive(Debug)]
pub enum BackgroundEvent<M, RoleLoad, DriftCheck, DriftDetection, IsolationCleanup> {
    Message(M),
    RoleLoadFinished {
        load: RoleLoad,
        result: anyhow::Result<()>,
    },
    DriftCheckFinished {
        check: DriftCheck,
        detection: anyhow::Result<DriftDetection>,
    },
    IsolationCleanupFinished {
        cleanup: IsolationCleanup,
        result: anyhow::Result<()>,
    },
}

#[derive(Debug)]
pub enum ConsoleInputOutcome<RoleSelector, Agent, InstanceAction, Provider> {
    Continue,
    ExitJackin,
    LaunchNamed(String),
    LaunchCurrentDir,
    LaunchWithAgent(RoleSelector),
    LaunchWithRuntimeAgent(Agent),
    InstanceAction {
        container: String,
        action: InstanceAction,
    },
    NewSessionWithProvider {
        container: String,
        agent: Agent,
        provider: Provider,
    },
    LaunchWithProvider {
        selector: RoleSelector,
        agent: Agent,
        provider: Provider,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleInstanceAction<Agent> {
    Reconnect,
    /// Reconnect and ask the in-container daemon to focus this pane
    /// (`session_id`) before forwarding output.
    ReconnectFocus(u64),
    NewSession,
    NewSessionWithAgent(Agent),
    Shell,
    Inspect,
    Stop,
    Purge,
}

impl<Agent> ConsoleInstanceAction<Agent> {
    /// Actions that do not replace the TUI with another foreground process.
    pub fn runs_in_place(self) -> bool {
        matches!(self, Self::Stop | Self::Purge)
    }

    pub fn workspace_action_fact(
        self,
    ) -> crate::tui::screens::workspaces::update::WorkspaceInstanceAction {
        use crate::tui::screens::workspaces::update::WorkspaceInstanceAction;

        match self {
            Self::Reconnect | Self::ReconnectFocus(_) => WorkspaceInstanceAction::Reconnect,
            Self::NewSession | Self::NewSessionWithAgent(_) => WorkspaceInstanceAction::NewSession,
            Self::Shell => WorkspaceInstanceAction::Shell,
            Self::Inspect => WorkspaceInstanceAction::Inspect,
            Self::Stop => WorkspaceInstanceAction::Stop,
            Self::Purge => WorkspaceInstanceAction::Purge,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleOutcome<RoleSelector, Workspace, Agent, Provider> {
    Launch(RoleSelector, Workspace, Option<Agent>),
    InstanceAction {
        container: String,
        action: ConsoleInstanceAction<Agent>,
    },
    /// Operator selected an agent and a provider in the console picker.
    NewSessionWithProvider {
        container: String,
        agent: Agent,
        provider: Provider,
    },
    /// Initial launch with a provider selected before the container exists.
    LaunchWithProvider {
        selector: RoleSelector,
        workspace: Workspace,
        agent: Agent,
        provider: Provider,
    },
}

pub trait InstanceActionHandler<Agent> {
    async fn run_in_place(
        &mut self,
        container: &str,
        action: ConsoleInstanceAction<Agent>,
    ) -> anyhow::Result<()>;
}

#[derive(Debug)]
pub enum ConsolePreludeModalOutcome {
    Continue,
    OpenUrl(String),
    ReopenFileBrowserAtLastCwd,
    ApplyFileBrowserOutcome {
        outcome: crate::tui::components::file_browser::FileBrowserOutcome<PathBuf>,
        browser_cwd: Option<PathBuf>,
    },
    ResolveFileBrowserGitUrl(PathBuf),
}

#[derive(Debug)]
pub enum ConsoleEditorModalOutcome<RoleSelector, RoleSource, OpRef> {
    Continue,
    StartRoleRegistration {
        raw: String,
        key: String,
        selector: RoleSelector,
        source: RoleSource,
    },
    PersistTrustedRoleSource {
        key: String,
        source: RoleSource,
    },
    ApplyFileBrowserOutcome(crate::tui::components::file_browser::FileBrowserOutcome<PathBuf>),
    ResolveFileBrowserGitUrl(PathBuf),
    OpenUrl(String),
    ValidateOpRef(OpRef),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConsoleSettingsModalOutcome {
    Continue,
    SaveSettings,
    OpenGlobalMountFileBrowser,
    OpenUrl(String),
    ApplyFileBrowserOutcome(crate::tui::components::file_browser::FileBrowserOutcome<PathBuf>),
    ResolveFileBrowserGitUrl(PathBuf),
}

#[derive(Debug)]
pub enum ConsoleSettingsAuthOutcome<OpRef> {
    Continue,
    ValidateOpRef(OpRef),
}

#[derive(Debug)]
pub enum AgentPickerResolution {
    Opened,
    NotNeeded,
    Failed(anyhow::Error),
}

#[derive(Debug)]
pub enum AgentPickerChoices<Agent> {
    Choices(Vec<Agent>),
    NotNeeded,
    Failed(anyhow::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptOutcome {
    Launch,
    Defer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnPromptFailure {
    ClearPending,
    RestorePending,
}

#[derive(Debug)]
pub enum LaunchPromptDispatch<Outcome, Request> {
    Launch(Outcome),
    Prompt(Request),
    None,
}

#[derive(Debug)]
pub struct LaunchPromptRequest<Role, Workspace, Input> {
    pub role: Role,
    pub workspace: Workspace,
    pub input: Input,
    pub on_failure: OnPromptFailure,
}

#[derive(Debug)]
pub struct PendingMountInfoRefresh {
    pub target: MountInfoRefreshTarget,
    pub entries: Vec<(String, crate::mount_info::MountKind)>,
}

#[derive(Debug, Clone, Copy)]
pub enum MountInfoRefreshTarget {
    ManagerList,
    Editor,
    SettingsMounts,
}

#[cfg(test)]
mod tests;

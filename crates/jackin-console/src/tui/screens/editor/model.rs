//! Editor screen state: draft workspace config being edited and per-tab/
//! per-field edit state for General, Mounts, Roles, Secrets, and Auth panels.
//!
//! Not responsible for: event handling (see `update`) or rendering (see
//! `view`).

use std::collections::{BTreeMap, BTreeSet};

use jackin_config::WorkspaceConfig;
use jackin_tui::components::FocusOwner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    General,
    Mounts,
    Roles,
    Secrets,
    Auth,
}

impl EditorTab {
    pub const ALL: [Self; 5] = [
        Self::General,
        Self::Mounts,
        Self::Roles,
        Self::Secrets,
        Self::Auth,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Mounts => "Mounts",
            Self::Roles => "Roles",
            Self::Secrets => "Environments",
            Self::Auth => "Auth",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorFocusTarget {
    WorkspaceMounts,
    TabContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorHoverTarget {
    Tab(usize),
    MountRow(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleHeaderExpansionPlan {
    Set { role: String, expanded: bool },
    HeaderNoop,
    NotHeader,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorRoleHeaderExpansionKeyPlan {
    Secrets(RoleHeaderExpansionPlan),
    Auth(RoleHeaderExpansionPlan),
    NotRoleHeaderTab,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthEnterPlan {
    AddRoleOverride,
    ToggleRole(String),
    OpenForm,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEnterKeyPlan {
    OpenGeneralField,
    OpenMountFileBrowser,
    OpenSecretsPicker,
    OpenSecretsEnterModal,
    OpenRoleInput,
    Auth(AuthEnterPlan),
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorEscapeKeyPlan {
    FocusTabBar,
    FocusTabBarAndClearAuthKind,
    ClearAuthKind,
    OpenSaveDiscard,
    ReloadFromConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSaveKeyPlan {
    BeginSave,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorMountGithubOpenPlan {
    NoSelection,
    NoGithubUrl,
    Open(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorHorizontalScrollKeyPlan {
    WorkspaceMounts { delta: i16, content_width: usize },
    TabContent { delta: i16, content_width: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorFieldSelectionKeyPlan {
    pub delta: isize,
    pub max_row: usize,
    pub skipped_rows: Vec<usize>,
    pub term: ratatui::layout::Rect,
    pub footer_h: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorNavigationKeyPlan {
    MoveTab { delta: isize, focus_tab_bar: bool },
    FocusContent,
    FocusTabBar,
    NotNavigation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTopLevelKeyPlan {
    Save,
    Escape,
    Navigation(EditorNavigationKeyPlan),
    ScrollHorizontal { delta: i16 },
    MoveField { delta: isize },
    SetRoleHeaderExpanded { expanded: bool },
    CheckImmediateAction,
    ContinueToTabActions,
}

// Editor top-level key dispatch lives in `input/editor.rs`
// (`dispatch_editor_top_level`), which resolves keys through the
// `EDITOR_GLOBAL` / `EDITOR_TAB_BAR` / `EDITOR_CONTENT` keymaps in
// `tui::keymap`. There is deliberately no parallel `match` encoding here: the
// keymap registry is the single source of truth, and its precedence is covered
// by `input::editor::tests::dispatch_editor_top_level_preserves_precedence`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorImmediateActionKeyPlan {
    EnterAuthKind(crate::tui::auth::AuthKind),
    ToggleGeneralSelected,
    ToggleMountReadonlySelected,
    ToggleSecretMask { scope: SecretsScopeTag, key: String },
    NotImmediateAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorRoleActionKeyPlan {
    OpenRoleInput,
    ToggleAllowed,
    ToggleDefault,
    NotRoleAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMountActionKeyPlan {
    AddMount,
    RemoveSelectedMount,
    CycleIsolation,
    OpenGithub,
    NotMountAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSecretsActionKeyPlan {
    OpenPicker,
    OpenDeleteConfirm,
    OpenAddModal,
    NotSecretsAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAuthActionKeyPlan {
    OpenRolePicker,
    ClearFocusedRow,
    NotAuthAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorTabActionKeyPlan {
    Role(EditorRoleActionKeyPlan),
    Mount(EditorMountActionKeyPlan),
    Secrets(EditorSecretsActionKeyPlan),
    Auth(EditorAuthActionKeyPlan),
    Enter(EditorEnterKeyPlan),
    Noop,
}

#[derive(Debug, Clone)]
pub enum EditorMode {
    Edit { name: String },
    Create,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSaveModePlan {
    Edit { original_name: String },
    Create,
}

#[must_use]
pub fn editor_save_mode_plan(mode: &EditorMode) -> EditorSaveModePlan {
    match mode {
        EditorMode::Edit { name } => EditorSaveModePlan::Edit {
            original_name: name.clone(),
        },
        EditorMode::Create => EditorSaveModePlan::Create,
    }
}

pub trait EditorStatusPopupModal {
    fn is_status_popup(&self) -> bool;
}

pub trait EditorRoleOverridePickerModal {
    fn is_role_override_picker(&self) -> bool;
}

pub trait EditorSaveDiscardModal<SaveDiscardState> {
    fn save_discard_cancel_modal(state: SaveDiscardState) -> Self;
}

pub trait EditorErrorPopupModal<ErrorPopupState> {
    fn error_popup_modal(state: ErrorPopupState) -> Self;
}

#[derive(Debug)]
pub struct EditorState<
    WorkspaceConfig,
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> {
    pub mode: EditorMode,
    pub active_tab: EditorTab,
    /// W3C ARIA Tabs: focus is either on the tab list or exactly one content block.
    pub focus_owner: FocusOwner<EditorFocusTarget>,
    pub hover_target: Option<EditorHoverTarget>,
    pub active_field: FieldFocus,
    pub original: WorkspaceConfig,
    pub pending: WorkspaceConfig,
    pub mount_info_cache: MountInfoCache,
    pub modal: Option<Modal>,
    pub modal_parents: Vec<Modal>,
    /// Create-mode only; Edit mode reads name from `EditorMode::Edit`.
    pub pending_name: Option<String>,
    /// Signals the outer input handler to save and/or pop to List.
    pub exit_after_save: Option<ExitIntent>,
    pub save_flow: SaveFlow,
    /// Secrets tab keys whose value is currently unmasked.
    pub unmasked_rows: BTreeSet<(SecretsScopeTag, String)>,
    pub secrets_expanded: BTreeSet<String>,
    pub auth_expanded: BTreeSet<String>,
    pub auth_selected_kind: Option<crate::tui::auth::AuthKind>,
    pub pending_picker_target: Option<(SecretsScopeTag, Option<String>)>,
    pub pending_picker_value: Option<EnvValue>,
    pub workspace_mounts_scroll_x: u16,
    pub tab_scroll_x: u16,
    pub tab_scroll_y: u16,
    pub tab_content_width: usize,
    pub tab_content_height: usize,
    pub generating_token_target: Option<AuthFormTarget>,
    pub pending_token_generate: Option<PendingTokenGenerate>,
    pub pending_role_load: Option<PendingRoleLoad>,
    pub pending_drift_check: Option<PendingDriftCheck>,
    pub pending_isolation_cleanup: Option<PendingIsolationCleanup>,
    pub pending_op_commit: Option<PendingOpCommit>,
    pub cached_footer_h: u16,
}

impl<
    WorkspaceConfig,
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::model::ConsoleEditorModalPresence
    for EditorState<
        WorkspaceConfig,
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    fn editor_modal_open(&self) -> bool {
        self.modal.is_some()
    }
}

impl<
    WorkspaceConfig,
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::model::ConsoleAnimationTick
    for EditorState<
        WorkspaceConfig,
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
where
    Modal: crate::tui::model::ConsoleAnimationTick,
{
    fn tick_active_animation(&mut self) -> bool {
        self.modal
            .as_mut()
            .is_some_and(crate::tui::model::ConsoleAnimationTick::tick_active_animation)
    }
}

impl<
    WorkspaceConfig,
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    OpRef,
> crate::tui::model::ConsolePendingOpCommit
    for EditorState<
        WorkspaceConfig,
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        crate::tui::subscriptions::PendingOpCommit<OpRef>,
    >
{
    type OpRef = OpRef;

    fn poll_pending_op_commit(&mut self) -> Option<(Self::OpRef, anyhow::Result<()>)> {
        use jackin_tui::runtime::{Subscription, SubscriptionPoll};

        let pending = self.pending_op_commit.as_mut()?;
        let result = match pending.rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => Err(anyhow::anyhow!(
                crate::tui::subscriptions::op_read_worker_disconnected_message()
            )),
        };
        let pending = self.pending_op_commit.take()?;
        Some((pending.op_ref, result))
    }
}

impl<
    WorkspaceConfig,
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    DriftDetection,
    SavePlan,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::model::ConsolePendingDriftCheck
    for EditorState<
        WorkspaceConfig,
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        crate::tui::subscriptions::PendingDriftCheck<DriftDetection, SavePlan>,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    type PendingDriftCheck = crate::tui::subscriptions::PendingDriftCheck<DriftDetection, SavePlan>;
    type DriftDetection = DriftDetection;

    fn poll_pending_drift_check(
        &mut self,
    ) -> Option<(
        Self::PendingDriftCheck,
        anyhow::Result<Self::DriftDetection>,
    )> {
        use jackin_tui::runtime::{Subscription, SubscriptionPoll};

        let check = self.pending_drift_check.as_mut()?;
        let result = match check.rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => Err(anyhow::anyhow!(
                crate::tui::subscriptions::drift_check_worker_disconnected_message()
            )),
        };
        let check = self.pending_drift_check.take()?;
        Some((check, result))
    }
}

impl<
    WorkspaceConfig,
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    SavePlan,
    PendingOpCommit,
> crate::tui::model::ConsolePendingIsolationCleanup
    for EditorState<
        WorkspaceConfig,
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        crate::tui::subscriptions::PendingIsolationCleanup<SavePlan>,
        PendingOpCommit,
    >
{
    type PendingIsolationCleanup = crate::tui::subscriptions::PendingIsolationCleanup<SavePlan>;

    fn poll_pending_isolation_cleanup(
        &mut self,
    ) -> Option<(Self::PendingIsolationCleanup, anyhow::Result<()>)> {
        use jackin_tui::runtime::{Subscription, SubscriptionPoll};

        let cleanup = self.pending_isolation_cleanup.as_mut()?;
        let result = match cleanup.rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => Err(anyhow::anyhow!(
                crate::tui::subscriptions::isolation_cleanup_worker_disconnected_message()
            )),
        };
        let cleanup = self.pending_isolation_cleanup.take()?;
        Some((cleanup, result))
    }
}

impl<
    WorkspaceConfig,
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    RoleSource,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::model::ConsolePendingRoleLoad
    for EditorState<
        WorkspaceConfig,
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        crate::tui::subscriptions::PendingRoleLoad<RoleSource>,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    type PendingRoleLoad = crate::tui::subscriptions::PendingRoleLoad<RoleSource>;

    fn poll_pending_role_load(&mut self) -> Option<(Self::PendingRoleLoad, anyhow::Result<()>)> {
        use jackin_tui::runtime::{Subscription, SubscriptionPoll};

        let load = self.pending_role_load.as_mut()?;
        let result = match load.rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => Err(anyhow::anyhow!(
                crate::tui::subscriptions::role_loader_worker_disconnected_message()
            )),
        };
        let load = self.pending_role_load.take()?;
        Some((load, result))
    }
}

impl<
    WorkspaceConfig,
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::model::ConsolePendingTokenGenerate
    for EditorState<
        WorkspaceConfig,
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    type PendingTokenGenerate = PendingTokenGenerate;

    fn take_pending_token_generate(&mut self) -> Option<Self::PendingTokenGenerate> {
        self.pending_token_generate.take()
    }
}

impl<
    WorkspaceConfig,
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::model::ConsoleEditorFooterHeight
    for EditorState<
        WorkspaceConfig,
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    fn editor_cached_footer_height(&self) -> u16 {
        self.cached_footer_h
    }
}

impl<
    WorkspaceConfig,
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
> crate::tui::debug::ConsoleEditorDebugFacts
    for EditorState<
        WorkspaceConfig,
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
where
    Modal: crate::tui::debug::ConsoleModalDebugKind,
{
    fn editor_stage_debug(&self) -> crate::tui::debug::ConsoleStageDebug {
        crate::tui::debug::ConsoleStageDebug::Editor {
            mode: format!("{:?}", self.mode),
            tab: format!("{:?}", self.active_tab),
            field: format!("{:?}", self.active_field),
            modal: self
                .modal
                .as_ref()
                .map(crate::tui::debug::ConsoleModalDebugKind::modal_debug_kind),
        }
    }
}

impl<
    WorkspaceConfig,
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>
    EditorState<
        WorkspaceConfig,
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    pub fn new_edit(name: String, ws: WorkspaceConfig) -> Self
    where
        WorkspaceConfig: Clone,
        MountInfoCache: Default,
        SaveFlow: Default,
    {
        Self {
            mode: EditorMode::Edit { name },
            active_tab: EditorTab::General,
            focus_owner: FocusOwner::TabBar,
            hover_target: None,
            active_field: FieldFocus::Row(0),
            original: ws.clone(),
            pending: ws,
            mount_info_cache: MountInfoCache::default(),
            modal: None,
            modal_parents: Vec::new(),
            pending_name: None,
            exit_after_save: None,
            save_flow: SaveFlow::default(),
            unmasked_rows: BTreeSet::default(),
            secrets_expanded: BTreeSet::default(),
            auth_expanded: BTreeSet::default(),
            auth_selected_kind: None,
            pending_picker_target: None,
            pending_picker_value: None,
            workspace_mounts_scroll_x: 0,
            tab_scroll_x: 0,
            tab_scroll_y: 0,
            tab_content_width: 0,
            tab_content_height: 0,
            generating_token_target: None,
            pending_token_generate: None,
            pending_role_load: None,
            pending_drift_check: None,
            pending_isolation_cleanup: None,
            pending_op_commit: None,
            cached_footer_h: 1,
        }
    }

    #[must_use]
    pub const fn focus_owner(&self) -> FocusOwner<EditorFocusTarget> {
        self.focus_owner
    }

    pub fn set_focus_owner(&mut self, owner: FocusOwner<EditorFocusTarget>) {
        self.focus_owner = owner;
    }

    pub fn apply_auth_kind_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorAuthKindPlan<crate::tui::auth::AuthKind>,
    ) {
        self.auth_selected_kind = plan.selected_kind;
        self.active_field = FieldFocus::Row(plan.active_row);
        self.tab_scroll_x = plan.tab_scroll_x;
        self.tab_scroll_y = plan.tab_scroll_y;
    }

    pub fn apply_tab_move_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorTabMovePlan,
    ) {
        self.active_tab = plan.active_tab;
        self.set_tab_bar_focused(plan.tab_bar_focused);
        self.active_field = FieldFocus::Row(plan.active_row);
        self.tab_scroll_x = plan.tab_scroll_x;
        self.tab_scroll_y = plan.tab_scroll_y;
        if plan.tab_bar_focused {
            self.set_workspace_mounts_scroll_focused(false);
            self.set_tab_content_scroll_focused(false);
        }
        if plan.clear_auth_kind {
            self.auth_selected_kind = None;
        }
        if plan.clear_secret_view_state {
            self.unmasked_rows.clear();
            self.secrets_expanded.clear();
        }
    }

    pub fn apply_tab_select_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorTabSelectPlan,
    ) {
        self.active_tab = plan.active_tab;
        self.set_tab_bar_focused(plan.tab_bar_focused);
        self.active_field = FieldFocus::Row(plan.active_row);
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
        if plan.clear_auth_kind {
            self.auth_selected_kind = None;
        }
        if plan.clear_secret_view_state {
            self.unmasked_rows.clear();
            self.secrets_expanded.clear();
        }
    }

    pub fn apply_field_selection_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorFieldSelectionPlan,
    ) {
        self.active_field = FieldFocus::Row(plan.active_row);
        self.tab_scroll_y = plan.tab_scroll_y;
    }

    pub fn apply_mount_row_select_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorMountRowSelectPlan,
    ) {
        self.active_field = FieldFocus::Row(plan.active_row);
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
    }

    pub fn select_row(&mut self, row: usize) {
        self.active_field = FieldFocus::Row(row);
    }

    pub fn select_auth_row(&mut self, row: usize) {
        self.select_row(row);
    }

    pub fn apply_tab_horizontal_scroll_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorHorizontalScrollPlan,
    ) {
        self.tab_scroll_x = plan.scroll_x;
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
        self.set_tab_content_scroll_focused(plan.tab_content_scroll_focused);
    }

    pub fn apply_workspace_mounts_horizontal_scroll_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorHorizontalScrollPlan,
    ) {
        self.workspace_mounts_scroll_x = plan.scroll_x;
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
        self.set_tab_content_scroll_focused(plan.tab_content_scroll_focused);
    }

    pub fn apply_scroll_focus_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorScrollFocusPlan,
    ) {
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
        self.set_tab_content_scroll_focused(plan.tab_content_scroll_focused);
    }

    #[must_use]
    pub const fn tab_bar_focused(&self) -> bool {
        self.focus_owner.is_tab_bar()
    }

    #[must_use]
    pub fn navigation_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
    ) -> EditorNavigationKeyPlan {
        use crossterm::event::KeyCode;

        match key_code {
            KeyCode::Left | KeyCode::BackTab if self.tab_bar_focused() => {
                EditorNavigationKeyPlan::MoveTab {
                    delta: -1,
                    focus_tab_bar: true,
                }
            }
            KeyCode::Right if self.tab_bar_focused() => EditorNavigationKeyPlan::MoveTab {
                delta: 1,
                focus_tab_bar: true,
            },
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j' | 'J') if self.tab_bar_focused() => {
                EditorNavigationKeyPlan::FocusContent
            }
            KeyCode::Tab => EditorNavigationKeyPlan::MoveTab {
                delta: 1,
                focus_tab_bar: true,
            },
            KeyCode::BackTab => EditorNavigationKeyPlan::FocusTabBar,
            _ => EditorNavigationKeyPlan::NotNavigation,
        }
    }

    #[must_use]
    pub const fn content_area(&self, term_size: ratatui::layout::Rect) -> ratatui::layout::Rect {
        crate::tui::layout::tabbed_content_area(term_size, self.cached_footer_h)
    }

    pub fn set_tab_bar_focused(&mut self, focused: bool) {
        self.focus_owner = if focused {
            FocusOwner::TabBar
        } else if matches!(self.active_tab, EditorTab::Mounts) {
            FocusOwner::Content(EditorFocusTarget::WorkspaceMounts)
        } else {
            FocusOwner::Content(EditorFocusTarget::TabContent)
        };
    }

    pub fn apply_tab_bar_focus_plan(&mut self, focused: bool) {
        self.set_tab_bar_focused(focused);
    }

    #[must_use]
    pub const fn workspace_mounts_scroll_focused(&self) -> bool {
        matches!(
            self.focus_owner,
            FocusOwner::Content(EditorFocusTarget::WorkspaceMounts)
        )
    }

    pub fn set_workspace_mounts_scroll_focused(&mut self, focused: bool) {
        if focused {
            self.focus_owner = FocusOwner::Content(EditorFocusTarget::WorkspaceMounts);
        } else if self.workspace_mounts_scroll_focused() {
            self.focus_owner = FocusOwner::TabBar;
        }
    }

    #[must_use]
    pub const fn tab_content_scroll_focused(&self) -> bool {
        matches!(
            self.focus_owner,
            FocusOwner::Content(EditorFocusTarget::TabContent)
        )
    }

    pub fn set_tab_content_scroll_focused(&mut self, focused: bool) {
        if focused {
            self.focus_owner = FocusOwner::Content(EditorFocusTarget::TabContent);
        } else if self.tab_content_scroll_focused() {
            self.focus_owner = FocusOwner::TabBar;
        }
    }

    #[must_use]
    pub const fn hovered_tab(&self) -> Option<usize> {
        match self.hover_target {
            Some(EditorHoverTarget::Tab(index)) => Some(index),
            _ => None,
        }
    }

    #[must_use]
    pub const fn hovered_mount_row(&self) -> Option<usize> {
        match self.hover_target {
            Some(EditorHoverTarget::MountRow(index)) => Some(index),
            _ => None,
        }
    }

    pub fn set_hover_target(&mut self, target: Option<EditorHoverTarget>) {
        self.hover_target = target;
    }

    #[must_use]
    pub fn workspace_name_for_panel(&self) -> String {
        crate::tui::screens::editor::view::editor_name_value(
            &self.mode,
            self.pending_name.as_deref(),
            "(new workspace)",
        )
    }

    pub fn new_create() -> Self
    where
        WorkspaceConfig: Clone + Default,
        MountInfoCache: Default,
        SaveFlow: Default,
    {
        let empty = WorkspaceConfig::default();
        Self::new_edit(String::new(), empty).into_create_mode()
    }

    pub fn new_create_with_workspace(name: String, workspace: WorkspaceConfig) -> Self
    where
        WorkspaceConfig: Clone,
        MountInfoCache: Default,
        SaveFlow: Default,
    {
        let mut editor = Self::new_edit(String::new(), workspace).into_create_mode();
        editor.pending_name = Some(name);
        editor
    }

    pub fn commit_workspace_name_input(&mut self, name: impl Into<String>) {
        self.pending_name = Some(name.into());
        self.clear_modal_chain();
    }

    #[must_use]
    fn into_create_mode(mut self) -> Self {
        self.mode = EditorMode::Create;
        self
    }

    pub fn open_sub_modal(&mut self, child: Modal) {
        if let Some(parent) = self.modal.take() {
            self.modal_parents.push(parent);
        }
        self.modal = Some(child);
    }

    pub fn open_save_discard_cancel<SaveDiscardState>(&mut self, state: SaveDiscardState)
    where
        Modal: EditorSaveDiscardModal<SaveDiscardState>,
    {
        self.modal = Some(Modal::save_discard_cancel_modal(state));
    }

    pub fn open_error_popup<ErrorPopupState>(&mut self, state: ErrorPopupState)
    where
        Modal: EditorErrorPopupModal<ErrorPopupState>,
    {
        self.modal = Some(Modal::error_popup_modal(state));
    }

    pub fn pop_modal_chain(&mut self) {
        self.modal = self.modal_parents.pop();
        if self.modal.is_none() {
            self.drop_modal_scratch();
        }
    }

    pub fn clear_modal_chain(&mut self) {
        self.modal = None;
        self.modal_parents.clear();
        self.drop_modal_scratch();
    }

    pub fn dismiss_active_modal(&mut self) {
        self.modal = None;
    }

    #[must_use]
    pub fn has_modal_parent(&self) -> bool {
        !self.modal_parents.is_empty()
    }

    pub fn dismiss_status_popup(&mut self)
    where
        Modal: EditorStatusPopupModal,
    {
        if self
            .modal
            .as_ref()
            .is_some_and(EditorStatusPopupModal::is_status_popup)
        {
            self.modal = None;
        }
    }

    #[must_use]
    pub fn has_active_role_override_picker(&self) -> bool
    where
        Modal: EditorRoleOverridePickerModal,
    {
        self.modal
            .as_ref()
            .is_some_and(EditorRoleOverridePickerModal::is_role_override_picker)
    }

    fn drop_modal_scratch(&mut self) {
        self.pending_picker_value = None;
    }

    #[must_use]
    pub fn auth_form_can_generate_token(&self) -> bool
    where
        Modal: crate::tui::auth_config::ModalAuthFormGenerate,
    {
        let editing_existing_workspace = matches!(self.mode, EditorMode::Edit { .. });
        self.modal
            .as_ref()
            .is_some_and(|modal| modal.auth_form_can_generate_token(editing_existing_workspace))
    }

    #[must_use]
    pub fn active_auth_form_focus(
        &self,
    ) -> Option<crate::tui::screens::settings::model::AuthFormFocus>
    where
        Modal: crate::tui::auth_config::ModalAuthFormFocusInspect<
                crate::tui::screens::settings::model::AuthFormFocus,
            >,
    {
        self.modal
            .as_ref()
            .and_then(crate::tui::auth_config::ModalAuthFormFocusInspect::active_auth_form_focus)
    }

    #[must_use]
    pub fn has_auth_form_parent(&self) -> bool
    where
        Modal: crate::tui::auth_config::ModalAuthFormParentInspect,
    {
        self.modal_parents
            .last()
            .is_some_and(crate::tui::auth_config::ModalAuthFormParentInspect::is_auth_form_parent)
    }

    pub fn start_auth_token_generate<SourcePickerState>(
        &mut self,
        source_picker_state: SourcePickerState,
    ) -> bool
    where
        Modal: crate::tui::auth_config::ModalAuthFormGenerate
            + crate::tui::auth_config::ModalAuthTokenGenerateStart<AuthFormTarget, SourcePickerState>,
        AuthFormTarget: Clone,
    {
        if !self.auth_form_can_generate_token() {
            return false;
        }
        let Some(generate_target) = Modal::open_auth_generate_source_picker(
            &mut self.modal,
            &mut self.modal_parents,
            source_picker_state,
        ) else {
            return false;
        };
        self.generating_token_target = Some(generate_target);
        true
    }
}

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>
    EditorState<
        WorkspaceConfig,
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    pub fn commit_workdir_input(&mut self, workdir: impl Into<String>) {
        self.pending.workdir = workdir.into();
        self.clear_modal_chain();
    }

    pub fn commit_last_mount_dst_input(&mut self, dst: impl Into<String>) {
        if let Some(last) = self.pending.mounts.last_mut() {
            last.dst = dst.into();
        }
        self.clear_modal_chain();
    }

    pub fn apply_confirmed_mounts(
        &mut self,
        final_mounts: Option<Vec<jackin_config::MountConfig>>,
    ) {
        if let Some(final_mounts) = final_mounts {
            self.pending.mounts = final_mounts;
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        if self.pending != self.original {
            return true;
        }
        if let EditorMode::Edit { name } = &self.mode
            && self.pending_name.as_deref().is_some_and(|n| n != name)
        {
            return true;
        }
        false
    }

    #[must_use]
    pub fn change_count(&self) -> usize {
        let mut n = 0;
        if self.pending.workdir != self.original.workdir {
            n += 1;
        }
        if self.pending.default_role != self.original.default_role {
            n += 1;
        }
        if self.pending.allowed_roles != self.original.allowed_roles {
            n += 1;
        }
        if self.pending.keep_awake != self.original.keep_awake {
            n += 1;
        }
        if self.pending.git_pull_on_entry != self.original.git_pull_on_entry {
            n += 1;
        }
        if self.pending.claude != self.original.claude {
            n += 1;
        }
        if self.pending.codex != self.original.codex {
            n += 1;
        }
        if self.pending.github != self.original.github {
            n += 1;
        }
        if let EditorMode::Edit { name } = &self.mode
            && self.pending_name.as_deref().is_some_and(|pn| pn != name)
        {
            n += 1;
        }
        n += crate::mount_diff::classify_mount_diffs(&self.original.mounts, &self.pending.mounts)
            .iter()
            .filter(|d| !matches!(d, crate::mount_diff::MountDiff::Unchanged(_)))
            .count();
        n += crate::tui::screens::settings::update::settings_map_change_count(
            &self.original.env,
            &self.pending.env,
        );

        let role_keys: BTreeSet<&String> = self
            .original
            .roles
            .keys()
            .chain(self.pending.roles.keys())
            .collect();
        for role in role_keys {
            let orig = self.original.roles.get(role);
            let pend = self.pending.roles.get(role);
            let empty = BTreeMap::<String, jackin_config::EnvValue>::new();
            let orig_env = orig.map_or(&empty, |o| &o.env);
            let pend_env = pend.map_or(&empty, |p| &p.env);
            n += crate::tui::screens::settings::update::settings_map_change_count(
                orig_env, pend_env,
            );
            if orig.map(|o| &o.claude) != pend.map(|p| &p.claude) {
                n += 1;
            }
            if orig.map(|o| &o.codex) != pend.map(|p| &p.codex) {
                n += 1;
            }
            if orig.map(|o| &o.github) != pend.map(|p| &p.github) {
                n += 1;
            }
        }
        n
    }

    pub fn cycle_isolation_for_selected_mount(&mut self) {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::cycle_mount_isolation_at(&mut self.pending.mounts, n);
    }

    pub fn remove_selected_mount(&mut self) {
        let FieldFocus::Row(n) = self.active_field;
        if n < self.pending.mounts.len() {
            self.pending.mounts.remove(n);
        }
    }

    pub fn add_shared_mount(&mut self, src: &str, dst: &str) {
        self.pending
            .mounts
            .push(crate::services::workspace::shared_mount_config(
                src, dst, false,
            ));
    }

    pub fn toggle_general_selected(&mut self) {
        let FieldFocus::Row(row) = self.active_field;
        match row {
            2 => {
                self.pending.keep_awake.enabled = !self.pending.keep_awake.enabled;
            }
            3 => {
                self.pending.git_pull_on_entry = !self.pending.git_pull_on_entry;
            }
            _ => {}
        }
    }

    pub fn toggle_selected_mount_readonly(&mut self) {
        let FieldFocus::Row(row) = self.active_field;
        if let Some(mount) = self.pending.mounts.get_mut(row) {
            mount.readonly = !mount.readonly;
        }
    }

    #[must_use]
    #[allow(unfulfilled_lint_expectations)]
    #[expect(
        single_use_lifetimes,
        reason = "impl Iterator over borrowed String keys cannot use anonymous lifetimes on stable Rust"
    )]
    pub fn eligible_role_override_selectors<'a>(
        &self,
        registered_roles: impl Iterator<Item = &'a String>,
    ) -> Vec<jackin_core::RoleSelector> {
        crate::workspace::eligible_role_keys_for_override(registered_roles, &self.pending)
            .into_iter()
            .filter_map(|name| jackin_core::RoleSelector::parse(&name).ok())
            .collect()
    }

    #[must_use]
    #[allow(unfulfilled_lint_expectations)]
    #[expect(
        single_use_lifetimes,
        reason = "impl Iterator over borrowed String keys cannot use anonymous lifetimes on stable Rust"
    )]
    pub fn auth_role_override_selectors<'a>(
        &self,
        registered_roles: impl Iterator<Item = &'a String>,
    ) -> Option<Vec<jackin_core::RoleSelector>> {
        let kind = self.auth_selected_kind?;
        let already_overridden: BTreeSet<String> = self
            .pending
            .roles
            .iter()
            .filter(|(_, role_override)| {
                crate::tui::auth_config::role_override_present(kind, role_override)
            })
            .map(|(name, _)| name.clone())
            .collect();

        let candidates =
            crate::workspace::eligible_role_keys_for_override(registered_roles, &self.pending)
                .into_iter()
                .filter(|role| !already_overridden.contains(role))
                .filter_map(|role| jackin_core::RoleSelector::parse(&role).ok())
                .collect();
        Some(candidates)
    }

    pub fn toggle_allowed_role_at_cursor(&mut self, role_names: &[String]) {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::toggle_allowed_role_at(
            &mut self.pending.allowed_roles,
            &mut self.pending.default_role,
            role_names,
            n,
        );
    }

    pub fn toggle_default_role_at_cursor(&mut self, role_names: &[String]) {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::toggle_default_role_at(
            &self.pending.allowed_roles,
            &mut self.pending.default_role,
            role_names,
            n,
        );
    }

    pub fn toggle_auth_role_expanded(&mut self, role: String) {
        if !self.auth_expanded.remove(&role) {
            self.auth_expanded.insert(role);
        }
    }

    pub fn set_auth_role_expanded(&mut self, role: String, expanded: bool) {
        if expanded {
            self.auth_expanded.insert(role);
        } else {
            self.auth_expanded.remove(&role);
        }
    }

    pub fn set_secrets_role_expanded(&mut self, role: String, expanded: bool) {
        if expanded {
            self.secrets_expanded.insert(role);
        } else {
            self.secrets_expanded.remove(&role);
        }
    }

    pub fn toggle_secret_mask(&mut self, scope: SecretsScopeTag, key: String) {
        let entry = (scope, key);
        if !self.unmasked_rows.remove(&entry) {
            self.unmasked_rows.insert(entry);
        }
    }

    /// Delete an environment key from the draft workspace or role override.
    ///
    /// Claude OAuth-token mode owns its token through the token-setup flow, so
    /// the editor must not silently remove that managed slot.
    pub fn delete_env_var(&mut self, scope: &SecretsScopeTag, key: &str) -> anyhow::Result<()> {
        let protected = key == jackin_core::env_model::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME
            && matches!(scope, SecretsScopeTag::Workspace)
            && self.pending.claude.as_ref().map(|c| c.auth_forward)
                == Some(jackin_config::AuthForwardMode::OAuthToken);
        if protected {
            anyhow::bail!(
                "CLAUDE_CODE_OAUTH_TOKEN is managed by `jackin workspace claude-token` \
                 — use `jackin workspace claude-token revoke <workspace>` to clear it"
            );
        }

        match scope {
            SecretsScopeTag::Workspace => {
                self.pending.env.remove(key);
            }
            SecretsScopeTag::Role(role) => {
                let mut drop_role = false;
                if let Some(override_config) = self.pending.roles.get_mut(role) {
                    override_config.env.remove(key);
                    drop_role = override_config.env.is_empty();
                }
                if drop_role {
                    self.pending.roles.remove(role);
                }
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn focused_auth_role_expansion_plan(
        &self,
        config: &jackin_config::AppConfig,
        expanded: bool,
    ) -> RoleHeaderExpansionPlan {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.auth_flat_rows(config);
        let Some(AuthRow::RoleHeader {
            role,
            expanded: current,
        }) = rows.get(n).cloned()
        else {
            return RoleHeaderExpansionPlan::NotHeader;
        };
        if current == expanded {
            RoleHeaderExpansionPlan::HeaderNoop
        } else {
            RoleHeaderExpansionPlan::Set { role, expanded }
        }
    }

    pub fn clear_auth_row_at_cursor(&mut self, config: &jackin_config::AppConfig) {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.auth_flat_rows(config);
        match rows.get(n).cloned() {
            Some(AuthRow::RoleHeader { role, .. }) => {
                if let Some(kind) = self.auth_selected_kind {
                    self.clear_role_auth_kind(&role, kind);
                }
            }
            Some(AuthRow::RoleMode { role, kind }) => {
                self.clear_role_auth_kind(&role, kind);
            }
            Some(AuthRow::WorkspaceMode { kind }) => {
                crate::tui::auth_config::clear_workspace_auth_layer(&mut self.pending, kind);
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn focused_auth_form(
        &self,
        config: &jackin_config::AppConfig,
    ) -> Option<(
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
    )> {
        let FieldFocus::Row(n) = self.active_field;
        let target = self.resolve_auth_form_target(config, n)?;
        let kind = *target.kind();
        let (existing_mode, existing_credential) = self.auth_form_mode_and_credential(&target);
        let form = existing_mode
            .map_or_else(
                || crate::tui::components::auth_panel::AuthForm::new(kind),
                |mode| {
                    crate::tui::components::auth_panel::AuthForm::from_existing(
                        kind,
                        mode,
                        existing_credential,
                    )
                },
            )
            .with_source_folder(
                self.auth_form_source_folder(&target),
                self.auth_form_source_folder_fallback(config, &target),
            );
        Some((target, form))
    }

    /// Apply a successful auth-form commit to the draft workspace config.
    ///
    /// Writes both the kind block (`auth_forward`) and the credential env var
    /// when the form outcome includes one.
    pub fn persist_auth_form(
        &mut self,
        target: &crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        form: &crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
    ) {
        let Some(outcome) = form.commit() else {
            return;
        };
        match target {
            crate::tui::screens::settings::model::AuthFormTarget::Workspace { kind } => {
                crate::tui::auth_config::apply_workspace_auth_commit(
                    &mut self.pending,
                    *kind,
                    outcome.mode,
                    outcome.env_var_name,
                    outcome.env_value.clone(),
                );
                crate::tui::auth_config::set_workspace_sync_source_dir(
                    &mut self.pending,
                    *kind,
                    outcome.source_folder,
                );
            }
            crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole { role, kind } => {
                let entry = self.pending.roles.entry(role.clone()).or_default();
                crate::tui::auth_config::apply_role_auth_commit(
                    entry,
                    *kind,
                    outcome.mode,
                    outcome.env_var_name,
                    outcome.env_value.clone(),
                );
                crate::tui::auth_config::set_role_sync_source_dir(
                    entry,
                    *kind,
                    outcome.source_folder,
                );
            }
        }
    }

    /// Clear the auth layer and source-folder override for the form target.
    pub fn clear_auth_form_layer(
        &mut self,
        target: &crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
    ) {
        match target {
            crate::tui::screens::settings::model::AuthFormTarget::Workspace { kind } => {
                crate::tui::auth_config::clear_workspace_auth_layer(&mut self.pending, *kind);
                crate::tui::auth_config::set_workspace_sync_source_dir(
                    &mut self.pending,
                    *kind,
                    None,
                );
            }
            crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole { role, kind } => {
                if let Some(entry) = self.pending.roles.get_mut(role) {
                    crate::tui::auth_config::clear_role_auth_layer(entry, *kind);
                    crate::tui::auth_config::set_role_sync_source_dir(entry, *kind, None);
                }
            }
        }
    }

    fn auth_form_mode_and_credential(
        &self,
        target: &crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
    ) -> (
        Option<crate::tui::auth::AuthMode>,
        Option<jackin_core::EnvValue>,
    ) {
        match target {
            crate::tui::screens::settings::model::AuthFormTarget::Workspace { kind } => {
                crate::tui::auth_config::workspace_auth_mode_and_credential(&self.pending, *kind)
            }
            crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole { role, kind } => {
                crate::tui::auth_config::role_auth_mode_and_credential(
                    self.pending.roles.get(role),
                    *kind,
                )
            }
        }
    }

    fn auth_form_source_folder(
        &self,
        target: &crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
    ) -> Option<std::path::PathBuf> {
        let agent = crate::tui::auth_config::auth_kind_agent(*target.kind())?;
        match target {
            crate::tui::screens::settings::model::AuthFormTarget::Workspace { .. } => {
                self.pending.sync_source_dir_for(agent)
            }
            crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole {
                role, ..
            } => self
                .pending
                .roles
                .get(role)
                .and_then(|role| role.sync_source_dir_for(agent)),
        }
    }

    fn auth_form_source_folder_fallback(
        &self,
        config: &jackin_config::AppConfig,
        target: &crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
    ) -> Option<crate::tui::components::editor_rows::AuthSourceFolderDisplay> {
        crate::tui::auth_config::auth_kind_agent(*target.kind())?;
        let synthesized = self.synthesize_app_config_for_auth(config);
        let workspace_name = self.workspace_name_for_panel();
        let role = match target {
            crate::tui::screens::settings::model::AuthFormTarget::Workspace { .. } => "",
            crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole {
                role, ..
            } => role.as_str(),
        };
        Some(crate::tui::auth_config::editor_source_folder_display(
            &synthesized,
            &workspace_name,
            role,
            *target.kind(),
        ))
    }

    fn clear_role_auth_kind(&mut self, role: &str, kind: crate::tui::auth::AuthKind) {
        if let Some(role_override) = self.pending.roles.get_mut(role) {
            crate::tui::auth_config::clear_role_auth_layer(role_override, kind);
        }
    }

    #[must_use]
    pub fn secret_value(
        &self,
        scope: &SecretsScopeTag,
        key: &str,
    ) -> Option<&jackin_core::EnvValue> {
        match scope {
            SecretsScopeTag::Workspace => self.pending.env.get(key),
            SecretsScopeTag::Role(role) => self
                .pending
                .roles
                .get(role)
                .and_then(|role_override| role_override.env.get(key)),
        }
    }

    #[must_use]
    pub fn secret_is_text_editable(&self, scope: &SecretsScopeTag, key: &str) -> bool {
        !self
            .secret_value(scope, key)
            .is_some_and(|value| matches!(value, jackin_core::EnvValue::OpRef(_)))
    }

    #[must_use]
    pub fn focused_secret_is_op_ref(&self) -> bool {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        match rows.get(n) {
            Some(SecretsRow::WorkspaceKeyRow(key)) => self
                .pending
                .env
                .get(key)
                .is_some_and(|value| matches!(value, jackin_core::EnvValue::OpRef(_))),
            Some(SecretsRow::RoleKeyRow { role, key }) => self
                .pending
                .roles
                .get(role)
                .and_then(|role_override| role_override.env.get(key))
                .is_some_and(|value| matches!(value, jackin_core::EnvValue::OpRef(_))),
            _ => false,
        }
    }

    /// No-op on header/sentinel/op:// rows.
    #[must_use]
    pub fn focused_unmask_key(&self) -> Option<(SecretsScopeTag, String)> {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        crate::tui::screens::editor::update::secret_unmask_target_for_row(
            rows.get(n),
            |scope, key| self.secret_is_text_editable(scope, key),
        )
    }

    #[must_use]
    pub fn focused_secret_enter_plan(&self) -> SecretsEnterPlan {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        crate::tui::screens::editor::update::secret_enter_plan_for_row(rows.get(n), |scope, key| {
            self.secret_is_text_editable(scope, key)
        })
    }

    #[must_use]
    pub fn focused_secret_delete_target(&self) -> Option<(SecretsScopeTag, String)> {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        crate::tui::screens::editor::update::secret_delete_target_for_row(rows.get(n))
    }

    #[must_use]
    pub fn focused_secret_add_target(&self) -> Option<SecretsScopeTag> {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        crate::tui::screens::editor::update::secret_add_target_for_row(rows.get(n))
    }

    #[must_use]
    pub fn focused_secrets_role_expansion_plan(&self, expanded: bool) -> RoleHeaderExpansionPlan {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.secrets_flat_rows();
        let Some(SecretsRow::RoleHeader {
            role,
            expanded: current,
        }) = rows.get(n).cloned()
        else {
            return RoleHeaderExpansionPlan::NotHeader;
        };
        if current == expanded {
            RoleHeaderExpansionPlan::HeaderNoop
        } else {
            RoleHeaderExpansionPlan::Set { role, expanded }
        }
    }

    #[must_use]
    pub fn synthesize_app_config_for_auth(
        &self,
        config: &jackin_config::AppConfig,
    ) -> jackin_config::AppConfig {
        crate::tui::auth_config::synthesize_app_config_for_workspace_auth(
            config,
            self.workspace_name_for_panel(),
            self.pending.clone(),
        )
    }

    #[must_use]
    pub fn secrets_flat_rows(&self) -> Vec<SecretsRow> {
        crate::tui::screens::editor::update::secrets_flat_rows(
            &self.pending.env,
            &self.pending.roles,
            &self.secrets_expanded,
            |role| &role.env,
        )
    }

    #[must_use]
    pub fn auth_flat_rows(
        &self,
        config: &jackin_config::AppConfig,
    ) -> Vec<AuthRow<crate::tui::auth::AuthKind>> {
        let synthesized = self.synthesize_app_config_for_auth(config);
        let ws_name = self.workspace_name_for_panel();
        crate::tui::screens::editor::update::auth_flat_rows(
            self.auth_selected_kind,
            crate::tui::auth::AuthKind::WORKSPACE_PANEL_KINDS
                .iter()
                .copied(),
            &self.pending.roles,
            self.pending.allowed_roles.len(),
            &self.auth_expanded,
            &crate::tui::screens::editor::update::AuthFlatRowPredicates {
                role_override_present: &|kind, role| {
                    crate::tui::auth_config::role_override_present(*kind, role)
                },
                effective_mode_needs_credential: &|kind, role| {
                    crate::tui::auth_config::panel_mode_requires_credential(
                        &synthesized,
                        &ws_name,
                        role,
                        *kind,
                    )
                },
                effective_mode_supports_source_folder: &|kind, role| {
                    let mode = crate::tui::auth_config::resolve_panel_mode(
                        &synthesized,
                        *kind,
                        &ws_name,
                        role,
                    );
                    crate::tui::auth::auth_mode_supports_source_folder(*kind, mode)
                },
            },
        )
    }

    #[must_use]
    pub fn focused_auth_kind(
        &self,
        config: &jackin_config::AppConfig,
    ) -> Option<crate::tui::auth::AuthKind> {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.auth_flat_rows(config);
        match rows.get(n) {
            Some(AuthRow::AuthKindRow { kind }) => Some(*kind),
            _ => None,
        }
    }

    #[must_use]
    pub fn focused_auth_enter_plan(&self, config: &jackin_config::AppConfig) -> AuthEnterPlan {
        let FieldFocus::Row(n) = self.active_field;
        let rows = self.auth_flat_rows(config);
        match rows.get(n) {
            Some(AuthRow::AddSentinel { .. }) => AuthEnterPlan::AddRoleOverride,
            Some(AuthRow::RoleHeader { role, .. }) => AuthEnterPlan::ToggleRole(role.clone()),
            Some(AuthRow::WorkspaceMode { .. } | AuthRow::RoleMode { .. }) => {
                AuthEnterPlan::OpenForm
            }
            _ => AuthEnterPlan::Noop,
        }
    }

    #[must_use]
    pub fn enter_key_plan(
        &self,
        config: &jackin_config::AppConfig,
        op_available: bool,
    ) -> EditorEnterKeyPlan {
        match self.active_tab {
            EditorTab::General => EditorEnterKeyPlan::OpenGeneralField,
            EditorTab::Mounts if self.focused_mount_add_row_selected() => {
                EditorEnterKeyPlan::OpenMountFileBrowser
            }
            EditorTab::Mounts => EditorEnterKeyPlan::Noop,
            EditorTab::Secrets if self.focused_secret_is_op_ref() && op_available => {
                EditorEnterKeyPlan::OpenSecretsPicker
            }
            EditorTab::Secrets => EditorEnterKeyPlan::OpenSecretsEnterModal,
            EditorTab::Roles if self.focused_role_add_row_selected(config) => {
                EditorEnterKeyPlan::OpenRoleInput
            }
            EditorTab::Roles => EditorEnterKeyPlan::Noop,
            EditorTab::Auth => EditorEnterKeyPlan::Auth(self.focused_auth_enter_plan(config)),
        }
    }

    #[must_use]
    pub fn escape_key_plan(&self) -> EditorEscapeKeyPlan {
        if !self.tab_bar_focused() {
            return if self.active_tab == EditorTab::Auth && self.auth_selected_kind.is_some() {
                EditorEscapeKeyPlan::FocusTabBarAndClearAuthKind
            } else {
                EditorEscapeKeyPlan::FocusTabBar
            };
        }

        if self.active_tab == EditorTab::Auth && self.auth_selected_kind.is_some() {
            EditorEscapeKeyPlan::ClearAuthKind
        } else if self.is_dirty() {
            EditorEscapeKeyPlan::OpenSaveDiscard
        } else {
            EditorEscapeKeyPlan::ReloadFromConfig
        }
    }

    #[must_use]
    pub fn save_key_plan(&self) -> EditorSaveKeyPlan {
        if self.change_count() == 0 {
            EditorSaveKeyPlan::Noop
        } else {
            EditorSaveKeyPlan::BeginSave
        }
    }

    #[must_use]
    pub fn focused_role_header_expansion_key_plan(
        &self,
        config: &jackin_config::AppConfig,
        expanded: bool,
    ) -> EditorRoleHeaderExpansionKeyPlan {
        match self.active_tab {
            EditorTab::Secrets => EditorRoleHeaderExpansionKeyPlan::Secrets(
                self.focused_secrets_role_expansion_plan(expanded),
            ),
            EditorTab::Auth => EditorRoleHeaderExpansionKeyPlan::Auth(
                self.focused_auth_role_expansion_plan(config, expanded),
            ),
            EditorTab::General | EditorTab::Mounts | EditorTab::Roles => {
                EditorRoleHeaderExpansionKeyPlan::NotRoleHeaderTab
            }
        }
    }

    #[must_use]
    pub fn focused_mount_add_row_selected(&self) -> bool {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::editor_mount_add_row_selected(
            n,
            self.pending.mounts.len(),
        )
    }

    #[must_use]
    pub fn focused_role_add_row_selected(&self, config: &jackin_config::AppConfig) -> bool {
        let FieldFocus::Row(n) = self.active_field;
        crate::tui::screens::editor::update::editor_role_add_row_selected(n, config.roles.len())
    }

    #[must_use]
    pub fn selection_bounds(&self, config: &jackin_config::AppConfig) -> (usize, Vec<usize>) {
        let secrets_rows = self.secrets_flat_rows();
        let auth_rows = self.auth_flat_rows(config);
        crate::tui::screens::editor::update::editor_selection_bounds(
            self.active_tab,
            self.pending.mounts.len(),
            config.roles.len(),
            &secrets_rows,
            &auth_rows,
        )
    }

    #[must_use]
    pub fn field_selection_key_plan(
        &self,
        config: &jackin_config::AppConfig,
        delta: isize,
        term: ratatui::layout::Rect,
    ) -> EditorFieldSelectionKeyPlan {
        let (max_row, skipped_rows) = self.selection_bounds(config);
        EditorFieldSelectionKeyPlan {
            delta,
            max_row,
            skipped_rows,
            term,
            footer_h: self.cached_footer_h,
        }
    }

    #[must_use]
    pub fn immediate_action_key_plan(
        &self,
        config: &jackin_config::AppConfig,
        key_code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) -> EditorImmediateActionKeyPlan {
        use crossterm::event::{KeyCode, KeyModifiers};

        match key_code {
            KeyCode::Enter if self.active_tab == EditorTab::Auth => self
                .focused_auth_kind(config)
                .map_or(EditorImmediateActionKeyPlan::NotImmediateAction, |kind| {
                    EditorImmediateActionKeyPlan::EnterAuthKind(kind)
                }),
            KeyCode::Char(' ') if self.active_tab == EditorTab::General => {
                EditorImmediateActionKeyPlan::ToggleGeneralSelected
            }
            KeyCode::Char('r' | 'R') if self.active_tab == EditorTab::Mounts => {
                EditorImmediateActionKeyPlan::ToggleMountReadonlySelected
            }
            KeyCode::Char('m' | 'M')
                if self.active_tab == EditorTab::Secrets
                    && (modifiers - KeyModifiers::SHIFT).is_empty() =>
            {
                self.focused_unmask_key().map_or(
                    EditorImmediateActionKeyPlan::NotImmediateAction,
                    |(scope, key)| EditorImmediateActionKeyPlan::ToggleSecretMask { scope, key },
                )
            }
            _ => EditorImmediateActionKeyPlan::NotImmediateAction,
        }
    }

    #[must_use]
    pub fn role_action_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
    ) -> EditorRoleActionKeyPlan {
        use crossterm::event::KeyCode;

        if self.active_tab != EditorTab::Roles {
            return EditorRoleActionKeyPlan::NotRoleAction;
        }

        match key_code {
            KeyCode::Char('a' | 'A') => EditorRoleActionKeyPlan::OpenRoleInput,
            KeyCode::Char(' ') => EditorRoleActionKeyPlan::ToggleAllowed,
            KeyCode::Char('*') => EditorRoleActionKeyPlan::ToggleDefault,
            _ => EditorRoleActionKeyPlan::NotRoleAction,
        }
    }

    #[must_use]
    pub fn mount_action_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
    ) -> EditorMountActionKeyPlan {
        use crossterm::event::KeyCode;

        if self.active_tab != EditorTab::Mounts {
            return EditorMountActionKeyPlan::NotMountAction;
        }

        match key_code {
            KeyCode::Char('a' | 'A') => EditorMountActionKeyPlan::AddMount,
            KeyCode::Char('d' | 'D') => EditorMountActionKeyPlan::RemoveSelectedMount,
            KeyCode::Char('i' | 'I') => EditorMountActionKeyPlan::CycleIsolation,
            KeyCode::Char('o' | 'O') => EditorMountActionKeyPlan::OpenGithub,
            _ => EditorMountActionKeyPlan::NotMountAction,
        }
    }

    #[must_use]
    pub fn secrets_action_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
        op_available: bool,
    ) -> EditorSecretsActionKeyPlan {
        use crossterm::event::{KeyCode, KeyModifiers};

        if self.active_tab != EditorTab::Secrets || !(modifiers - KeyModifiers::SHIFT).is_empty() {
            return EditorSecretsActionKeyPlan::NotSecretsAction;
        }

        match key_code {
            KeyCode::Char('p' | 'P') if op_available => EditorSecretsActionKeyPlan::OpenPicker,
            KeyCode::Char('d' | 'D') => EditorSecretsActionKeyPlan::OpenDeleteConfirm,
            KeyCode::Char('a' | 'A') => EditorSecretsActionKeyPlan::OpenAddModal,
            _ => EditorSecretsActionKeyPlan::NotSecretsAction,
        }
    }

    #[must_use]
    pub fn auth_action_key_plan(
        &self,
        key_code: crossterm::event::KeyCode,
    ) -> EditorAuthActionKeyPlan {
        use crossterm::event::KeyCode;

        if self.active_tab != EditorTab::Auth {
            return EditorAuthActionKeyPlan::NotAuthAction;
        }

        match key_code {
            KeyCode::Char('a' | 'A') if self.auth_selected_kind.is_some() => {
                EditorAuthActionKeyPlan::OpenRolePicker
            }
            KeyCode::Char('d' | 'D') => EditorAuthActionKeyPlan::ClearFocusedRow,
            _ => EditorAuthActionKeyPlan::NotAuthAction,
        }
    }

    #[must_use]
    pub fn tab_action_key_plan(
        &self,
        config: &jackin_config::AppConfig,
        key_code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
        op_available: bool,
    ) -> EditorTabActionKeyPlan {
        use crossterm::event::KeyCode;

        let role_action = self.role_action_key_plan(key_code);
        if !matches!(role_action, EditorRoleActionKeyPlan::NotRoleAction) {
            return EditorTabActionKeyPlan::Role(role_action);
        }

        let mount_action = self.mount_action_key_plan(key_code);
        if !matches!(mount_action, EditorMountActionKeyPlan::NotMountAction) {
            return EditorTabActionKeyPlan::Mount(mount_action);
        }

        let secrets_action = self.secrets_action_key_plan(key_code, modifiers, op_available);
        if !matches!(secrets_action, EditorSecretsActionKeyPlan::NotSecretsAction) {
            return EditorTabActionKeyPlan::Secrets(secrets_action);
        }

        let auth_action = self.auth_action_key_plan(key_code);
        if !matches!(auth_action, EditorAuthActionKeyPlan::NotAuthAction) {
            return EditorTabActionKeyPlan::Auth(auth_action);
        }

        if key_code == KeyCode::Enter {
            return EditorTabActionKeyPlan::Enter(self.enter_key_plan(config, op_available));
        }

        EditorTabActionKeyPlan::Noop
    }

    #[must_use]
    pub fn resolve_auth_form_target(
        &self,
        config: &jackin_config::AppConfig,
        row: usize,
    ) -> Option<crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>>
    {
        let rows = self.auth_flat_rows(config);
        crate::tui::screens::editor::update::resolve_auth_form_target(&rows, row)
    }
}

impl<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>
    EditorState<
        WorkspaceConfig,
        crate::mount_info_cache::MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    #[must_use]
    pub fn workspace_mounts_content_width(&self) -> usize {
        crate::tui::mount_display::workspace_config_mounts_content_width_with_cache(
            &self.pending.mounts,
            &self.mount_info_cache,
        )
    }

    #[must_use]
    pub fn horizontal_scroll_key_plan(&self, delta: i16) -> EditorHorizontalScrollKeyPlan {
        if self.active_tab == EditorTab::Mounts {
            return EditorHorizontalScrollKeyPlan::WorkspaceMounts {
                delta,
                content_width: self.workspace_mounts_content_width(),
            };
        }
        EditorHorizontalScrollKeyPlan::TabContent {
            delta,
            content_width: self.tab_content_width,
        }
    }

    #[must_use]
    pub fn focused_mount_github_open_plan(&self) -> EditorMountGithubOpenPlan {
        let FieldFocus::Row(n) = self.active_field;
        let Some(mount) = self.pending.mounts.get(n) else {
            return EditorMountGithubOpenPlan::NoSelection;
        };
        match self.mount_info_cache.github_web_url(&mount.src) {
            Some(web_url) => EditorMountGithubOpenPlan::Open(web_url),
            None => EditorMountGithubOpenPlan::NoGithubUrl,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFocus {
    Row(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecretsScopeTag {
    Workspace,
    Role(String),
}

/// Flat row model for the Secrets tab; cursor is a single index.
#[derive(Debug, Clone)]
pub enum SecretsRow {
    WorkspaceKeyRow(String),
    WorkspaceAddSentinel,
    RoleHeader {
        role: String,
        expanded: bool,
    },
    RoleKeyRow {
        role: String,
        key: String,
    },
    RoleAddSentinel(String),
    /// Non-focusable; cursor Up/Down skips over it.
    SectionSpacer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretsEnterPlan {
    EditValue { scope: SecretsScopeTag, key: String },
    OpenScopePicker,
    ExpandRole(String),
    AddRoleKey { scope: SecretsScopeTag },
    Noop,
}

/// Row-shape model for the Auth tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthRow<K> {
    /// Root picker row: choose which auth kind to manage.
    AuthKindRow { kind: K },
    /// Selected auth kind's workspace-level mode row.
    WorkspaceMode { kind: K },
    /// Selected auth kind's workspace credential source row.
    WorkspaceSource { kind: K },
    /// Selected auth kind's workspace sync source-folder row.
    WorkspaceSourceFolder { kind: K },
    /// Collapsible role override block.
    RoleHeader { role: String, expanded: bool },
    /// Mode row inside an expanded `RoleHeader`.
    RoleMode { role: String, kind: K },
    /// Credential source row inside an expanded `RoleHeader`.
    RoleSource { role: String, kind: K },
    /// Sync source-folder row inside an expanded `RoleHeader`.
    RoleSourceFolder { role: String, kind: K },
    /// `+ Override for a role` sentinel.
    AddSentinel { eligible: usize },
    /// Visual spacer.
    Spacer,
}

#[derive(Debug, Clone)]
pub struct PendingSaveCommit<M> {
    pub effective_removals: Vec<String>,
    pub final_mounts: Option<Vec<M>>,
    /// True when the operator has already confirmed isolated-state cleanup
    /// for source drift in this save cycle.
    pub delete_isolated_acknowledged: bool,
    /// True after the acknowledged cleanup worker has completed; the final
    /// write pass can then skip drift re-check and cleanup.
    pub isolated_cleanup_complete: bool,
}

#[derive(Debug, Clone, Default)]
pub enum EditorSaveFlow<P> {
    #[default]
    Idle,
    Confirming {
        exit_on_success: bool,
    },
    PendingCommit {
        plan: P,
        exit_on_success: bool,
    },
    Error {
        message: String,
    },
}

impl<P> EditorSaveFlow<P> {
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error { .. })
    }

    #[must_use]
    pub const fn error_message(&self) -> Option<&str> {
        if let Self::Error { message } = self {
            Some(message.as_str())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConfirmTarget<R, P> {
    DeleteEnvVar {
        scope: SecretsScopeTag,
        key: String,
    },
    TrustRoleSource {
        key: String,
        source: R,
    },
    DeleteIsolatedAndSave {
        plan: P,
        exit_on_success: bool,
        affected_containers: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputTarget {
    Name,
    Workdir,
    MountDst,
    Role,
    EnvKey { scope: SecretsScopeTag },
    EnvValue { scope: SecretsScopeTag, key: String },
    AuthCredential,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBrowserTarget {
    CreateFirstMountSrc,
    EditAddMountSrc,
    AuthFormSourceFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitIntent {
    Save,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateStep {
    PickFirstMountSrc,
    PickFirstMountDst,
    PickWorkdir,
    NameWorkspace,
}

#[cfg(test)]
mod tests {
    use jackin_config::{
        MountConfig, MountIsolation, RoleSource, WorkspaceConfig, WorkspaceRoleOverride,
    };

    use super::{
        AuthEnterPlan, AuthRow, EditorAuthActionKeyPlan, EditorEnterKeyPlan, EditorEscapeKeyPlan,
        EditorFieldSelectionKeyPlan, EditorHorizontalScrollKeyPlan, EditorImmediateActionKeyPlan,
        EditorMode, EditorMountActionKeyPlan, EditorMountGithubOpenPlan, EditorNavigationKeyPlan,
        EditorRoleActionKeyPlan, EditorRoleHeaderExpansionKeyPlan, EditorSaveKeyPlan,
        EditorSaveModePlan, EditorSecretsActionKeyPlan, EditorState, EditorStatusPopupModal,
        EditorTab, EditorTabActionKeyPlan, FieldFocus, RoleHeaderExpansionPlan, SecretsRow,
        editor_save_mode_plan,
    };

    type TestEditor =
        EditorState<WorkspaceConfig, (), (), (), jackin_config::EnvValue, (), (), (), (), (), ()>;
    #[derive(Debug)]
    enum TestStatusModal {
        Status,
        Other,
    }

    impl EditorStatusPopupModal for TestStatusModal {
        fn is_status_popup(&self) -> bool {
            matches!(self, Self::Status)
        }
    }

    impl super::EditorRoleOverridePickerModal for TestStatusModal {
        fn is_role_override_picker(&self) -> bool {
            matches!(self, Self::Other)
        }
    }

    impl super::EditorSaveDiscardModal<u8> for TestStatusModal {
        fn save_discard_cancel_modal(state: u8) -> Self {
            if state == 0 {
                Self::Status
            } else {
                Self::Other
            }
        }
    }

    impl super::EditorErrorPopupModal<u8> for TestStatusModal {
        fn error_popup_modal(state: u8) -> Self {
            if state == 0 {
                Self::Status
            } else {
                Self::Other
            }
        }
    }

    type TestEditorWithStatusModal = EditorState<
        WorkspaceConfig,
        (),
        TestStatusModal,
        (),
        jackin_config::EnvValue,
        (),
        (),
        (),
        (),
        (),
        (),
    >;
    #[derive(Debug)]
    enum TestAuthModal {
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

    type TestEditorWithAuthModal = EditorState<
        WorkspaceConfig,
        (),
        TestAuthModal,
        (),
        jackin_config::EnvValue,
        u8,
        (),
        (),
        (),
        (),
        (),
    >;
    type TestEditorWithMountCache = EditorState<
        WorkspaceConfig,
        crate::mount_info_cache::MountInfoCache,
        (),
        (),
        jackin_config::EnvValue,
        (),
        (),
        (),
        (),
        (),
        (),
    >;

    #[test]
    fn editor_apply_auth_kind_plan_updates_selection_and_scroll() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.active_field = FieldFocus::Row(9);
        editor.tab_scroll_x = 12;
        editor.tab_scroll_y = 4;

        editor.apply_auth_kind_plan(
            crate::tui::screens::editor::update::enter_editor_auth_kind_plan(
                crate::tui::auth::AuthKind::Claude,
            ),
        );

        assert_eq!(
            editor.auth_selected_kind,
            Some(crate::tui::auth::AuthKind::Claude)
        );
        assert_eq!(editor.active_field, FieldFocus::Row(0));
        assert_eq!(editor.tab_scroll_x, 0);
        assert_eq!(editor.tab_scroll_y, 0);

        editor.apply_auth_kind_plan(
            crate::tui::screens::editor::update::clear_editor_auth_kind_plan(),
        );
        assert_eq!(editor.auth_selected_kind, None);
    }

    #[test]
    fn editor_apply_tab_move_plan_resets_departed_tab_state() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.active_tab = EditorTab::Secrets;
        editor
            .unmasked_rows
            .insert((super::SecretsScopeTag::Workspace, "API_KEY".to_owned()));
        editor.secrets_expanded.insert("builder".to_owned());
        editor.set_tab_content_scroll_focused(true);

        editor.apply_tab_move_plan(crate::tui::screens::editor::update::editor_tab_move_plan(
            EditorTab::Secrets,
            1,
            true,
        ));

        assert_eq!(editor.active_tab, EditorTab::Auth);
        assert!(editor.tab_bar_focused());
        assert_eq!(editor.active_field, FieldFocus::Row(0));
        assert!(editor.unmasked_rows.is_empty());
        assert!(editor.secrets_expanded.is_empty());
    }

    #[test]
    fn editor_apply_selection_and_scroll_plans_update_focus() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);

        editor.apply_tab_select_plan(crate::tui::screens::editor::update::editor_tab_select_plan(
            EditorTab::Auth,
            EditorTab::Mounts,
        ));
        assert_eq!(editor.active_tab, EditorTab::Mounts);
        assert_eq!(editor.auth_selected_kind, None);
        assert_eq!(editor.active_field, FieldFocus::Row(0));

        editor.apply_tab_bar_focus_plan(false);
        assert!(!editor.tab_bar_focused());

        editor.apply_mount_row_select_plan(
            crate::tui::screens::editor::update::editor_mount_row_select_plan(3),
        );
        assert_eq!(editor.active_field, FieldFocus::Row(3));
        assert!(editor.workspace_mounts_scroll_focused());

        editor.select_row(5);
        assert_eq!(editor.active_field, FieldFocus::Row(5));

        editor.set_hover_target(Some(super::EditorHoverTarget::MountRow(2)));
        assert_eq!(editor.hovered_mount_row(), Some(2));

        editor.apply_tab_horizontal_scroll_plan(
            crate::tui::screens::editor::update::editor_tab_horizontal_scroll_plan(0, 8, 20, 80),
        );
        assert_eq!(editor.tab_scroll_x, 8);
        assert!(editor.tab_content_scroll_focused());
    }

    #[test]
    fn editor_apply_scroll_focus_plan_updates_focus_owner() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.apply_scroll_focus_plan(
            crate::tui::screens::editor::update::EditorScrollFocusPlan {
                workspace_mounts_scroll_focused: true,
                tab_content_scroll_focused: false,
            },
        );
        assert!(editor.workspace_mounts_scroll_focused());

        editor.apply_scroll_focus_plan(
            crate::tui::screens::editor::update::EditorScrollFocusPlan {
                workspace_mounts_scroll_focused: false,
                tab_content_scroll_focused: true,
            },
        );
        assert!(editor.tab_content_scroll_focused());
    }

    #[test]
    fn editor_toggles_general_config_at_cursor() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.active_field = FieldFocus::Row(2);
        editor.toggle_general_selected();
        editor.active_field = FieldFocus::Row(3);
        editor.toggle_general_selected();

        assert!(editor.pending.keep_awake.enabled);
        assert!(editor.pending.git_pull_on_entry);
    }

    #[test]
    fn editor_toggles_selected_mount_readonly() {
        let mut workspace = WorkspaceConfig::default();
        workspace.mounts.push(MountConfig {
            src: "/src".into(),
            dst: "/dst".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.active_field = FieldFocus::Row(0);
        editor.toggle_selected_mount_readonly();

        assert!(editor.pending.mounts[0].readonly);
    }

    #[test]
    fn editor_sets_role_expansion_state() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.set_auth_role_expanded(String::from("dev"), true);
        editor.set_secrets_role_expanded(String::from("ops"), true);
        assert!(editor.auth_expanded.contains("dev"));
        assert!(editor.secrets_expanded.contains("ops"));

        editor.set_auth_role_expanded(String::from("dev"), false);
        editor.set_secrets_role_expanded(String::from("ops"), false);
        assert!(!editor.auth_expanded.contains("dev"));
        assert!(!editor.secrets_expanded.contains("ops"));
    }

    #[test]
    fn editor_toggles_secret_mask_state() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.toggle_secret_mask(super::SecretsScopeTag::Workspace, String::from("API_KEY"));
        assert!(
            editor
                .unmasked_rows
                .contains(&(super::SecretsScopeTag::Workspace, String::from("API_KEY")))
        );

        editor.toggle_secret_mask(super::SecretsScopeTag::Workspace, String::from("API_KEY"));
        assert!(editor.unmasked_rows.is_empty());
    }

    #[test]
    fn editor_dirty_tracks_pending_config_and_rename() {
        let workspace = WorkspaceConfig {
            workdir: "/work".into(),
            ..Default::default()
        };
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        assert!(!editor.is_dirty());
        editor.pending_name = Some("beta".into());
        assert!(editor.is_dirty());
    }

    #[test]
    fn editor_workspace_name_for_panel_uses_create_fallback_or_pending_name() {
        let mut editor = TestEditor::new_create();

        assert_eq!(editor.workspace_name_for_panel(), "(new workspace)");

        editor.pending_name = Some("draft".into());
        assert_eq!(editor.workspace_name_for_panel(), "draft");
    }

    #[test]
    fn new_create_with_workspace_sets_pending_name_and_config() {
        let workspace = WorkspaceConfig {
            workdir: "/repo".into(),
            ..Default::default()
        };

        let editor = TestEditor::new_create_with_workspace("draft".into(), workspace);

        assert!(matches!(editor.mode, EditorMode::Create));
        assert_eq!(editor.pending_name.as_deref(), Some("draft"));
        assert_eq!(editor.pending.workdir, "/repo");
    }

    #[test]
    fn commit_workspace_name_input_updates_pending_name() {
        let mut editor = TestEditor::new_create();

        editor.commit_workspace_name_input("renamed");

        assert_eq!(editor.pending_name.as_deref(), Some("renamed"));
    }

    #[test]
    fn dismiss_active_modal_preserves_modal_stack_and_scratch() {
        let mut editor =
            TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.modal = Some(TestStatusModal::Status);
        editor.modal_parents.push(TestStatusModal::Other);
        editor.pending_picker_value = Some(jackin_config::EnvValue::Plain("secret".into()));

        editor.dismiss_active_modal();

        assert!(editor.modal.is_none());
        assert_eq!(editor.modal_parents.len(), 1);
        assert!(matches!(editor.modal_parents[0], TestStatusModal::Other));
        assert!(matches!(
            editor.pending_picker_value,
            Some(jackin_config::EnvValue::Plain(_))
        ));
    }

    #[test]
    fn has_modal_parent_tracks_modal_stack_presence() {
        let mut editor =
            TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());

        assert!(!editor.has_modal_parent());

        editor.modal_parents.push(TestStatusModal::Other);

        assert!(editor.has_modal_parent());
    }

    #[test]
    fn open_save_discard_cancel_sets_modal() {
        let mut editor =
            TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.open_save_discard_cancel(1);

        assert!(matches!(editor.modal, Some(TestStatusModal::Other)));
    }

    #[test]
    fn open_error_popup_sets_modal() {
        let mut editor =
            TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.open_error_popup(1);

        assert!(matches!(editor.modal, Some(TestStatusModal::Other)));
    }

    #[test]
    fn dismiss_status_popup_only_closes_status_modal() {
        let mut editor =
            TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.modal = Some(TestStatusModal::Status);

        editor.dismiss_status_popup();

        assert!(editor.modal.is_none());

        editor.modal = Some(TestStatusModal::Other);

        editor.dismiss_status_popup();

        assert!(matches!(editor.modal, Some(TestStatusModal::Other)));
    }

    #[test]
    fn has_active_role_override_picker_checks_current_modal() {
        let mut editor =
            TestEditorWithStatusModal::new_edit("alpha".into(), WorkspaceConfig::default());

        assert!(!editor.has_active_role_override_picker());

        editor.modal = Some(TestStatusModal::Status);
        assert!(!editor.has_active_role_override_picker());

        editor.modal = Some(TestStatusModal::Other);
        assert!(editor.has_active_role_override_picker());
    }

    #[test]
    fn active_auth_form_focus_reads_only_auth_modal() {
        let mut editor =
            TestEditorWithAuthModal::new_edit("alpha".into(), WorkspaceConfig::default());

        assert_eq!(editor.active_auth_form_focus(), None);

        editor.modal = Some(TestAuthModal::Other);
        assert_eq!(editor.active_auth_form_focus(), None);

        editor.modal = Some(TestAuthModal::Auth {
            focus: crate::tui::screens::settings::model::AuthFormFocus::Save,
        });
        assert_eq!(
            editor.active_auth_form_focus(),
            Some(crate::tui::screens::settings::model::AuthFormFocus::Save)
        );
    }

    #[test]
    fn has_auth_form_parent_checks_top_parent_only() {
        let mut editor =
            TestEditorWithAuthModal::new_edit("alpha".into(), WorkspaceConfig::default());

        assert!(!editor.has_auth_form_parent());

        editor.modal_parents.push(TestAuthModal::Auth {
            focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
        });
        assert!(editor.has_auth_form_parent());

        editor.modal_parents.push(TestAuthModal::Other);
        assert!(!editor.has_auth_form_parent());
    }

    #[test]
    fn commit_workdir_input_updates_pending_workdir() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.commit_workdir_input("/repo");

        assert_eq!(editor.pending.workdir, "/repo");
    }

    #[test]
    fn commit_last_mount_dst_input_updates_last_mount() {
        let mut workspace = WorkspaceConfig::default();
        workspace.mounts.push(MountConfig {
            src: "/src".into(),
            dst: "/src".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.commit_last_mount_dst_input("/dst");

        assert_eq!(editor.pending.mounts[0].dst, "/dst");
    }

    #[test]
    fn apply_confirmed_mounts_replaces_pending_mounts_when_present() {
        let mut workspace = WorkspaceConfig::default();
        workspace.mounts.push(MountConfig {
            src: "/old".into(),
            dst: "/old".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.apply_confirmed_mounts(Some(vec![MountConfig {
            src: "/new".into(),
            dst: "/new".into(),
            readonly: true,
            isolation: MountIsolation::Shared,
        }]));

        assert_eq!(editor.pending.mounts.len(), 1);
        assert_eq!(editor.pending.mounts[0].src, "/new");
        assert!(editor.pending.mounts[0].readonly);
    }

    #[test]
    fn editor_save_mode_plan_classifies_edit_and_create() {
        assert_eq!(
            editor_save_mode_plan(&EditorMode::Edit {
                name: "alpha".into(),
            }),
            EditorSaveModePlan::Edit {
                original_name: "alpha".into(),
            }
        );

        assert_eq!(
            editor_save_mode_plan(&EditorMode::Create),
            EditorSaveModePlan::Create
        );
    }

    #[test]
    fn editor_synthesizes_pending_workspace_for_auth_rows() {
        let mut editor = TestEditor::new_create();
        editor.pending_name = Some("draft".into());
        editor.pending.env.insert(
            jackin_core::env_model::ZAI_API_KEY_ENV_NAME.into(),
            jackin_config::EnvValue::Plain("zai".into()),
        );
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Zai);

        let synthesized =
            editor.synthesize_app_config_for_auth(&jackin_config::AppConfig::default());
        let rows = editor.auth_flat_rows(&jackin_config::AppConfig::default());

        assert!(synthesized.workspaces.contains_key("draft"));
        assert!(rows.iter().any(|row| matches!(
            row,
            AuthRow::WorkspaceMode {
                kind: crate::tui::auth::AuthKind::Zai
            }
        )));
    }

    #[test]
    fn editor_focused_auth_form_prefills_workspace_layer() {
        let workspace = WorkspaceConfig {
            claude: Some(jackin_config::AgentAuthConfig {
                auth_forward: jackin_config::AuthForwardMode::Sync,
                sync_source_dir: Some(std::path::PathBuf::from("/host/claude")),
            }),
            ..WorkspaceConfig::default()
        };
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);

        let (target, form) = editor
            .focused_auth_form(&jackin_config::AppConfig::default())
            .expect("workspace mode row should open auth form");

        assert!(matches!(
            target,
            crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Claude
            }
        ));
        assert_eq!(form.mode, Some(crate::tui::auth::AuthMode::Sync));
        assert_eq!(
            form.source_folder,
            Some(std::path::PathBuf::from("/host/claude"))
        );
        assert!(form.shows_source_folder());
    }

    #[test]
    fn editor_focused_auth_form_returns_none_for_non_form_rows() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);
        editor.active_field = FieldFocus::Row(usize::MAX);

        assert!(
            editor
                .focused_auth_form(&jackin_config::AppConfig::default())
                .is_none()
        );
    }

    #[test]
    fn editor_persist_auth_form_writes_workspace_layer() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        let mut form =
            crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Zai);
        form.set_mode(crate::tui::auth::AuthMode::ApiKey);
        form.set_literal("zai-key".into());

        editor.persist_auth_form(
            &crate::tui::screens::settings::model::AuthFormTarget::Workspace {
                kind: crate::tui::auth::AuthKind::Zai,
            },
            &form,
        );

        assert_eq!(
            editor
                .pending
                .env
                .get(jackin_core::env_model::ZAI_API_KEY_ENV_NAME),
            Some(&jackin_config::EnvValue::Plain("zai-key".into()))
        );
    }

    #[test]
    fn editor_clear_auth_form_layer_clears_role_source_folder() {
        let mut workspace = WorkspaceConfig::default();
        workspace.roles.entry("dev".into()).or_default().claude =
            Some(jackin_config::AgentAuthConfig {
                auth_forward: jackin_config::AuthForwardMode::Sync,
                sync_source_dir: Some(std::path::PathBuf::from("/role/claude")),
            });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.clear_auth_form_layer(
            &crate::tui::screens::settings::model::AuthFormTarget::WorkspaceRole {
                role: "dev".into(),
                kind: crate::tui::auth::AuthKind::Claude,
            },
        );

        assert_eq!(editor.pending.roles["dev"].claude, None);
    }

    #[test]
    fn editor_toggle_auth_role_expanded_flips_role_section() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.toggle_auth_role_expanded("dev".into());
        assert!(editor.auth_expanded.contains("dev"));

        editor.toggle_auth_role_expanded("dev".into());
        assert!(!editor.auth_expanded.contains("dev"));
    }

    #[test]
    fn editor_focused_auth_role_expansion_plan_reads_current_row() {
        let workspace = WorkspaceConfig {
            roles: std::collections::BTreeMap::from([(
                "dev".into(),
                WorkspaceRoleOverride {
                    github: Some(jackin_config::GithubAuthConfig {
                        auth_forward: jackin_config::GithubAuthMode::Token,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Github);
        let config = jackin_config::AppConfig::default();
        editor.active_field = FieldFocus::Row(
            editor
                .auth_flat_rows(&config)
                .iter()
                .position(|row| matches!(row, AuthRow::RoleHeader { role, .. } if role == "dev"))
                .expect("role header row"),
        );

        assert_eq!(
            editor.focused_auth_role_expansion_plan(&config, true),
            RoleHeaderExpansionPlan::Set {
                role: "dev".into(),
                expanded: true
            }
        );

        editor.auth_expanded.insert("dev".into());
        assert_eq!(
            editor.focused_auth_role_expansion_plan(&config, true),
            RoleHeaderExpansionPlan::HeaderNoop
        );
        assert_eq!(
            editor.focused_auth_role_expansion_plan(&config, false),
            RoleHeaderExpansionPlan::Set {
                role: "dev".into(),
                expanded: false
            }
        );
    }

    #[test]
    fn editor_focused_role_header_expansion_key_plan_routes_by_tab() {
        let mut workspace = WorkspaceConfig::default();
        workspace
            .roles
            .entry("dev".into())
            .or_default()
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
        workspace.roles.entry("dev".into()).or_default().github =
            Some(jackin_config::GithubAuthConfig {
                auth_forward: jackin_config::GithubAuthMode::Token,
                ..Default::default()
            });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        let config = jackin_config::AppConfig::default();

        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(
            editor
                .secrets_flat_rows()
                .iter()
                .position(|row| matches!(row, SecretsRow::RoleHeader { role, .. } if role == "dev"))
                .expect("secrets role header row"),
        );
        assert_eq!(
            editor.focused_role_header_expansion_key_plan(&config, true),
            EditorRoleHeaderExpansionKeyPlan::Secrets(RoleHeaderExpansionPlan::Set {
                role: "dev".into(),
                expanded: true
            })
        );

        editor.active_tab = EditorTab::Auth;
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Github);
        editor.active_field = FieldFocus::Row(
            editor
                .auth_flat_rows(&config)
                .iter()
                .position(|row| matches!(row, AuthRow::RoleHeader { role, .. } if role == "dev"))
                .expect("auth role header row"),
        );
        assert_eq!(
            editor.focused_role_header_expansion_key_plan(&config, true),
            EditorRoleHeaderExpansionKeyPlan::Auth(RoleHeaderExpansionPlan::Set {
                role: "dev".into(),
                expanded: true
            })
        );

        editor.active_tab = EditorTab::Roles;
        assert_eq!(
            editor.focused_role_header_expansion_key_plan(&config, true),
            EditorRoleHeaderExpansionKeyPlan::NotRoleHeaderTab
        );
    }

    #[test]
    fn editor_focused_auth_kind_reads_current_row() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        let config = jackin_config::AppConfig::default();

        assert_eq!(
            editor.focused_auth_kind(&config),
            Some(crate::tui::auth::AuthKind::Claude)
        );

        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);
        assert_eq!(editor.focused_auth_kind(&config), None);
    }

    #[test]
    fn editor_focused_auth_enter_plan_reads_current_row() {
        let mut workspace = WorkspaceConfig::default();
        workspace.roles.entry("dev".into()).or_default().github =
            Some(jackin_config::GithubAuthConfig {
                auth_forward: jackin_config::GithubAuthMode::Token,
                ..Default::default()
            });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        let config = jackin_config::AppConfig::default();

        assert_eq!(editor.focused_auth_enter_plan(&config), AuthEnterPlan::Noop);

        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Github);
        assert_eq!(
            editor.focused_auth_enter_plan(&config),
            AuthEnterPlan::OpenForm
        );

        editor.active_field = FieldFocus::Row(
            editor
                .auth_flat_rows(&config)
                .iter()
                .position(|row| matches!(row, AuthRow::RoleHeader { role, .. } if role == "dev"))
                .expect("role header row"),
        );
        assert_eq!(
            editor.focused_auth_enter_plan(&config),
            AuthEnterPlan::ToggleRole("dev".into())
        );

        editor.active_field = FieldFocus::Row(editor.auth_flat_rows(&config).len() - 1);
        assert_eq!(
            editor.focused_auth_enter_plan(&config),
            AuthEnterPlan::AddRoleOverride
        );
    }

    #[test]
    fn editor_clear_auth_row_at_cursor_clears_workspace_auth_layer() {
        let workspace = WorkspaceConfig {
            env: std::collections::BTreeMap::from([(
                jackin_core::env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
                jackin_config::EnvValue::Plain("zai".into()),
            )]),
            ..WorkspaceConfig::default()
        };
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Zai);

        editor.clear_auth_row_at_cursor(&jackin_config::AppConfig::default());

        assert!(
            !editor
                .pending
                .env
                .contains_key(jackin_core::env_model::ZAI_API_KEY_ENV_NAME)
        );
    }

    #[test]
    fn editor_clear_auth_row_at_cursor_clears_role_auth_layer() {
        let mut workspace = WorkspaceConfig::default();
        workspace.roles.entry("dev".into()).or_default().env.insert(
            jackin_core::env_model::ZAI_API_KEY_ENV_NAME.to_owned(),
            jackin_config::EnvValue::Plain("zai".into()),
        );
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Zai);

        let rows = editor.auth_flat_rows(&jackin_config::AppConfig::default());
        editor.active_field = FieldFocus::Row(
            rows.iter()
                .position(|row| matches!(row, AuthRow::RoleHeader { role, .. } if role == "dev"))
                .expect("role header should be present"),
        );
        editor.clear_auth_row_at_cursor(&jackin_config::AppConfig::default());

        assert!(
            !editor.pending.roles["dev"]
                .env
                .contains_key(jackin_core::env_model::ZAI_API_KEY_ENV_NAME)
        );
    }

    #[test]
    fn editor_secrets_flat_rows_reads_pending_workspace_env() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor
            .pending
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

        assert!(editor.secrets_flat_rows().iter().any(|row| matches!(
            row,
            SecretsRow::WorkspaceKeyRow(key) if key == "TOKEN"
        )));
    }

    #[test]
    fn editor_selection_bounds_reads_state_and_config_counts() {
        let workspace = WorkspaceConfig {
            mounts: vec![
                MountConfig {
                    src: "/src-a".into(),
                    dst: "/dst-a".into(),
                    readonly: false,
                    isolation: MountIsolation::Shared,
                },
                MountConfig {
                    src: "/src-b".into(),
                    dst: "/dst-b".into(),
                    readonly: true,
                    isolation: MountIsolation::Shared,
                },
            ],
            ..Default::default()
        };
        let mut config = jackin_config::AppConfig::default();
        config.roles.insert("alpha".into(), RoleSource::default());
        config.roles.insert("beta".into(), RoleSource::default());
        config.roles.insert("gamma".into(), RoleSource::default());
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.active_tab = EditorTab::Mounts;
        assert_eq!(editor.selection_bounds(&config), (2, Vec::new()));

        editor.active_tab = EditorTab::Roles;
        assert_eq!(editor.selection_bounds(&config), (3, Vec::new()));
    }

    #[test]
    fn editor_field_selection_key_plan_includes_bounds_and_footer() {
        let workspace = WorkspaceConfig {
            mounts: vec![
                MountConfig {
                    src: "/src-a".into(),
                    dst: "/dst-a".into(),
                    readonly: false,
                    isolation: MountIsolation::Shared,
                },
                MountConfig {
                    src: "/src-b".into(),
                    dst: "/dst-b".into(),
                    readonly: false,
                    isolation: MountIsolation::Shared,
                },
            ],
            ..Default::default()
        };
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.active_tab = EditorTab::Mounts;
        editor.cached_footer_h = 3;
        let term = ratatui::layout::Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        };

        assert_eq!(
            editor.field_selection_key_plan(&jackin_config::AppConfig::default(), 1, term),
            EditorFieldSelectionKeyPlan {
                delta: 1,
                max_row: 2,
                skipped_rows: Vec::new(),
                term,
                footer_h: 3,
            }
        );
    }

    #[test]
    fn editor_navigation_key_plan_follows_tab_focus() {
        use crossterm::event::KeyCode;

        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        assert_eq!(
            editor.navigation_key_plan(KeyCode::Left),
            EditorNavigationKeyPlan::MoveTab {
                delta: -1,
                focus_tab_bar: true,
            }
        );
        assert_eq!(
            editor.navigation_key_plan(KeyCode::Right),
            EditorNavigationKeyPlan::MoveTab {
                delta: 1,
                focus_tab_bar: true,
            }
        );
        assert_eq!(
            editor.navigation_key_plan(KeyCode::Down),
            EditorNavigationKeyPlan::FocusContent
        );

        editor.set_tab_bar_focused(false);
        assert_eq!(
            editor.navigation_key_plan(KeyCode::Tab),
            EditorNavigationKeyPlan::MoveTab {
                delta: 1,
                focus_tab_bar: true,
            }
        );
        assert_eq!(
            editor.navigation_key_plan(KeyCode::BackTab),
            EditorNavigationKeyPlan::FocusTabBar
        );
        assert_eq!(
            editor.navigation_key_plan(KeyCode::Down),
            EditorNavigationKeyPlan::NotNavigation
        );
    }

    // Editor top-level dispatch precedence is covered against the real
    // keymap-based resolver in
    // `input::editor::tests::dispatch_editor_top_level_preserves_precedence`.

    #[test]
    fn editor_immediate_action_key_plan_routes_tab_actions() {
        use crossterm::event::{KeyCode, KeyModifiers};

        let mut workspace = WorkspaceConfig::default();
        workspace.env.insert(
            "TOKEN".into(),
            jackin_config::EnvValue::Plain("secret".into()),
        );
        workspace.mounts.push(MountConfig {
            src: "/src".into(),
            dst: "/dst".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        let config = jackin_config::AppConfig::default();

        editor.active_tab = EditorTab::Auth;
        assert_eq!(
            editor.immediate_action_key_plan(&config, KeyCode::Enter, KeyModifiers::empty()),
            EditorImmediateActionKeyPlan::EnterAuthKind(crate::tui::auth::AuthKind::Claude)
        );

        editor.active_tab = EditorTab::General;
        assert_eq!(
            editor.immediate_action_key_plan(&config, KeyCode::Char(' '), KeyModifiers::empty()),
            EditorImmediateActionKeyPlan::ToggleGeneralSelected
        );

        editor.active_tab = EditorTab::Mounts;
        assert_eq!(
            editor.immediate_action_key_plan(&config, KeyCode::Char('r'), KeyModifiers::empty()),
            EditorImmediateActionKeyPlan::ToggleMountReadonlySelected
        );

        editor.active_tab = EditorTab::Secrets;
        assert_eq!(
            editor.immediate_action_key_plan(&config, KeyCode::Char('m'), KeyModifiers::empty()),
            EditorImmediateActionKeyPlan::ToggleSecretMask {
                scope: super::SecretsScopeTag::Workspace,
                key: "TOKEN".into(),
            }
        );
        assert_eq!(
            editor.immediate_action_key_plan(&config, KeyCode::Char('m'), KeyModifiers::CONTROL),
            EditorImmediateActionKeyPlan::NotImmediateAction
        );
    }

    #[test]
    fn editor_role_action_key_plan_routes_role_tab_actions() {
        use crossterm::event::KeyCode;

        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.active_tab = EditorTab::Roles;

        assert_eq!(
            editor.role_action_key_plan(KeyCode::Char('a')),
            EditorRoleActionKeyPlan::OpenRoleInput
        );
        assert_eq!(
            editor.role_action_key_plan(KeyCode::Char('A')),
            EditorRoleActionKeyPlan::OpenRoleInput
        );
        assert_eq!(
            editor.role_action_key_plan(KeyCode::Char(' ')),
            EditorRoleActionKeyPlan::ToggleAllowed
        );
        assert_eq!(
            editor.role_action_key_plan(KeyCode::Char('*')),
            EditorRoleActionKeyPlan::ToggleDefault
        );
        assert_eq!(
            editor.role_action_key_plan(KeyCode::Char('x')),
            EditorRoleActionKeyPlan::NotRoleAction
        );

        editor.active_tab = EditorTab::Mounts;
        assert_eq!(
            editor.role_action_key_plan(KeyCode::Char('a')),
            EditorRoleActionKeyPlan::NotRoleAction
        );
    }

    #[test]
    fn editor_mount_action_key_plan_routes_mount_tab_actions() {
        use crossterm::event::KeyCode;

        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.active_tab = EditorTab::Mounts;

        assert_eq!(
            editor.mount_action_key_plan(KeyCode::Char('a')),
            EditorMountActionKeyPlan::AddMount
        );
        assert_eq!(
            editor.mount_action_key_plan(KeyCode::Char('A')),
            EditorMountActionKeyPlan::AddMount
        );
        assert_eq!(
            editor.mount_action_key_plan(KeyCode::Char('d')),
            EditorMountActionKeyPlan::RemoveSelectedMount
        );
        assert_eq!(
            editor.mount_action_key_plan(KeyCode::Char('i')),
            EditorMountActionKeyPlan::CycleIsolation
        );
        assert_eq!(
            editor.mount_action_key_plan(KeyCode::Char('o')),
            EditorMountActionKeyPlan::OpenGithub
        );
        assert_eq!(
            editor.mount_action_key_plan(KeyCode::Char('x')),
            EditorMountActionKeyPlan::NotMountAction
        );

        editor.active_tab = EditorTab::Roles;
        assert_eq!(
            editor.mount_action_key_plan(KeyCode::Char('a')),
            EditorMountActionKeyPlan::NotMountAction
        );
    }

    #[test]
    fn editor_secrets_action_key_plan_routes_secrets_tab_actions() {
        use crossterm::event::{KeyCode, KeyModifiers};

        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.active_tab = EditorTab::Secrets;

        assert_eq!(
            editor.secrets_action_key_plan(KeyCode::Char('p'), KeyModifiers::empty(), true),
            EditorSecretsActionKeyPlan::OpenPicker
        );
        assert_eq!(
            editor.secrets_action_key_plan(KeyCode::Char('P'), KeyModifiers::SHIFT, true),
            EditorSecretsActionKeyPlan::OpenPicker
        );
        assert_eq!(
            editor.secrets_action_key_plan(KeyCode::Char('p'), KeyModifiers::empty(), false),
            EditorSecretsActionKeyPlan::NotSecretsAction
        );
        assert_eq!(
            editor.secrets_action_key_plan(KeyCode::Char('d'), KeyModifiers::empty(), true),
            EditorSecretsActionKeyPlan::OpenDeleteConfirm
        );
        assert_eq!(
            editor.secrets_action_key_plan(KeyCode::Char('a'), KeyModifiers::empty(), true),
            EditorSecretsActionKeyPlan::OpenAddModal
        );
        assert_eq!(
            editor.secrets_action_key_plan(KeyCode::Char('a'), KeyModifiers::CONTROL, true),
            EditorSecretsActionKeyPlan::NotSecretsAction
        );

        editor.active_tab = EditorTab::Roles;
        assert_eq!(
            editor.secrets_action_key_plan(KeyCode::Char('a'), KeyModifiers::empty(), true),
            EditorSecretsActionKeyPlan::NotSecretsAction
        );
    }

    #[test]
    fn editor_auth_action_key_plan_routes_auth_tab_actions() {
        use crossterm::event::KeyCode;

        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.active_tab = EditorTab::Auth;

        assert_eq!(
            editor.auth_action_key_plan(KeyCode::Char('a')),
            EditorAuthActionKeyPlan::NotAuthAction
        );

        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);
        assert_eq!(
            editor.auth_action_key_plan(KeyCode::Char('a')),
            EditorAuthActionKeyPlan::OpenRolePicker
        );
        assert_eq!(
            editor.auth_action_key_plan(KeyCode::Char('A')),
            EditorAuthActionKeyPlan::OpenRolePicker
        );
        assert_eq!(
            editor.auth_action_key_plan(KeyCode::Char('d')),
            EditorAuthActionKeyPlan::ClearFocusedRow
        );
        assert_eq!(
            editor.auth_action_key_plan(KeyCode::Char('x')),
            EditorAuthActionKeyPlan::NotAuthAction
        );

        editor.active_tab = EditorTab::Roles;
        assert_eq!(
            editor.auth_action_key_plan(KeyCode::Char('d')),
            EditorAuthActionKeyPlan::NotAuthAction
        );
    }

    #[test]
    fn editor_tab_action_key_plan_routes_active_tab_precedence() {
        use crossterm::event::{KeyCode, KeyModifiers};

        let config = jackin_config::AppConfig::default();
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.active_tab = EditorTab::Mounts;
        assert_eq!(
            editor.tab_action_key_plan(&config, KeyCode::Char('a'), KeyModifiers::empty(), true,),
            EditorTabActionKeyPlan::Mount(EditorMountActionKeyPlan::AddMount)
        );

        editor.active_tab = EditorTab::Secrets;
        assert_eq!(
            editor.tab_action_key_plan(&config, KeyCode::Char('p'), KeyModifiers::empty(), true,),
            EditorTabActionKeyPlan::Secrets(EditorSecretsActionKeyPlan::OpenPicker)
        );
        assert_eq!(
            editor.tab_action_key_plan(&config, KeyCode::Char('p'), KeyModifiers::empty(), false,),
            EditorTabActionKeyPlan::Noop
        );

        editor.active_tab = EditorTab::Auth;
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);
        assert_eq!(
            editor.tab_action_key_plan(&config, KeyCode::Char('a'), KeyModifiers::empty(), true,),
            EditorTabActionKeyPlan::Auth(EditorAuthActionKeyPlan::OpenRolePicker)
        );
    }

    #[test]
    fn editor_tab_action_key_plan_delegates_enter_after_actions() {
        use crossterm::event::{KeyCode, KeyModifiers};

        let config = jackin_config::AppConfig::default();
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.active_tab = EditorTab::General;
        assert_eq!(
            editor.tab_action_key_plan(&config, KeyCode::Enter, KeyModifiers::empty(), true),
            EditorTabActionKeyPlan::Enter(EditorEnterKeyPlan::OpenGeneralField)
        );

        editor.active_tab = EditorTab::Auth;
        editor.auth_selected_kind = None;
        assert_eq!(
            editor.tab_action_key_plan(&config, KeyCode::Enter, KeyModifiers::empty(), true),
            EditorTabActionKeyPlan::Enter(EditorEnterKeyPlan::Auth(AuthEnterPlan::Noop))
        );
    }

    #[test]
    fn editor_enter_key_plan_routes_tab_actions() {
        let mut config = jackin_config::AppConfig::default();
        config.roles.insert("dev".into(), RoleSource::default());

        let mut workspace = WorkspaceConfig::default();
        workspace.env.insert(
            "A_PLAIN".into(),
            jackin_config::EnvValue::Plain("secret".into()),
        );
        workspace.env.insert(
            "Z_OP".into(),
            jackin_config::EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault/item/field".into(),
                path: "Vault/Item/Field".into(),
                account: None,
                on_demand: false,
            }),
        );
        workspace.mounts.push(MountConfig {
            src: "/src".into(),
            dst: "/dst".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        });

        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.active_tab = EditorTab::General;
        assert_eq!(
            editor.enter_key_plan(&config, true),
            EditorEnterKeyPlan::OpenGeneralField
        );

        editor.active_tab = EditorTab::Mounts;
        editor.active_field = FieldFocus::Row(0);
        assert_eq!(
            editor.enter_key_plan(&config, true),
            EditorEnterKeyPlan::Noop
        );
        editor.active_field = FieldFocus::Row(1);
        assert_eq!(
            editor.enter_key_plan(&config, true),
            EditorEnterKeyPlan::OpenMountFileBrowser
        );

        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        assert_eq!(
            editor.enter_key_plan(&config, true),
            EditorEnterKeyPlan::OpenSecretsEnterModal
        );
        editor.active_field = FieldFocus::Row(1);
        assert_eq!(
            editor.enter_key_plan(&config, true),
            EditorEnterKeyPlan::OpenSecretsPicker
        );
        assert_eq!(
            editor.enter_key_plan(&config, false),
            EditorEnterKeyPlan::OpenSecretsEnterModal
        );

        editor.active_tab = EditorTab::Roles;
        editor.active_field = FieldFocus::Row(1);
        assert_eq!(
            editor.enter_key_plan(&config, true),
            EditorEnterKeyPlan::OpenRoleInput
        );

        editor.active_tab = EditorTab::Auth;
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Github);
        editor.active_field = FieldFocus::Row(0);
        assert_eq!(
            editor.enter_key_plan(&config, true),
            EditorEnterKeyPlan::Auth(AuthEnterPlan::OpenForm)
        );
    }

    #[test]
    fn editor_escape_key_plan_routes_focus_auth_and_dirty_state() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.set_tab_bar_focused(false);
        editor.active_tab = EditorTab::General;
        assert_eq!(editor.escape_key_plan(), EditorEscapeKeyPlan::FocusTabBar);

        editor.active_tab = EditorTab::Auth;
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Claude);
        assert_eq!(
            editor.escape_key_plan(),
            EditorEscapeKeyPlan::FocusTabBarAndClearAuthKind
        );

        editor.set_tab_bar_focused(true);
        assert_eq!(editor.escape_key_plan(), EditorEscapeKeyPlan::ClearAuthKind);

        editor.auth_selected_kind = None;
        assert_eq!(
            editor.escape_key_plan(),
            EditorEscapeKeyPlan::ReloadFromConfig
        );

        editor.pending_name = Some("beta".into());
        assert_eq!(
            editor.escape_key_plan(),
            EditorEscapeKeyPlan::OpenSaveDiscard
        );
    }

    #[test]
    fn editor_save_key_plan_only_saves_dirty_editor() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        assert_eq!(editor.save_key_plan(), EditorSaveKeyPlan::Noop);

        editor.pending_name = Some("beta".into());
        assert_eq!(editor.save_key_plan(), EditorSaveKeyPlan::BeginSave);
    }

    #[test]
    fn editor_focused_add_row_selection_reads_counts() {
        let workspace = WorkspaceConfig {
            mounts: vec![
                MountConfig {
                    src: "/src-a".into(),
                    dst: "/dst-a".into(),
                    readonly: false,
                    isolation: MountIsolation::Shared,
                },
                MountConfig {
                    src: "/src-b".into(),
                    dst: "/dst-b".into(),
                    readonly: false,
                    isolation: MountIsolation::Shared,
                },
            ],
            ..Default::default()
        };
        let mut config = jackin_config::AppConfig::default();
        config.roles.insert("alpha".into(), RoleSource::default());
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.active_field = FieldFocus::Row(1);
        assert!(!editor.focused_mount_add_row_selected());
        assert!(editor.focused_role_add_row_selected(&config));

        editor.active_field = FieldFocus::Row(2);
        assert!(editor.focused_mount_add_row_selected());
        assert!(!editor.focused_role_add_row_selected(&config));
    }

    #[test]
    fn editor_focused_mount_github_open_plan_reads_cache() {
        let workspace = WorkspaceConfig {
            mounts: vec![
                MountConfig {
                    src: "/repo".into(),
                    dst: "/repo".into(),
                    readonly: false,
                    isolation: MountIsolation::Shared,
                },
                MountConfig {
                    src: "/folder".into(),
                    dst: "/folder".into(),
                    readonly: false,
                    isolation: MountIsolation::Shared,
                },
            ],
            ..Default::default()
        };
        let mut editor = TestEditorWithMountCache::new_edit("alpha".into(), workspace);
        editor.mount_info_cache.store_entries([
            (
                "/repo".into(),
                crate::mount_info::MountKind::Git {
                    branch: crate::mount_info::GitBranch::Named("main".into()),
                    origin: Some(crate::mount_info::GitOrigin::Github {
                        remote_url: "git@github.com:jackin-project/jackin.git".into(),
                        web_url: "https://github.com/jackin-project/jackin/tree/main".into(),
                    }),
                },
            ),
            ("/folder".into(), crate::mount_info::MountKind::Folder),
        ]);

        assert_eq!(
            editor.focused_mount_github_open_plan(),
            EditorMountGithubOpenPlan::Open(
                "https://github.com/jackin-project/jackin/tree/main".into()
            )
        );

        editor.active_field = FieldFocus::Row(1);
        assert_eq!(
            editor.focused_mount_github_open_plan(),
            EditorMountGithubOpenPlan::NoGithubUrl
        );

        editor.active_field = FieldFocus::Row(2);
        assert_eq!(
            editor.focused_mount_github_open_plan(),
            EditorMountGithubOpenPlan::NoSelection
        );
    }

    #[test]
    fn editor_horizontal_scroll_key_plan_targets_active_area() {
        let workspace = WorkspaceConfig {
            mounts: vec![MountConfig {
                src: "/repo".into(),
                dst: "/repo".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            ..Default::default()
        };
        let mut editor = TestEditorWithMountCache::new_edit("alpha".into(), workspace);
        editor.tab_content_width = 123;

        assert_eq!(
            editor.horizontal_scroll_key_plan(-8),
            EditorHorizontalScrollKeyPlan::TabContent {
                delta: -8,
                content_width: 123,
            }
        );

        editor.active_tab = EditorTab::Mounts;
        let expected_content_width = editor.workspace_mounts_content_width();
        assert_eq!(
            editor.horizontal_scroll_key_plan(8),
            EditorHorizontalScrollKeyPlan::WorkspaceMounts {
                delta: 8,
                content_width: expected_content_width,
            }
        );
    }

    #[test]
    fn editor_secret_value_reads_workspace_and_role_env() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor
            .pending
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
        editor
            .pending
            .roles
            .entry("dev".into())
            .or_default()
            .env
            .insert(
                "ROLE_TOKEN".into(),
                jackin_config::EnvValue::OpRef(jackin_core::OpRef {
                    op: "op://vault/item/field".into(),
                    path: "Vault/Item/Field".into(),
                    account: None,
                    on_demand: false,
                }),
            );

        assert_eq!(
            editor.secret_value(&super::SecretsScopeTag::Workspace, "TOKEN"),
            Some(&jackin_config::EnvValue::Plain("one".into()))
        );
        assert!(
            editor
                .secret_value(&super::SecretsScopeTag::Role("dev".into()), "ROLE_TOKEN")
                .is_some_and(|value| matches!(value, jackin_config::EnvValue::OpRef(_)))
        );
        assert!(
            editor
                .secret_value(
                    &super::SecretsScopeTag::Role("missing".into()),
                    "ROLE_TOKEN"
                )
                .is_none()
        );
    }

    #[test]
    fn editor_delete_env_var_removes_workspace_key() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor
            .pending
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

        editor
            .delete_env_var(&super::SecretsScopeTag::Workspace, "TOKEN")
            .unwrap();

        assert!(!editor.pending.env.contains_key("TOKEN"));
    }

    #[test]
    fn editor_delete_env_var_removes_empty_role_override() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor
            .pending
            .roles
            .entry("dev".into())
            .or_default()
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

        editor
            .delete_env_var(&super::SecretsScopeTag::Role("dev".into()), "TOKEN")
            .unwrap();

        assert!(!editor.pending.roles.contains_key("dev"));
    }

    #[test]
    fn editor_delete_env_var_blocks_managed_claude_oauth_token() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.pending.claude = Some(jackin_config::AgentAuthConfig {
            auth_forward: jackin_config::AuthForwardMode::OAuthToken,
            sync_source_dir: None,
        });
        editor.pending.env.insert(
            jackin_core::env_model::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME.into(),
            jackin_config::EnvValue::Plain("token".into()),
        );

        let err = editor
            .delete_env_var(
                &super::SecretsScopeTag::Workspace,
                jackin_core::env_model::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME,
            )
            .unwrap_err();

        assert!(err.to_string().contains("claude-token revoke"));
        assert!(
            editor
                .pending
                .env
                .contains_key(jackin_core::env_model::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME)
        );
    }

    #[test]
    fn editor_secret_text_editability_rejects_op_refs() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor
            .pending
            .env
            .insert("PLAIN".into(), jackin_config::EnvValue::Plain("one".into()));
        editor.pending.env.insert(
            "OP_REF".into(),
            jackin_config::EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault/item/field".into(),
                path: "Vault/Item/Field".into(),
                account: None,
                on_demand: false,
            }),
        );

        assert!(editor.secret_is_text_editable(&super::SecretsScopeTag::Workspace, "PLAIN"));
        assert!(!editor.secret_is_text_editable(&super::SecretsScopeTag::Workspace, "OP_REF"));
    }

    #[test]
    fn editor_focused_secret_is_op_ref_reads_current_row() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.pending.env.insert(
            "A_OP_REF".into(),
            jackin_config::EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault/item/field".into(),
                path: "Vault/Item/Field".into(),
                account: None,
                on_demand: false,
            }),
        );
        editor.pending.env.insert(
            "Z_PLAIN".into(),
            jackin_config::EnvValue::Plain("one".into()),
        );

        editor.active_field = FieldFocus::Row(0);
        assert!(editor.focused_secret_is_op_ref());

        editor.active_field = FieldFocus::Row(1);
        assert!(!editor.focused_secret_is_op_ref());
    }

    #[test]
    fn editor_focused_unmask_key_skips_op_refs() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.pending.env.insert(
            "A_TOKEN".into(),
            jackin_config::EnvValue::Plain("one".into()),
        );
        editor.pending.env.insert(
            "Z_OP_REF".into(),
            jackin_config::EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault/item/field".into(),
                path: "Vault/Item/Field".into(),
                account: None,
                on_demand: false,
            }),
        );

        editor.active_field = FieldFocus::Row(0);
        assert_eq!(
            editor.focused_unmask_key(),
            Some((super::SecretsScopeTag::Workspace, "A_TOKEN".into()))
        );

        editor.active_field = FieldFocus::Row(1);
        assert_eq!(editor.focused_unmask_key(), None);
    }

    #[test]
    fn editor_focused_secret_enter_plan_reads_current_row() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor
            .pending
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

        assert_eq!(
            editor.focused_secret_enter_plan(),
            super::SecretsEnterPlan::EditValue {
                scope: super::SecretsScopeTag::Workspace,
                key: "TOKEN".into()
            }
        );

        editor.active_field = FieldFocus::Row(1);
        assert_eq!(
            editor.focused_secret_enter_plan(),
            super::SecretsEnterPlan::Noop
        );

        editor.active_field = FieldFocus::Row(2);
        assert_eq!(
            editor.focused_secret_enter_plan(),
            super::SecretsEnterPlan::OpenScopePicker
        );
    }

    #[test]
    fn editor_focused_secrets_role_expansion_plan_reads_current_row() {
        let workspace = WorkspaceConfig {
            roles: std::collections::BTreeMap::from([(
                "dev".into(),
                WorkspaceRoleOverride::default(),
            )]),
            ..Default::default()
        };
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.active_field = FieldFocus::Row(
            editor
                .secrets_flat_rows()
                .iter()
                .position(|row| matches!(row, SecretsRow::RoleHeader { role, .. } if role == "dev"))
                .expect("role header row"),
        );

        assert_eq!(
            editor.focused_secrets_role_expansion_plan(true),
            RoleHeaderExpansionPlan::Set {
                role: "dev".into(),
                expanded: true
            }
        );

        editor.secrets_expanded.insert("dev".into());
        assert_eq!(
            editor.focused_secrets_role_expansion_plan(true),
            RoleHeaderExpansionPlan::HeaderNoop
        );
        assert_eq!(
            editor.focused_secrets_role_expansion_plan(false),
            RoleHeaderExpansionPlan::Set {
                role: "dev".into(),
                expanded: false
            }
        );
    }

    #[test]
    fn editor_focused_secret_targets_read_current_row() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor
            .pending
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));

        assert_eq!(
            editor.focused_secret_delete_target(),
            Some((super::SecretsScopeTag::Workspace, "TOKEN".into()))
        );
        assert_eq!(
            editor.focused_secret_add_target(),
            Some(super::SecretsScopeTag::Workspace)
        );

        editor.active_field = FieldFocus::Row(1);
        assert_eq!(editor.focused_secret_delete_target(), None);
        assert_eq!(editor.focused_secret_add_target(), None);
    }

    #[test]
    fn editor_change_count_tracks_env_and_role_auth() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        assert_eq!(editor.change_count(), 0);

        editor
            .pending
            .env
            .insert("TOKEN".into(), jackin_config::EnvValue::Plain("one".into()));
        editor.pending.roles.entry("dev".into()).or_default().github =
            Some(jackin_config::GithubAuthConfig {
                auth_forward: jackin_config::GithubAuthMode::Token,
                ..Default::default()
            });

        assert_eq!(editor.change_count(), 4);
    }

    #[test]
    fn editor_cycle_isolation_for_selected_mount_updates_pending_mount() {
        let mut workspace = WorkspaceConfig::default();
        workspace.mounts.push(MountConfig {
            src: "/host".into(),
            dst: "/work".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.cycle_isolation_for_selected_mount();

        assert_eq!(editor.pending.mounts[0].isolation, MountIsolation::Worktree);
    }

    #[test]
    fn editor_remove_selected_mount_deletes_pending_mount() {
        let mut workspace = WorkspaceConfig::default();
        workspace.mounts.push(MountConfig {
            src: "/host".into(),
            dst: "/work".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        });
        workspace.mounts.push(MountConfig {
            src: "/host2".into(),
            dst: "/work2".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.active_field = FieldFocus::Row(1);

        editor.remove_selected_mount();

        assert_eq!(editor.pending.mounts.len(), 1);
        assert_eq!(editor.pending.mounts[0].src, "/host");
    }

    #[test]
    fn editor_add_shared_mount_appends_pending_mount() {
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());

        editor.add_shared_mount("/host", "/work");

        assert_eq!(editor.pending.mounts.len(), 1);
        assert_eq!(editor.pending.mounts[0].src, "/host");
        assert_eq!(editor.pending.mounts[0].dst, "/work");
        assert_eq!(editor.pending.mounts[0].isolation, MountIsolation::Shared);
    }

    #[test]
    fn editor_eligible_role_override_selectors_use_workspace_allowed_roles() {
        let mut workspace = WorkspaceConfig {
            allowed_roles: vec!["beta".into()],
            ..Default::default()
        };
        workspace.roles.entry("alpha".into()).or_default();
        let editor = TestEditor::new_edit("alpha".into(), workspace);
        let registered = ["alpha".to_owned(), "beta".to_owned(), "bad role".to_owned()];

        let eligible = editor.eligible_role_override_selectors(registered.iter());

        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].name.as_str(), "beta");
    }

    #[test]
    fn editor_auth_role_override_selectors_filter_existing_overrides() {
        let mut workspace = WorkspaceConfig {
            allowed_roles: vec!["alpha".into(), "beta".into()],
            ..Default::default()
        };
        workspace.roles.entry("alpha".into()).or_default().github =
            Some(jackin_config::GithubAuthConfig {
                auth_forward: jackin_config::GithubAuthMode::Token,
                ..Default::default()
            });
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);
        editor.auth_selected_kind = Some(crate::tui::auth::AuthKind::Github);
        let registered = ["alpha".to_owned(), "beta".to_owned(), "bad role".to_owned()];

        let eligible = editor
            .auth_role_override_selectors(registered.iter())
            .expect("selected kind should produce candidates");

        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].name.as_str(), "beta");
    }

    #[test]
    fn editor_auth_role_override_selectors_require_selected_kind() {
        let editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        let registered = ["alpha".to_owned()];

        assert!(
            editor
                .auth_role_override_selectors(registered.iter())
                .is_none()
        );
    }

    #[test]
    fn editor_toggle_allowed_role_at_cursor_updates_pending_allow_list_and_default() {
        let workspace = WorkspaceConfig {
            default_role: Some("alpha".into()),
            ..Default::default()
        };
        let role_names = vec!["alpha".to_owned(), "beta".to_owned()];
        let mut editor = TestEditor::new_edit("alpha".into(), workspace);

        editor.toggle_allowed_role_at_cursor(&role_names);

        assert_eq!(editor.pending.allowed_roles, vec!["beta".to_owned()]);
        assert_eq!(editor.pending.default_role, None);
    }

    #[test]
    fn editor_toggle_default_role_at_cursor_only_sets_allowed_role() {
        let role_names = vec!["alpha".to_owned(), "beta".to_owned()];
        let mut editor = TestEditor::new_edit("alpha".into(), WorkspaceConfig::default());
        editor.active_field = FieldFocus::Row(1);

        editor.toggle_default_role_at_cursor(&role_names);
        assert_eq!(editor.pending.default_role.as_deref(), Some("beta"));

        editor.pending.allowed_roles = vec!["alpha".into()];
        editor.pending.default_role = None;
        editor.toggle_default_role_at_cursor(&role_names);
        assert_eq!(editor.pending.default_role, None);
    }
}

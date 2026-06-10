//! Editor screen state: draft workspace config being edited and per-tab/
//! per-field edit state for General, Mounts, Roles, Secrets, and Auth panels.
//!
//! Not responsible for: event handling (see `update`) or rendering (see
//! `view`).

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

#[derive(Debug, Clone)]
pub enum EditorMode {
    Edit { name: String },
    Create,
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
    pub unmasked_rows: std::collections::BTreeSet<(SecretsScopeTag, String)>,
    pub secrets_expanded: std::collections::BTreeSet<String>,
    pub auth_expanded: std::collections::BTreeSet<String>,
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
            unmasked_rows: std::collections::BTreeSet::default(),
            secrets_expanded: std::collections::BTreeSet::default(),
            auth_expanded: std::collections::BTreeSet::default(),
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

    #[must_use]
    pub const fn tab_bar_focused(&self) -> bool {
        self.focus_owner.is_tab_bar()
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

    pub fn new_create() -> Self
    where
        WorkspaceConfig: Clone + Default,
        MountInfoCache: Default,
        SaveFlow: Default,
    {
        let empty = WorkspaceConfig::default();
        Self::new_edit(String::new(), empty).into_create_mode()
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

    fn drop_modal_scratch(&mut self) {
        self.pending_picker_value = None;
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
    AuthWorkspaceSourceFolder {
        kind: crate::tui::auth::AuthKind,
    },
    AuthRoleSourceFolder {
        role: String,
        kind: crate::tui::auth::AuthKind,
    },
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

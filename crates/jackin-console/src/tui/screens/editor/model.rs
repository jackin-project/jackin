// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Editor screen state: draft workspace config being edited and per-tab/
//! per-field edit state for General, Mounts, Roles, Secrets, and Auth panels.
//!
//! Not responsible for: event handling (see `update`) or rendering (see
//! `view`).

use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;

use jackin_config::WorkspaceConfig;
use jackin_tui::components::FocusOwner;

mod state_impl;

#[cfg(test)]
mod tests;

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
    pub _env_value: PhantomData<fn() -> EnvValue>,
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
    EnvKey {
        scope: SecretsScopeTag,
    },
    EnvKeyWithValue {
        scope: SecretsScopeTag,
        value: jackin_core::EnvValue,
    },
    EnvValue {
        scope: SecretsScopeTag,
        key: String,
    },
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

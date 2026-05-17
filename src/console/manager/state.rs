//! Manager state machine. See docs/superpowers/specs/2026-04-23-workspace-manager-tui-design.md § 3.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;

use ratatui::layout::Rect;

use crate::config::AppConfig;
use crate::console::op_cache::OpCache;
use crate::workspace::WorkspaceConfig;

use crate::console::widgets::{
    auth_panel::AuthForm, confirm::ConfirmState, confirm_save::ConfirmSaveState,
    error_popup::ErrorPopupState, file_browser::FileBrowserState, github_picker::GithubPickerState,
    mount_dst_choice::MountDstChoiceState, op_picker::OpPickerState, role_picker::RolePickerState,
    scope_picker::ScopePickerState, source_picker::SourcePickerState, text_input::TextInputState,
    workdir_pick::WorkdirPickState,
};

/// Logical row in the manager list. Prefer over the raw `selected:
/// usize` when reasoning about what the operator is pointing at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerListRow {
    CurrentDirectory,
    SavedWorkspace(usize),
    NewWorkspace,
}

impl ManagerListRow {
    #[must_use]
    pub const fn to_screen_index(self, saved_count: usize) -> usize {
        match self {
            Self::CurrentDirectory => 0,
            Self::SavedWorkspace(i) => i + 1,
            Self::NewWorkspace => saved_count + 1,
        }
    }

    #[must_use]
    pub const fn to_visual_index(self, saved_count: usize) -> usize {
        match self {
            Self::CurrentDirectory => 0,
            Self::SavedWorkspace(i) => i + 1,
            Self::NewWorkspace => {
                if saved_count > 0 {
                    saved_count + 2
                } else {
                    saved_count + 1
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct ManagerState<'a> {
    pub stage: ManagerStage<'a>,
    pub workspaces: Vec<WorkspaceSummary>,
    pub instances: Vec<crate::instance::InstanceIndexEntry>,
    pub current_dir: String,
    pub selected: usize,
    pub toast: Option<Toast>,
    /// Modal slot at the list level (e.g. `Modal::GithubPicker`); the
    /// Editor / `CreatePrelude` stages own their own modal slots.
    pub list_modal: Option<Modal<'a>>,
    pub inline_role_picker: Option<RolePickerState>,
    pub inline_agent_picker: Option<(
        crate::selector::RoleSelector,
        crate::console::widgets::agent_choice::AgentChoiceState,
    )>,
    pub list_mounts_scroll_x: u16,
    pub list_mounts_scroll_y: u16,
    pub list_global_mounts_scroll_x: u16,
    pub list_global_mounts_scroll_y: u16,
    pub list_role_global_mounts_scroll_x: u16,
    pub list_role_global_mounts_scroll_y: u16,
    pub list_roles_scroll_x: u16,
    pub list_roles_scroll_y: u16,
    pub list_scroll_focus: Option<MountScrollFocus>,
    pub list_names_scroll_x: u16,
    pub list_names_focused: bool,
    pub list_split_pct: u16,
    pub drag_state: Option<DragState>,
    /// Process-lifetime cache of `op` structural metadata, threaded
    /// into the picker on open. Carries no credentials — see
    /// `op_cache.rs`.
    pub op_cache: Rc<RefCell<OpCache>>,
    /// Mirrored from `ConsoleState::op_available` (probed once at
    /// startup) so the Secrets-tab editor can disable the
    /// source-picker's 1Password choice without re-probing.
    pub op_available: bool,
    /// Last known terminal size, updated at the top of every render
    /// frame. Used by keyboard handlers to compute `viewport_h` for
    /// cursor-to-viewport scroll adjustment without needing a render pass.
    pub cached_term_size: Rect,
    /// Throttle the per-tick `InstanceIndex::read_or_rebuild` poll —
    /// state on disk can't change at the 20 Hz render cadence and the
    /// rebuild path walks every container directory.
    instances_last_refresh: Option<std::time::Instant>,
    /// Last surfaced `refresh_instances` error message. Dedup gate so
    /// transient errors don't spam toasts every refresh tick.
    instances_last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountScrollFocus {
    Workspace,
    Global,
    RoleGlobal,
    Roles,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragState {
    pub anchor_pct: u16,
    pub anchor_x: u16,
}

pub const MIN_SPLIT_PCT: u16 = 20;
pub const MAX_SPLIT_PCT: u16 = 80;
pub const DEFAULT_SPLIT_PCT: u16 = 30;

#[must_use]
pub const fn clamp_split(pct: u16) -> u16 {
    if pct < MIN_SPLIT_PCT {
        MIN_SPLIT_PCT
    } else if pct > MAX_SPLIT_PCT {
        MAX_SPLIT_PCT
    } else {
        pct
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ManagerStage<'a> {
    List,
    Editor(EditorState<'a>),
    Settings(SettingsState<'a>),
    CreatePrelude(CreatePreludeState<'a>),
    ConfirmDelete { name: String, state: ConfirmState },
}

#[derive(Debug)]
pub struct GlobalMountsState<'a> {
    pub selected: usize,
    pub pending: Vec<crate::config::GlobalMountRow>,
    pub original: Vec<crate::config::GlobalMountRow>,
    pub modal: Option<GlobalMountModal<'a>>,
    pub add_draft: Option<GlobalMountDraft>,
    pub error: Option<String>,
    pub success: Option<String>,
    pub scroll_x: u16,
    pub scroll_y: u16,
    pub scroll_focused: bool,
    /// Dispatcher pops back to the workspace list when set.
    pub exit_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct SettingsGeneralState {
    pub pending_coauthor_trailer: bool,
    pub(super) original_coauthor_trailer: bool,
    pub pending_dco: bool,
    pub(super) original_dco: bool,
    pub selected: usize,
}

impl SettingsGeneralState {
    pub const fn from_config(config: &AppConfig) -> Self {
        Self {
            pending_coauthor_trailer: config.git.coauthor_trailer,
            original_coauthor_trailer: config.git.coauthor_trailer,
            pending_dco: config.git.dco,
            original_dco: config.git.dco,
            selected: 0,
        }
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.pending_coauthor_trailer != self.original_coauthor_trailer
            || self.pending_dco != self.original_dco
    }

    pub const fn discard(&mut self) {
        self.pending_coauthor_trailer = self.original_coauthor_trailer;
        self.pending_dco = self.original_dco;
    }

    #[must_use]
    pub fn change_count(&self) -> usize {
        usize::from(self.pending_coauthor_trailer != self.original_coauthor_trailer)
            + usize::from(self.pending_dco != self.original_dco)
    }
}

#[derive(Debug)]
pub struct SettingsState<'a> {
    pub active_tab: SettingsTab,
    /// W3C ARIA Tabs: when true, focus is on the tab list (←/→ cycle tabs,
    /// Tab/↓ enters content); when false, focus is in the tab panel.
    pub tab_bar_focused: bool,
    pub general: SettingsGeneralState,
    pub mounts: GlobalMountsState<'a>,
    pub env: SettingsEnvState<'a>,
    pub auth: SettingsAuthState,
    pub trust: SettingsTrustState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Mounts,
    Environments,
    Auth,
    Trust,
}

impl SettingsTab {
    pub const ALL: [Self; 5] = [
        Self::General,
        Self::Mounts,
        Self::Environments,
        Self::Auth,
        Self::Trust,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Mounts => "Mounts",
            Self::Environments => "Environments",
            Self::Auth => "Auth",
            Self::Trust => "Trust",
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    #[must_use]
    pub fn previous(self) -> Self {
        let idx = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug)]
pub struct SettingsEnvState<'a> {
    pub selected: usize,
    pub pending: SettingsEnvConfig,
    pub original: SettingsEnvConfig,
    pub modal: Option<SettingsEnvModal<'a>>,
    pub pending_env_key: Option<(SettingsEnvScope, String)>,
    pub pending_picker_target: Option<(SettingsEnvScope, Option<String>)>,
    pub pending_picker_value: Option<crate::operator_env::EnvValue>,
    pub unmasked_rows: BTreeSet<(SettingsEnvScope, String)>,
    pub expanded: BTreeSet<String>,
    pub error: Option<String>,
    pub scroll_y: u16,
    pub scroll_focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEnvConfig {
    pub env: BTreeMap<String, crate::operator_env::EnvValue>,
    pub roles: BTreeMap<String, BTreeMap<String, crate::operator_env::EnvValue>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettingsEnvScope {
    Global,
    Role(String),
}

#[derive(Debug)]
pub enum SettingsEnvModal<'a> {
    Text {
        target: SettingsEnvTextTarget,
        state: Box<TextInputState<'a>>,
    },
    SourcePicker {
        state: SourcePickerState,
    },
    OpPicker {
        state: Box<OpPickerState>,
    },
    RolePicker {
        state: RolePickerState,
    },
    ScopePicker {
        state: ScopePickerState,
    },
    Confirm {
        action: SettingsEnvConfirm,
        state: ConfirmState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEnvConfirm {
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvTextTarget {
    EnvKey {
        scope: SettingsEnvScope,
    },
    EnvValue {
        scope: SettingsEnvScope,
        key: String,
    },
}

#[derive(Debug)]
pub struct SettingsAuthState {
    pub selected: usize,
    pub selected_kind: Option<crate::console::manager::auth_kind::AuthKind>,
    pub pending: Vec<SettingsAuthRow>,
    pub original: Vec<SettingsAuthRow>,
    pub github_env: BTreeMap<String, crate::operator_env::EnvValue>,
    pub original_github_env: BTreeMap<String, crate::operator_env::EnvValue>,
    pub modal: Option<SettingsAuthModal<'static>>,
    pub pending_auth_form_return: Option<AuthFormReturnPath>,
    pub error: Option<String>,
    pub scroll_y: u16,
    pub scroll_focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsAuthRow {
    pub kind: crate::console::manager::auth_kind::AuthKind,
    pub mode: crate::console::manager::auth_kind::AuthMode,
}

#[derive(Debug)]
pub enum SettingsAuthModal<'a> {
    TextInput {
        state: Box<TextInputState<'a>>,
    },
    SourcePicker {
        state: SourcePickerState,
    },
    OpPicker {
        state: Box<OpPickerState>,
    },
    AuthForm {
        target: AuthFormTarget,
        state: Box<AuthForm>,
        focus: AuthFormFocus,
        literal_buffer: String,
    },
}

#[derive(Debug)]
pub struct SettingsTrustState {
    pub selected: usize,
    pub pending: Vec<SettingsTrustRow>,
    pub original: Vec<SettingsTrustRow>,
    pub error: Option<String>,
    pub scroll_x: u16,
    pub scroll_y: u16,
    pub scroll_focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsTrustRow {
    pub role: String,
    pub git: String,
    pub trusted: bool,
}

#[derive(Debug, Default)]
pub struct GlobalMountDraft {
    pub name: String,
    pub src: String,
    pub dst: String,
    pub scope: Option<String>,
}

#[derive(Debug)]
pub enum GlobalMountModal<'a> {
    Text {
        target: GlobalMountTextTarget,
        state: Box<TextInputState<'a>>,
    },
    FileBrowser {
        state: Box<FileBrowserState>,
    },
    MountDstChoice {
        state: MountDstChoiceState,
    },
    ScopePicker {
        state: ScopePickerState,
    },
    RolePicker {
        state: RolePickerState,
    },
    Confirm {
        action: GlobalMountConfirm,
        state: ConfirmState,
    },
    /// Full change-preview dialog shown before committing a settings save.
    /// Reuses the `ConfirmSave` widget so the operator sees the same
    /// scrollable diff format as the workspace editor.
    PreviewSave {
        state: crate::console::widgets::confirm_save::ConfirmSaveState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountConfirm {
    Remove,
    Save,
    Sensitive,
    Discard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountTextTarget {
    AddScope,
    AddName,
    AddSource,
    AddDestination,
    Source,
    Destination,
    Scope,
    Rename,
}

#[derive(Debug, Clone)]
pub struct WorkspaceSummary {
    pub name: String,
    pub workdir: String,
    pub mount_count: usize,
    pub readonly_mount_count: usize,
    pub allowed_role_count: usize,
    pub default_role: Option<String>,
    pub last_role: Option<String>,
}

#[derive(Debug)]
pub struct EditorState<'a> {
    pub mode: EditorMode,
    pub active_tab: EditorTab,
    /// W3C ARIA Tabs: when true, focus is on the tab list (←/→ cycle tabs,
    /// Tab/↓ enters content); when false, focus is in the tab panel.
    pub tab_bar_focused: bool,
    pub active_field: FieldFocus,
    pub original: WorkspaceConfig,
    pub pending: WorkspaceConfig,
    pub modal: Option<Modal<'a>>,
    /// Create-mode only; Edit mode reads name from `EditorMode::Edit`.
    pub pending_name: Option<String>,
    /// Signals the outer `handle_key` to save and/or pop to List.
    pub exit_after_save: Option<ExitIntent>,
    pub save_flow: EditorSaveFlow,
    /// Secrets tab: keys whose value is currently unmasked. Cleared on
    /// tab leave so re-entry starts all-masked. Op:// rows ignore this
    /// — they render as a breadcrumb, not a masked value.
    pub unmasked_rows: BTreeSet<(SecretsScopeTag, String)>,
    pub secrets_expanded: BTreeSet<String>,
    pub auth_expanded: BTreeSet<String>,
    /// Auth tab two-screen state: `None` renders the auth-kind
    /// picker; `Some(kind)` renders the focused editor for that
    /// auth kind. Cleared by Esc on the focused screen (see the Auth
    /// branch in `input::editor::handle_editor_key`'s `KeyCode::Esc`
    /// arm) and by Tab/BackTab leaving the Auth tab. The Esc-pop
    /// is in-tab navigation and intentionally bypasses the
    /// dirty-modal flow — pending edits stay in `editor.pending`.
    ///
    /// Widened from `Agent` to [`AuthKind`] so `Github` can sit on
    /// the panel without forcing a runtime `Agent::Github` variant.
    pub auth_selected_kind: Option<crate::console::manager::auth_kind::AuthKind>,
    /// Scratch for the two-step add flow: set on `EnvKey` commit,
    /// cleared on `EnvValue` commit/cancel.
    pub pending_env_key: Option<(SecretsScopeTag, String)>,
    /// Stashed by `P` on a Secrets row so `OpPicker` knows where to
    /// write its `op://` path. `Some((scope, Some(key)))` replaces a
    /// row's value; `Some((scope, None))` opens the `EnvKey` modal
    /// next with the value pre-stashed in `pending_picker_value`.
    pub pending_picker_target: Option<(SecretsScopeTag, Option<String>)>,
    /// In the sentinel-add flow, holds the picker-supplied `OpRef`
    /// (wrapped as `EnvValue::OpRef`) until the operator names the key
    /// and the `EnvKey` modal commits both fields at once.
    pub pending_picker_value: Option<crate::operator_env::EnvValue>,
    /// Stash for the auth-form ↔ side-modal round trips. Set when the
    /// operator presses Enter on the credential row, and consumed when
    /// the side modal commits or cancels:
    ///
    ///   - `AuthSourcePicker` (literal) → `TextInput` → `AuthForm`
    ///   - `AuthSourcePicker` (1Password) → `OpPicker` → `AuthForm`
    ///
    /// On commit the form is reconstructed with the new credential
    /// applied (literal text via `set_literal`, `OpRef` via
    /// `try_commit_op_ref`); on cancel it's reconstructed pristine.
    /// Threading the auth-form context through this single field
    /// (rather than via a payload on each side variant) keeps the
    /// picker/text-input variants orthogonal to their caller, at the
    /// cost of an invariant that only the side-modal handlers
    /// touch this slot — see `AUTH00x` debug tags in
    /// `input::auth` for the recovery path on stash desync.
    pub pending_auth_form_return: Option<AuthFormReturnPath>,
    pub workspace_mounts_scroll_x: u16,
    pub workspace_mounts_scroll_focused: bool,
    /// Horizontal scroll offset shared across non-Mounts editor content tabs.
    /// Reset to 0 on every tab change so each tab starts at the left edge.
    pub tab_scroll_x: u16,
    /// Vertical scroll offset shared across all editor content tabs.
    /// Reset to 0 on every tab change so each tab starts at the top.
    pub tab_scroll_y: u16,
    /// Whether the non-Mounts tab content block has keyboard/click focus
    /// (green border). Updated each click via `update_scroll_focus`.
    pub tab_content_scroll_focused: bool,
    /// Last rendered line count for the active non-Mounts tab content block.
    /// Written by the render function; read by `update_scroll_focus` to
    /// determine whether the block is actually scrollable.
    pub tab_content_width: usize,
    pub tab_content_height: usize,
}

/// Captured auth-form context to re-mount the form after a side
/// modal (`AuthSourcePicker`, `TextInput`, or `OpPicker`) commits or
/// cancels.
///
/// `state` and `literal_buffer` are stashed so a half-typed literal
/// isn't lost when the operator detours through the source picker
/// → text-input round trip and cancels back.
#[derive(Debug)]
pub struct AuthFormReturnPath {
    pub target: AuthFormTarget,
    pub state: Box<AuthForm>,
    pub focus: AuthFormFocus,
    pub literal_buffer: String,
}

/// Save cycle state machine.
///
/// `Idle` → (open `ConfirmSave`) `Confirming` → (stash plan)
/// `PendingCommit` → (outer loop writes to disk) `Idle` or `Error`.
/// `exit_on_success` is true when save came from `SaveDiscardCancel`
/// — outer loop pops to list on success. Pre-commit validation
/// errors land in `Error` and render as an inline banner instead of
/// a modal.
#[derive(Debug, Clone, Default)]
pub enum EditorSaveFlow {
    #[default]
    Idle,
    Confirming {
        exit_on_success: bool,
    },
    PendingCommit {
        plan: PendingSaveCommit,
        exit_on_success: bool,
    },
    Error {
        message: String,
    },
}

impl EditorSaveFlow {
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

impl GlobalMountsState<'_> {
    pub fn from_config(config: &AppConfig) -> Self {
        let rows = config.list_mount_rows();
        Self {
            selected: 0,
            pending: rows.clone(),
            original: rows,
            modal: None,
            add_draft: None,
            error: None,
            success: None,
            scroll_x: 0,
            scroll_y: 0,
            scroll_focused: false,
            exit_requested: false,
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.pending != self.original
    }

    pub fn discard(&mut self) {
        self.pending = self.original.clone();
        self.selected = self.selected.min(self.pending.len().saturating_sub(1));
        self.add_draft = None;
        self.error = None;
        self.success = None;
    }

    pub fn save_to_config(
        &mut self,
        paths: &crate::paths::JackinPaths,
    ) -> anyhow::Result<AppConfig> {
        AppConfig::validate_global_mount_rows(&self.pending)?;
        let mut editor = crate::config::ConfigEditor::open(paths)?;
        for row in &self.original {
            editor.remove_mount(&row.name, row.scope.as_deref());
        }
        for row in &self.pending {
            editor.add_mount(&row.name, row.mount.clone(), row.scope.as_deref());
        }
        let config = editor.save()?;
        self.original = self.pending.clone();
        Ok(config)
    }
}

impl SettingsState<'_> {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            active_tab: SettingsTab::General,
            tab_bar_focused: true,
            general: SettingsGeneralState::from_config(config),
            mounts: GlobalMountsState::from_config(config),
            env: SettingsEnvState::from_config(config),
            auth: SettingsAuthState::from_config(config),
            trust: SettingsTrustState::from_config(config),
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.general.is_dirty()
            || self.mounts.is_dirty()
            || self.env.is_dirty()
            || self.auth.is_dirty()
            || self.trust.is_dirty()
    }

    #[must_use]
    pub fn change_count(&self) -> usize {
        self.general.change_count()
            + self.mounts_change_count()
            + self.env.change_count()
            + settings_vec_change_count(&self.auth.original, &self.auth.pending)
            + env_change_count(&self.auth.original_github_env, &self.auth.github_env)
            + settings_vec_change_count(&self.trust.original, &self.trust.pending)
    }

    fn mounts_change_count(&self) -> usize {
        settings_vec_change_count(&self.mounts.original, &self.mounts.pending)
    }

    pub fn discard(&mut self) {
        self.general.discard();
        self.mounts.discard();
        self.env.discard();
        self.auth.discard();
        self.trust.discard();
    }

    pub fn save_to_config(
        &mut self,
        paths: &crate::paths::JackinPaths,
    ) -> anyhow::Result<AppConfig> {
        AppConfig::validate_global_mount_rows(&self.mounts.pending)?;
        validate_settings_env(&self.env.pending, &self.trust.pending)?;
        let mut editor = crate::config::ConfigEditor::open(paths)?;

        for row in &self.mounts.original {
            editor.remove_mount(&row.name, row.scope.as_deref());
        }
        for row in &self.mounts.pending {
            editor.add_mount(&row.name, row.mount.clone(), row.scope.as_deref());
        }

        for key in self.env.original.env.keys() {
            editor.remove_env_var(&crate::config::EnvScope::Global, key);
        }
        for (role, env) in &self.env.original.roles {
            for key in env.keys() {
                editor.remove_env_var(&crate::config::EnvScope::Role(role.clone()), key);
            }
        }
        for (key, value) in &self.env.pending.env {
            editor.set_env_var(&crate::config::EnvScope::Global, key, value.clone())?;
        }
        for (role, env) in &self.env.pending.roles {
            for (key, value) in env {
                editor.set_env_var(
                    &crate::config::EnvScope::Role(role.clone()),
                    key,
                    value.clone(),
                )?;
            }
        }

        for row in &self.auth.pending {
            match row.kind {
                crate::console::manager::auth_kind::AuthKind::Claude
                | crate::console::manager::auth_kind::AuthKind::Codex
                | crate::console::manager::auth_kind::AuthKind::Amp
                | crate::console::manager::auth_kind::AuthKind::Kimi
                | crate::console::manager::auth_kind::AuthKind::Opencode => {
                    let Some(agent) = row.kind.agent() else {
                        continue;
                    };
                    let Some(mode) = row.mode.to_auth_forward() else {
                        anyhow::bail!(
                            "auth mode {} is not supported for {}",
                            row.mode.as_str(),
                            row.kind.label()
                        );
                    };
                    editor.set_global_auth_forward(agent, mode);
                }
                crate::console::manager::auth_kind::AuthKind::Github => {
                    let Some(mode) = row.mode.to_github() else {
                        anyhow::bail!(
                            "auth mode {} is not supported for {}",
                            row.mode.as_str(),
                            row.kind.label()
                        );
                    };
                    editor.set_global_github_auth_forward(mode);
                }
            }
        }
        for key in self.auth.original_github_env.keys() {
            editor.remove_global_github_env_var(key);
        }
        for (key, value) in &self.auth.github_env {
            editor.set_global_github_env_var(key, value.clone())?;
        }

        for row in &self.trust.pending {
            editor.set_agent_trust(&row.role, row.trusted);
        }

        editor.set_git_coauthor_trailer(self.general.pending_coauthor_trailer);
        editor.set_git_dco(self.general.pending_dco);

        let config = editor.save()?;
        self.general.original_coauthor_trailer = self.general.pending_coauthor_trailer;
        self.general.original_dco = self.general.pending_dco;
        self.mounts.original = self.mounts.pending.clone();
        self.env.original = self.env.pending.clone();
        self.auth.original = self.auth.pending.clone();
        self.auth.original_github_env = self.auth.github_env.clone();
        self.trust.original = self.trust.pending.clone();
        Ok(config)
    }
}

fn settings_vec_change_count<T: PartialEq>(original: &[T], pending: &[T]) -> usize {
    let common_changes = original
        .iter()
        .zip(pending.iter())
        .filter(|(a, b)| a != b)
        .count();
    common_changes + original.len().abs_diff(pending.len())
}

impl SettingsEnvState<'_> {
    pub fn from_config(config: &AppConfig) -> Self {
        let pending = SettingsEnvConfig {
            env: config.env.clone(),
            roles: config
                .roles
                .iter()
                .map(|(role, source)| (role.clone(), source.env.clone()))
                .collect(),
        };
        Self {
            selected: 0,
            original: pending.clone(),
            pending,
            modal: None,
            pending_env_key: None,
            pending_picker_target: None,
            pending_picker_value: None,
            unmasked_rows: BTreeSet::default(),
            expanded: BTreeSet::default(),
            error: None,
            scroll_y: 0,
            scroll_focused: false,
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.pending != self.original
    }

    pub fn discard(&mut self) {
        self.pending = self.original.clone();
        self.selected = self
            .selected
            .min(settings_env_flat_row_count(self).saturating_sub(1));
        self.modal = None;
        self.pending_env_key = None;
        self.pending_picker_target = None;
        self.pending_picker_value = None;
        self.unmasked_rows.clear();
        self.expanded.clear();
        self.error = None;
    }

    #[must_use]
    pub fn change_count(&self) -> usize {
        env_change_count(&self.original.env, &self.pending.env)
            + self
                .original
                .roles
                .keys()
                .chain(self.pending.roles.keys())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(|role| {
                    let empty = BTreeMap::new();
                    let original = self.original.roles.get(role).unwrap_or(&empty);
                    let pending = self.pending.roles.get(role).unwrap_or(&empty);
                    env_change_count(original, pending)
                })
                .sum::<usize>()
    }
}

impl SettingsAuthState {
    pub fn from_config(config: &AppConfig) -> Self {
        let pending = [
            crate::console::manager::auth_kind::AuthKind::Claude,
            crate::console::manager::auth_kind::AuthKind::Codex,
            crate::console::manager::auth_kind::AuthKind::Amp,
            crate::console::manager::auth_kind::AuthKind::Kimi,
            crate::console::manager::auth_kind::AuthKind::Opencode,
            crate::console::manager::auth_kind::AuthKind::Github,
        ]
        .into_iter()
        .map(|kind| SettingsAuthRow {
            kind,
            mode: match kind {
                crate::console::manager::auth_kind::AuthKind::Claude
                | crate::console::manager::auth_kind::AuthKind::Codex
                | crate::console::manager::auth_kind::AuthKind::Amp
                | crate::console::manager::auth_kind::AuthKind::Kimi
                | crate::console::manager::auth_kind::AuthKind::Opencode => kind.agent().map_or(
                    crate::console::manager::auth_kind::AuthMode::Sync,
                    |agent| {
                        crate::console::manager::auth_kind::AuthMode::from_auth_forward(
                            crate::config::resolve_mode(config, agent, "", ""),
                        )
                    },
                ),
                crate::console::manager::auth_kind::AuthKind::Github => {
                    crate::console::manager::auth_kind::AuthMode::from_github(
                        crate::config::resolve_github_mode(config, "", ""),
                    )
                }
            },
        })
        .collect::<Vec<_>>();
        Self {
            selected: 0,
            selected_kind: None,
            original: pending.clone(),
            pending,
            github_env: config
                .github
                .as_ref()
                .map(|github| github.env.clone())
                .unwrap_or_default(),
            original_github_env: config
                .github
                .as_ref()
                .map(|github| github.env.clone())
                .unwrap_or_default(),
            modal: None,
            pending_auth_form_return: None,
            error: None,
            scroll_y: 0,
            scroll_focused: false,
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.pending != self.original || self.github_env != self.original_github_env
    }

    pub fn discard(&mut self) {
        self.pending = self.original.clone();
        self.github_env = self.original_github_env.clone();
        self.selected_kind = None;
        self.selected = self.selected.min(self.pending.len().saturating_sub(1));
        self.modal = None;
        self.pending_auth_form_return = None;
        self.error = None;
    }
}

impl SettingsTrustState {
    pub fn from_config(config: &AppConfig) -> Self {
        let pending = config
            .roles
            .iter()
            .map(|(role, source)| SettingsTrustRow {
                role: role.clone(),
                git: source.git.clone(),
                trusted: source.trusted,
            })
            .collect::<Vec<_>>();
        Self {
            selected: 0,
            original: pending.clone(),
            pending,
            error: None,
            scroll_x: 0,
            scroll_y: 0,
            scroll_focused: false,
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.pending != self.original
    }

    pub fn discard(&mut self) {
        self.pending = self.original.clone();
        self.selected = self.selected.min(self.pending.len().saturating_sub(1));
        self.error = None;
    }
}

fn validate_settings_env(
    env: &SettingsEnvConfig,
    roles: &[SettingsTrustRow],
) -> anyhow::Result<()> {
    let registered: std::collections::BTreeSet<&str> =
        roles.iter().map(|r| r.role.as_str()).collect();
    validate_settings_env_keys("global", env.env.keys())?;
    for (role, role_env) in &env.roles {
        if !registered.contains(role.as_str()) {
            anyhow::bail!("role {role:?} is not registered");
        }
        validate_settings_env_keys(role, role_env.keys())?;
    }
    Ok(())
}

fn validate_settings_env_keys<'a>(
    scope: &str,
    keys: impl Iterator<Item = &'a String>,
) -> anyhow::Result<()> {
    for key in keys {
        if key.trim().is_empty() {
            anyhow::bail!("env var key cannot be empty");
        }
        if crate::env_model::is_reserved(key) {
            anyhow::bail!(
                "env name {key:?} in {scope} is reserved by the jackin runtime and cannot be set"
            );
        }
    }
    Ok(())
}

fn settings_env_flat_row_count(env: &SettingsEnvState<'_>) -> usize {
    let mut rows = env.pending.env.len();
    if !env.pending.env.is_empty() {
        rows += 1;
    }
    rows += 1;
    for (role, role_env) in &env.pending.roles {
        if role_env.is_empty() {
            continue;
        }
        rows += 2;
        if env.expanded.contains(role) {
            rows += role_env.len() + 2;
        }
    }
    rows
}

#[derive(Debug, Clone)]
pub struct PendingSaveCommit {
    pub effective_removals: Vec<String>,
    pub final_mounts: Option<Vec<crate::workspace::MountConfig>>,
    /// `true` when the operator has already confirmed the source-drift
    /// modal in this save cycle (Task 10.3). Causes
    /// `commit_editor_save` to skip the drift-detection check and go
    /// straight to `force_cleanup_isolated` + the on-disk write.
    /// Defaults to `false` so the first commit attempt always runs the
    /// safety check.
    pub delete_isolated_acknowledged: bool,
}

#[derive(Debug, Clone)]
pub enum EditorMode {
    Edit { name: String },
    Create,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    General,
    Mounts,
    Roles,
    Secrets,
    /// Auth panel: opens on a kind-first picker (`Claude Code` /
    /// `Codex`); selecting a kind drops into a focused view with the
    /// workspace-level mode + optional credential source plus
    /// per-role overrides for that kind only. The form modal lives in
    /// `auth_panel`; the row enumeration is `auth_flat_rows` in
    /// `render::editor`.
    Auth,
}

#[derive(Debug, Clone, Copy)]
pub enum FieldFocus {
    Row(usize),
}

// `TextInputState` is ~600B while other variants are ~330B. Boxing the state
// field would cascade through 19 construction/match sites (including wizard
// step transitions that move state in and out of `Modal`). The ergonomic cost
// is worse than the small stack-size win here, so we accept the variance.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Modal<'a> {
    TextInput {
        target: TextInputTarget,
        state: TextInputState<'a>,
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
        state: crate::console::widgets::save_discard::SaveDiscardState,
    },
    /// Workspace list, when ≥2 GitHub mounts and operator pressed `o`.
    GithubPicker {
        state: GithubPickerState,
    },
    ConfirmSave {
        state: ConfirmSaveState,
    },
    ErrorPopup {
        state: ErrorPopupState,
    },
    /// Boxed because the picker's `Vec`s + runner + channel are
    /// substantially larger than other variants.
    OpPicker {
        state: Box<OpPickerState>,
    },
    /// Manager-list disambiguation picker (`ManagerState.list_modal`
    /// slot, same as `GithubPicker`).
    RolePicker {
        state: RolePickerState,
    },
    /// Editor-tab override picker (`EditorState.modal` slot, not the
    /// launch-disambiguation slot on `ManagerState`) so the editor's
    /// commit handler can create the override entry and auto-expand.
    RoleOverridePicker {
        state: RolePickerState,
    },
    /// Auth-tab role picker — opened from the `+ Override for a role`
    /// sentinel when an auth kind is already focused. Commit reads
    /// `editor.auth_selected_kind` to build the `AuthFormTarget`
    /// directly, then hands off to `Modal::AuthForm`.
    AuthRolePicker {
        state: RolePickerState,
    },
    SourcePicker {
        state: SourcePickerState,
    },
    AuthSourcePicker {
        state: SourcePickerState,
    },
    ScopePicker {
        state: ScopePickerState,
    },
    /// Auth-form modal opened from the Auth tab. `target` identifies
    /// which scope (workspace or workspace × role) and which auth
    /// kind (Claude / Codex / Github) the form is editing so commit
    /// can write back to the correct slot on `editor.pending`.
    AuthForm {
        target: AuthFormTarget,
        state: Box<AuthForm>,
        /// Active focus inside the form: mode picker, credential
        /// source row, or one of the Save/Cancel/Reset buttons.
        focus: AuthFormFocus,
        /// Buffer used to round-trip a previously-typed literal
        /// credential through the source-picker → text-input detour
        /// so the value isn't lost on cancel and the text-input
        /// modal can re-open pre-populated.
        literal_buffer: String,
    },
}

/// Where in the auth-edit form the cursor currently sits.
///
/// The credential value is collected through
/// `Modal::AuthSourcePicker` → `Modal::TextInput` (literal) or
/// `Modal::OpPicker` (1Password), so the form carries only one
/// credential-related focus (`CredentialSource`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFormFocus {
    /// Mode picker line — Space cycles modes; Tab/Down advances focus.
    Mode,
    /// Required credential row — Enter opens the shared source picker.
    CredentialSource,
    /// Save action button.
    Save,
    /// Cancel action button.
    Cancel,
    /// Reset action button — clears the layer's mode/credential.
    Reset,
}

/// Identifies the (scope, kind) pair an open `AuthForm` modal is editing.
///
/// Committing the form writes back into the matching slot on
/// `editor.pending`:
///
///   - workspace `claude` / `codex` / `amp` / `github` field, or
///   - workspace-role override `claude` / `codex` / `amp` / `github` field,
///
/// plus the credential env var when the chosen mode requires one.
///
/// Widened from `Agent` to [`AuthKind`] so the GitHub kind (which has
/// no agent dimension because `.config/gh/` is shared by every agent
/// in the container) can target the same modal flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthFormTarget {
    /// `[workspaces.<ws>.<kind>].auth_forward` slot, with the credential
    /// env var landing in `[workspaces.<ws>.env]` (Claude / Codex) or
    /// `[workspaces.<ws>.github.env]` (Github).
    Workspace {
        kind: crate::console::manager::auth_kind::AuthKind,
    },
    /// `[workspaces.<ws>.roles.<role>.<kind>].auth_forward` slot, with
    /// the credential env var landing in
    /// `[workspaces.<ws>.roles.<role>.env]` (Claude / Codex) or
    /// `[workspaces.<ws>.roles.<role>.github.env]` (Github).
    WorkspaceRole {
        role: String,
        kind: crate::console::manager::auth_kind::AuthKind,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileBrowserTarget {
    CreateFirstMountSrc,
    EditAddMountSrc,
}

#[derive(Debug, Clone)]
pub enum ConfirmTarget {
    DeleteEnvVar {
        scope: SecretsScopeTag,
        key: String,
    },
    TrustRoleSource {
        key: String,
        source: crate::config::RoleSource,
    },
    /// Source-drift confirmation (Task 10.3): operator's edit changes the
    /// `src` of one or more mounts that have preserved isolated state on
    /// stopped containers. Carries the planner's pending save material so
    /// the commit pass can run `force_cleanup_isolated` for each affected
    /// container then write the edit through.
    DeleteIsolatedAndSave {
        plan: PendingSaveCommit,
        exit_on_success: bool,
        affected_containers: Vec<String>,
    },
}

/// Separate from [`crate::config::editor::EnvScope`].
///
/// That type needs the workspace name, which Create mode hasn't
/// captured until `pending_name` lands at save time. The full
/// `EnvScope` is derived in `commit_editor_save`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecretsScopeTag {
    Workspace,
    Role(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitIntent {
    Save,
    Discard,
}

#[derive(Debug)]
pub struct CreatePreludeState<'a> {
    pub step: CreateStep,
    pub pending_mount_src: Option<PathBuf>,
    pub pending_mount_dst: Option<String>,
    pub pending_readonly: bool,
    pub pending_workdir: Option<String>,
    pub pending_name: Option<String>,
    pub modal: Option<Modal<'a>>,
    /// Captured so Esc on `MountDstChoice` re-opens `FileBrowser` at
    /// the same directory instead of `$HOME`.
    pub last_browser_cwd: Option<PathBuf>,
    /// Picks Esc-on-`WorkdirPick` rewind target: `TextInputDst` when
    /// the Edit-destination branch was used, else `MountDstChoice`.
    pub used_edit_dst: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateStep {
    PickFirstMountSrc,
    PickFirstMountDst,
    PickWorkdir,
    NameWorkspace,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub shown_at: std::time::Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
}

// ── Impls ──────────────────────────────────────────────────────────

impl WorkspaceSummary {
    pub fn from_config(name: &str, ws: &WorkspaceConfig) -> Self {
        Self {
            name: name.to_string(),
            workdir: ws.workdir.clone(),
            mount_count: ws.mounts.len(),
            readonly_mount_count: ws.mounts.iter().filter(|m| m.readonly).count(),
            allowed_role_count: ws.allowed_roles.len(),
            default_role: ws.default_role.clone(),
            last_role: ws.last_role.clone(),
        }
    }
}

impl ManagerState<'_> {
    pub(in crate::console::manager) const fn list_scroll_x_mut(
        &mut self,
        focus: MountScrollFocus,
    ) -> &mut u16 {
        match focus {
            MountScrollFocus::Workspace => &mut self.list_mounts_scroll_x,
            MountScrollFocus::Global => &mut self.list_global_mounts_scroll_x,
            MountScrollFocus::RoleGlobal => &mut self.list_role_global_mounts_scroll_x,
            MountScrollFocus::Roles => &mut self.list_roles_scroll_x,
        }
    }

    pub(in crate::console::manager) const fn list_scroll_y_mut(
        &mut self,
        focus: MountScrollFocus,
    ) -> &mut u16 {
        match focus {
            MountScrollFocus::Workspace => &mut self.list_mounts_scroll_y,
            MountScrollFocus::Global => &mut self.list_global_mounts_scroll_y,
            MountScrollFocus::RoleGlobal => &mut self.list_role_global_mounts_scroll_y,
            MountScrollFocus::Roles => &mut self.list_roles_scroll_y,
        }
    }

    pub(in crate::console::manager) const fn reset_list_scroll(&mut self) {
        self.list_mounts_scroll_x = 0;
        self.list_mounts_scroll_y = 0;
        self.list_global_mounts_scroll_x = 0;
        self.list_global_mounts_scroll_y = 0;
        self.list_role_global_mounts_scroll_x = 0;
        self.list_role_global_mounts_scroll_y = 0;
        self.list_roles_scroll_x = 0;
        self.list_roles_scroll_y = 0;
        self.list_scroll_focus = None;
        self.list_names_scroll_x = 0;
    }

    /// Allocates a fresh empty cache and assumes `op` unavailable —
    /// production reset paths use the `_with_cache_and_op` variant to
    /// preserve the `ConsoleState`-owned cache.
    pub fn from_config(config: &AppConfig, cwd: &std::path::Path) -> Self {
        Self::from_config_with_cache(config, cwd, Rc::new(RefCell::new(OpCache::default())))
    }

    pub fn from_config_with_cache(
        config: &AppConfig,
        cwd: &std::path::Path,
        op_cache: Rc<RefCell<OpCache>>,
    ) -> Self {
        Self::from_config_with_cache_and_op(config, cwd, op_cache, false)
    }

    pub fn from_config_with_cache_and_op(
        config: &AppConfig,
        cwd: &std::path::Path,
        op_cache: Rc<RefCell<OpCache>>,
        op_available: bool,
    ) -> Self {
        let workspaces: Vec<WorkspaceSummary> = config
            .workspaces
            .iter()
            .map(|(name, ws)| WorkspaceSummary::from_config(name, ws))
            .collect();

        let saved_count = workspaces.len();
        let matching_saved = crate::app::context::find_saved_workspace_for_cwd(config, cwd)
            .and_then(|(name, _)| workspaces.iter().position(|w| w.name == name));
        let selected_row = matching_saved.map_or(
            ManagerListRow::CurrentDirectory,
            ManagerListRow::SavedWorkspace,
        );
        let selected = selected_row.to_screen_index(saved_count);

        Self {
            stage: ManagerStage::List,
            workspaces,
            instances: Vec::new(),
            current_dir: cwd.display().to_string(),
            selected,
            toast: None,
            list_modal: None,
            inline_role_picker: None,
            inline_agent_picker: None,
            list_mounts_scroll_x: 0,
            list_mounts_scroll_y: 0,
            list_global_mounts_scroll_x: 0,
            list_global_mounts_scroll_y: 0,
            list_role_global_mounts_scroll_x: 0,
            list_role_global_mounts_scroll_y: 0,
            list_roles_scroll_x: 0,
            list_roles_scroll_y: 0,
            list_scroll_focus: None,
            list_names_scroll_x: 0,
            list_names_focused: false,
            list_split_pct: DEFAULT_SPLIT_PCT,
            drag_state: None,
            op_cache,
            op_available,
            cached_term_size: Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            },
            instances_last_refresh: None,
            instances_last_error: None,
        }
    }

    /// Total number of rows in the list (current-dir + saved + sentinel).
    #[must_use]
    pub const fn row_count(&self) -> usize {
        self.workspaces.len() + 2
    }

    /// Index of the "+ New workspace" sentinel row.
    #[must_use]
    pub const fn new_workspace_row_index(&self) -> usize {
        self.workspaces.len() + 1
    }

    /// Decode a raw screen-row `usize` into a [`ManagerListRow`]. Returns
    /// `None` when `idx` is out of range.
    #[must_use]
    pub const fn row_at(&self, idx: usize) -> Option<ManagerListRow> {
        let saved_count = self.workspaces.len();
        if idx == 0 {
            Some(ManagerListRow::CurrentDirectory)
        } else if idx == saved_count + 1 {
            Some(ManagerListRow::NewWorkspace)
        } else if idx <= saved_count {
            Some(ManagerListRow::SavedWorkspace(idx - 1))
        } else {
            None
        }
    }

    /// Decode a visual list row into a logical row. The rendered list keeps a
    /// blank spacer before "+ New workspace" when saved workspaces exist; that
    /// spacer is intentionally not selectable.
    #[must_use]
    pub const fn row_at_visual_index(&self, idx: usize) -> Option<ManagerListRow> {
        let saved_count = self.workspaces.len();
        if idx == 0 {
            Some(ManagerListRow::CurrentDirectory)
        } else if idx <= saved_count {
            Some(ManagerListRow::SavedWorkspace(idx - 1))
        } else if idx == saved_count + 1 && saved_count > 0 {
            None
        } else if (saved_count > 0 && idx == saved_count + 2)
            || (saved_count == 0 && idx == saved_count + 1)
        {
            Some(ManagerListRow::NewWorkspace)
        } else {
            None
        }
    }

    /// Selected index in rendered-list coordinates. Differs from `selected`
    /// only when the blank spacer before "+ New workspace" is present.
    #[must_use]
    pub fn visual_selected(&self) -> usize {
        self.selected_row().to_visual_index(self.workspaces.len())
    }

    /// What the operator currently has highlighted.
    #[must_use]
    pub fn selected_row(&self) -> ManagerListRow {
        self.row_at(self.selected)
            .unwrap_or(ManagerListRow::CurrentDirectory)
    }

    /// Convenience: `true` when the selection is on the synthetic
    /// "Current directory" row.
    #[must_use]
    pub fn is_current_dir_selected(&self) -> bool {
        matches!(self.selected_row(), ManagerListRow::CurrentDirectory)
    }

    /// Convenience: `true` when the selection is on the "+ New workspace"
    /// sentinel.
    #[must_use]
    pub fn is_new_workspace_selected(&self) -> bool {
        matches!(self.selected_row(), ManagerListRow::NewWorkspace)
    }

    /// The [`WorkspaceSummary`] currently highlighted, or `None` when the
    /// selection is on Current Directory or New Workspace.
    #[must_use]
    pub fn selected_workspace_summary(&self) -> Option<&WorkspaceSummary> {
        if let ManagerListRow::SavedWorkspace(i) = self.selected_row() {
            self.workspaces.get(i)
        } else {
            None
        }
    }

    pub fn refresh_instances(&mut self, paths: &crate::paths::JackinPaths) {
        const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
        let now = std::time::Instant::now();
        if let Some(last) = self.instances_last_refresh
            && now.duration_since(last) < REFRESH_INTERVAL
        {
            return;
        }
        self.instances_last_refresh = Some(now);
        match crate::instance::InstanceIndex::read_or_rebuild(&paths.data_dir) {
            Ok(index) => {
                self.instances = index.instances;
                self.instances_last_error = None;
            }
            Err(error) => {
                // Empty list would look identical to "no instances",
                // hiding corrupt index / permission errors. Surface
                // via toast so the operator has a path to investigate.
                self.instances.clear();
                let message = format!("instance index error: {error}");
                if self.instances_last_error.as_deref() != Some(&message) {
                    self.toast = Some(Toast {
                        message: message.clone(),
                        kind: ToastKind::Error,
                        shown_at: std::time::Instant::now(),
                    });
                    self.instances_last_error = Some(message);
                }
            }
        }
    }

    /// Test helper: force the next `refresh_instances` call to hit disk
    /// regardless of the throttle interval.
    #[cfg(test)]
    pub const fn force_refresh_instances_for_test(&mut self) {
        self.instances_last_refresh = None;
    }

    /// Drained from the outer event loop every tick so picker results
    /// land without keystroke pumping. Idempotent on empty channels.
    /// Covers both modal anchors (`list_modal` and `editor.modal`).
    pub fn poll_picker_loads(&mut self) {
        if let Some(Modal::OpPicker { state }) = self.list_modal.as_mut() {
            state.poll_load();
        }
        if let ManagerStage::Editor(editor) = &mut self.stage
            && let Some(Modal::OpPicker { state }) = editor.modal.as_mut()
        {
            state.poll_load();
        }
        if let ManagerStage::Settings(settings) = &mut self.stage
            && let Some(SettingsEnvModal::OpPicker { state }) = settings.env.modal.as_mut()
        {
            state.poll_load();
        }
        if let ManagerStage::Settings(settings) = &mut self.stage
            && let Some(SettingsAuthModal::OpPicker { state }) = settings.auth.modal.as_mut()
        {
            state.poll_load();
        }
    }
}

impl EditorState<'_> {
    pub fn new_edit(name: String, ws: WorkspaceConfig) -> Self {
        Self {
            mode: EditorMode::Edit { name },
            active_tab: EditorTab::General,
            tab_bar_focused: true,
            active_field: FieldFocus::Row(0),
            original: ws.clone(),
            pending: ws,
            modal: None,
            pending_name: None,
            exit_after_save: None,
            save_flow: EditorSaveFlow::Idle,
            unmasked_rows: BTreeSet::default(),
            secrets_expanded: BTreeSet::default(),
            auth_expanded: BTreeSet::default(),
            auth_selected_kind: None,
            pending_env_key: None,
            pending_picker_target: None,
            pending_picker_value: None,
            pending_auth_form_return: None,
            workspace_mounts_scroll_x: 0,
            workspace_mounts_scroll_focused: false,
            tab_scroll_x: 0,
            tab_scroll_y: 0,
            tab_content_scroll_focused: false,
            tab_content_width: 0,
            tab_content_height: 0,
        }
    }

    pub fn new_create() -> Self {
        let empty = WorkspaceConfig::default();
        Self {
            mode: EditorMode::Create,
            active_tab: EditorTab::General,
            tab_bar_focused: true,
            active_field: FieldFocus::Row(0),
            original: empty.clone(),
            pending: empty,
            modal: None,
            pending_name: None,
            exit_after_save: None,
            save_flow: EditorSaveFlow::Idle,
            unmasked_rows: BTreeSet::default(),
            secrets_expanded: BTreeSet::default(),
            auth_expanded: BTreeSet::default(),
            auth_selected_kind: None,
            pending_env_key: None,
            pending_picker_target: None,
            pending_picker_value: None,
            pending_auth_form_return: None,
            workspace_mounts_scroll_x: 0,
            workspace_mounts_scroll_focused: false,
            tab_scroll_x: 0,
            tab_scroll_y: 0,
            tab_content_scroll_focused: false,
            tab_content_width: 0,
            tab_content_height: 0,
        }
    }

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

    /// Field-level diff count used for "s save (N changes)".
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
        // The Github kind is single-entry at the workspace and role
        // layers (no agent dimension). A whole-block diff lights up
        // the save counter for both `auth_forward` flips and
        // `[github.env]` mutations like setting `GH_TOKEN`.
        if self.pending.github != self.original.github {
            n += 1;
        }
        // Rename in Edit mode counts as a change.
        if let EditorMode::Edit { name } = &self.mode
            && self.pending_name.as_deref().is_some_and(|pn| pn != name)
        {
            n += 1;
        }
        n += classify_mount_diffs(&self.original.mounts, &self.pending.mounts)
            .iter()
            .filter(|d| !matches!(d, MountDiff::Unchanged(_)))
            .count();
        n += env_change_count(&self.original.env, &self.pending.env);
        // Per-role overrides: union the keys; an role present on
        // only one side counts its whole env map / claude / codex /
        // github block as added/removed.
        let agent_keys: std::collections::BTreeSet<&String> = self
            .original
            .roles
            .keys()
            .chain(self.pending.roles.keys())
            .collect();
        for role in agent_keys {
            let orig = self.original.roles.get(role);
            let pend = self.pending.roles.get(role);
            let empty = std::collections::BTreeMap::<String, crate::operator_env::EnvValue>::new();
            let orig_env = orig.map_or(&empty, |o| &o.env);
            let pend_env = pend.map_or(&empty, |p| &p.env);
            n += env_change_count(orig_env, pend_env);
            // Per-role auth-forward overrides count as one change
            // each so a role × github mode flip with no env edit
            // still wakes the save button.
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

    /// Cycle the per-mount isolation strategy on the highlighted mount row.
    /// Sequence: `Shared → Worktree → Clone → Shared`. Silent no-op when the cursor
    /// is on the `+ Add mount` sentinel (i.e. past the last data row).
    pub fn cycle_isolation_for_selected_mount(&mut self) {
        use crate::isolation::MountIsolation::{Clone, Shared, Worktree};
        let FieldFocus::Row(n) = self.active_field;
        if let Some(m) = self.pending.mounts.get_mut(n) {
            m.isolation = match m.isolation {
                Shared => Worktree,
                Worktree => Clone,
                Clone => Shared,
            };
        }
    }
}

/// Per-mount classification used by both `change_count` and the
/// Confirm Save mount-diff summary.
///
/// Same-`dst` matches with structural drift are reported as a single
/// `Modified`, not as `Removed + Added` — operators perceive an
/// isolation/readonly flip on an existing mount as one logical change,
/// not two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountDiff<'a> {
    Unchanged(&'a crate::workspace::MountConfig),
    Added(&'a crate::workspace::MountConfig),
    Removed(&'a crate::workspace::MountConfig),
    Modified {
        original: &'a crate::workspace::MountConfig,
        pending: &'a crate::workspace::MountConfig,
    },
}

/// Classify the mount-set delta. `dst` is the identity key (matches the
/// upsert/remove semantics used everywhere else). `Unchanged` rows are
/// returned too so callers can render them or filter as needed.
pub fn classify_mount_diffs<'a>(
    original: &'a [crate::workspace::MountConfig],
    pending: &'a [crate::workspace::MountConfig],
) -> Vec<MountDiff<'a>> {
    let mut out = Vec::with_capacity(original.len() + pending.len());
    for p in pending {
        match original.iter().find(|o| o.dst == p.dst) {
            Some(o) if o == p => out.push(MountDiff::Unchanged(p)),
            Some(o) => out.push(MountDiff::Modified {
                original: o,
                pending: p,
            }),
            None => out.push(MountDiff::Added(p)),
        }
    }
    for o in original {
        if !pending.iter().any(|p| p.dst == o.dst) {
            out.push(MountDiff::Removed(o));
        }
    }
    out
}

fn env_change_count(
    original: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
    pending: &std::collections::BTreeMap<String, crate::operator_env::EnvValue>,
) -> usize {
    let mut n = 0;
    for (k, v) in pending {
        match original.get(k) {
            None => n += 1,                // added
            Some(ov) if ov != v => n += 1, // changed
            _ => {}
        }
    }
    for k in original.keys() {
        if !pending.contains_key(k) {
            n += 1; // removed
        }
    }
    n
}

impl Default for CreatePreludeState<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl CreatePreludeState<'_> {
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
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{KeepAwakeConfig, MountConfig, WorkspaceConfig};

    fn empty_ws(workdir: &str) -> WorkspaceConfig {
        WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: workdir.into(),
            ..Default::default()
        }
    }

    #[test]
    fn summary_counts_mounts_and_readonly() {
        let ws = WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: "/a".into(),
            mounts: vec![
                MountConfig {
                    src: "/s1".into(),
                    dst: "/a".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
                MountConfig {
                    src: "/s2".into(),
                    dst: "/b".into(),
                    readonly: true,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            ],
            allowed_roles: vec!["agent-smith".into()],
            ..Default::default()
        };
        let sum = WorkspaceSummary::from_config("big-monorepo", &ws);
        assert_eq!(sum.name, "big-monorepo");
        assert_eq!(sum.mount_count, 2);
        assert_eq!(sum.readonly_mount_count, 1);
        assert_eq!(sum.allowed_role_count, 1);
    }

    #[test]
    fn manager_from_config_lists_all_workspaces() {
        let mut config = AppConfig::default();
        config.workspaces.insert("a".into(), empty_ws("/a"));
        // cwd is unrelated to /a — landing row is the synthetic
        // "Current directory" at index 0.
        let tmp = tempfile::tempdir().unwrap();
        let state = ManagerState::from_config(&config, tmp.path());
        assert_eq!(state.workspaces.len(), 1);
        assert!(matches!(state.stage, ManagerStage::List));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn refresh_instances_loads_rebuildable_index() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(tmp.path());
        let mut manifest =
            crate::instance::InstanceManifest::new(crate::instance::NewInstanceManifest {
                container_base: "jk-k7p9m2xq-demo-alpha",
                workspace_name: Some("demo"),
                workspace_label: "demo",
                workdir: "/workspace/demo",
                host_workdir_fingerprint: "sha256:test",
                role_key: "alpha",
                role_display_name: "Alpha",
                agent_runtime: crate::agent::Agent::Claude,
                role_source_git: "https://example.invalid/alpha.git",
                role_source_ref: None,
                image_tag: "jk_alpha",
                docker: crate::instance::DockerResources {
                    role_container: "jk-k7p9m2xq-demo-alpha".into(),
                    dind_container: "jk-k7p9m2xq-demo-alpha-dind".into(),
                    network: "jk-k7p9m2xq-demo-alpha-net".into(),
                    certs_volume: "jk-k7p9m2xq-demo-alpha-dind-certs".into(),
                },
            });
        manifest.mark_status(crate::instance::InstanceStatus::RestoreAvailable);
        manifest
            .write(&paths.data_dir.join("jk-k7p9m2xq-demo-alpha"))
            .unwrap();

        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.refresh_instances(&paths);

        assert_eq!(state.instances.len(), 1);
        assert_eq!(state.instances[0].instance_id, "k7p9m2xq");
        assert_eq!(
            state.instances[0].status,
            crate::instance::InstanceStatus::RestoreAvailable
        );
    }

    #[test]
    fn refresh_instances_throttles_within_interval() {
        // 20 Hz render loop must not reparse instances.json on every
        // tick. After the first refresh, a follow-up call inside the
        // throttle window keeps the cached `instances` snapshot even
        // when the on-disk index changes; `force_refresh_instances_for_test`
        // bypasses the gate.
        let tmp = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(tmp.path());
        let mut manifest =
            crate::instance::InstanceManifest::new(crate::instance::NewInstanceManifest {
                container_base: "jk-k7p9m2xq-demo-alpha",
                workspace_name: Some("demo"),
                workspace_label: "demo",
                workdir: "/workspace/demo",
                host_workdir_fingerprint: "sha256:test",
                role_key: "alpha",
                role_display_name: "Alpha",
                agent_runtime: crate::agent::Agent::Claude,
                role_source_git: "https://example.invalid/alpha.git",
                role_source_ref: None,
                image_tag: "jk_alpha",
                docker: crate::instance::DockerResources {
                    role_container: "jk-k7p9m2xq-demo-alpha".into(),
                    dind_container: "jk-k7p9m2xq-demo-alpha-dind".into(),
                    network: "jk-k7p9m2xq-demo-alpha-net".into(),
                    certs_volume: "jk-k7p9m2xq-demo-alpha-dind-certs".into(),
                },
            });
        manifest.mark_status(crate::instance::InstanceStatus::Active);
        manifest
            .write(&paths.data_dir.join("jk-k7p9m2xq-demo-alpha"))
            .unwrap();

        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.refresh_instances(&paths);
        assert_eq!(state.instances.len(), 1);
        assert_eq!(
            state.instances[0].status,
            crate::instance::InstanceStatus::Active
        );

        // Mutate the manifest on disk; without the bypass, an
        // immediate refresh must observe the cached value.
        manifest.mark_status(crate::instance::InstanceStatus::Crashed);
        manifest
            .write(&paths.data_dir.join("jackin-demo-alpha-k7p9m2xq"))
            .unwrap();
        crate::instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();

        state.refresh_instances(&paths);
        assert_eq!(
            state.instances[0].status,
            crate::instance::InstanceStatus::Active,
            "throttle window must keep the cached snapshot",
        );

        // Bypass the throttle — disk state is now observable.
        state.force_refresh_instances_for_test();
        state.refresh_instances(&paths);
        assert_eq!(
            state.instances[0].status,
            crate::instance::InstanceStatus::Crashed,
        );
    }

    #[test]
    fn refresh_instances_surfaces_index_error_via_toast() {
        // Corrupt the index file; the read path must surface the parse
        // error as a toast and dedup so subsequent identical errors
        // don't pin a new toast every refresh tick.
        let tmp = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(tmp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::write(paths.data_dir.join("instances.json"), b"not json").unwrap();
        // Also drop a directory that looks like a state dir so
        // `rebuild()` doesn't silently regenerate a fresh empty index.
        let bogus = paths.data_dir.join("jackin-bogus-k7p9m2xq");
        std::fs::create_dir_all(bogus.join(".jackin")).unwrap();
        std::fs::write(bogus.join(".jackin/instance.json"), b"not json").unwrap();

        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.refresh_instances(&paths);

        assert!(state.instances.is_empty());
        let toast = state.toast.as_ref().expect("error toast must be emitted");
        assert_eq!(toast.kind, ToastKind::Error);
        assert!(
            toast.message.contains("instance index error"),
            "toast message: {}",
            toast.message
        );

        let first_shown_at = toast.shown_at;
        // Second refresh: stash + dedup must not overwrite the first
        // toast when the error message is unchanged.
        state.force_refresh_instances_for_test();
        state.refresh_instances(&paths);
        let toast2 = state.toast.as_ref().unwrap();
        assert_eq!(
            toast2.shown_at, first_shown_at,
            "identical error must not re-emit the toast",
        );
    }

    #[test]
    fn manager_preselects_saved_workspace_matching_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().canonicalize().unwrap();
        let workdir = project.display().to_string();

        let mut config = AppConfig::default();
        config.workspaces.insert(
            "big-monorepo".into(),
            WorkspaceConfig {
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
                workdir: workdir.clone(),
                mounts: vec![MountConfig {
                    src: workdir.clone(),
                    dst: workdir,
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        );
        // Second workspace that does NOT match cwd — used to verify the
        // preselect calculation points at the matching one, not simply
        // "index 1" which works for a single workspace by accident.
        config
            .workspaces
            .insert("z-unrelated".into(), empty_ws("/some/other/path"));

        let state = ManagerState::from_config(&config, &project);
        // Workspaces are ordered by BTreeMap key: ["big-monorepo", "z-unrelated"].
        // "big-monorepo" is at saved_index 0, so selected = 1 + 0 = 1.
        assert_eq!(state.selected, 1);
        assert_eq!(state.workspaces[state.selected - 1].name, "big-monorepo");
    }

    /// Pins that `ms.selected == 0` means "Current directory" regardless
    /// of how many saved workspaces are present. The render path
    /// (`render_list_body`) and the input path (`handle_list_key`) both
    /// depend on this: selected==0 is the synthetic cwd row, 1..=N are
    /// saved workspaces, N+1 is the "+ New workspace" sentinel.
    #[test]
    fn manager_current_directory_is_first_row() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().canonicalize().unwrap();

        // Empty config: only the synthetic "Current directory" + sentinel.
        let config_empty = AppConfig::default();
        let state_empty = ManagerState::from_config(&config_empty, &cwd);
        assert_eq!(state_empty.selected, 0);
        assert_eq!(state_empty.workspaces.len(), 0);

        // Non-empty config with unrelated saved workspaces — preselect
        // still lands on row 0.
        let mut config = AppConfig::default();
        config
            .workspaces
            .insert("a".into(), empty_ws("/some/other/path"));
        config
            .workspaces
            .insert("b".into(), empty_ws("/yet/another"));
        let state = ManagerState::from_config(&config, &cwd);
        assert_eq!(
            state.selected, 0,
            "selected==0 must always map to Current directory"
        );
        assert_eq!(state.workspaces.len(), 2);
    }

    #[test]
    fn manager_preselects_current_directory_when_no_saved_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path().canonicalize().unwrap();

        let mut config = AppConfig::default();
        config
            .workspaces
            .insert("unrelated".into(), empty_ws("/some/other/path"));

        let state = ManagerState::from_config(&config, &cwd);
        assert_eq!(
            state.selected, 0,
            "no saved workspace covers cwd → land on Current directory"
        );
    }

    #[test]
    fn new_edit_is_not_dirty() {
        let e = EditorState::new_edit("a".into(), empty_ws("/a"));
        assert!(!e.is_dirty());
        assert_eq!(e.change_count(), 0);
    }

    #[test]
    fn changing_workdir_is_dirty_count_one() {
        let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
        e.pending.workdir = "/b".into();
        assert!(e.is_dirty());
        assert_eq!(e.change_count(), 1);
    }

    #[test]
    fn adding_mount_counts_as_one_change() {
        let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
        e.pending.mounts.push(MountConfig {
            src: "/s".into(),
            dst: "/a".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        });
        assert_eq!(e.change_count(), 1);
    }

    /// Regression: cycling isolation on an existing mount (same `dst`,
    /// same `src`) is one logical change. Pre-fix it counted as 2
    /// because the structural-equality classifier treated the new
    /// `MountConfig` as added and the old one as removed.
    #[test]
    fn isolation_only_change_counts_as_one() {
        let mut ws = empty_ws("/workspace/jackin");
        ws.mounts.push(MountConfig {
            src: "/host/jackin".into(),
            dst: "/workspace/jackin".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        });
        let mut e = EditorState::new_edit("jackin".into(), ws);
        assert_eq!(e.change_count(), 0);
        // Cycle from Shared to Worktree on the only mount row.
        e.active_field = FieldFocus::Row(0);
        e.cycle_isolation_for_selected_mount();
        assert_eq!(e.change_count(), 1);
    }

    #[test]
    fn classify_mount_diffs_distinguishes_modified_from_remove_add() {
        let original = vec![MountConfig {
            src: "/host/jackin".into(),
            dst: "/workspace/jackin".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }];
        let mut pending = original.clone();
        pending[0].isolation = crate::isolation::MountIsolation::Worktree;

        let diffs = classify_mount_diffs(&original, &pending);
        assert_eq!(diffs.len(), 1, "same-dst diff is one row, not two");
        assert!(
            matches!(diffs[0], MountDiff::Modified { .. }),
            "got {:?}",
            diffs[0]
        );
    }

    #[test]
    fn classify_mount_diffs_keeps_genuine_remove_add_separate() {
        let original = vec![MountConfig {
            src: "/host/a".into(),
            dst: "/workspace/a".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }];
        let pending = vec![MountConfig {
            src: "/host/b".into(),
            dst: "/workspace/b".into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }];
        let diffs = classify_mount_diffs(&original, &pending);
        assert_eq!(diffs.len(), 2);
        // Order: pending first (Added), then original (Removed).
        assert!(matches!(diffs[0], MountDiff::Added(_)));
        assert!(matches!(diffs[1], MountDiff::Removed(_)));
    }

    // ── change_count env-diff coverage (Secrets tab) ──

    /// Setting a new workspace-level env key on `pending` (with
    /// `original.env` empty) contributes exactly +1 to the change count.
    #[test]
    fn change_count_env_set_counts_as_one() {
        let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
        assert_eq!(e.change_count(), 0);
        e.pending.env.insert(
            "DB_URL".into(),
            crate::operator_env::EnvValue::Plain("postgres://…".into()),
        );
        assert_eq!(e.change_count(), 1);
    }

    /// Removing an existing workspace-level env key (seeded in
    /// `original.env` at construction time) contributes exactly +1.
    #[test]
    fn change_count_env_remove_counts_as_one() {
        let mut ws = empty_ws("/a");
        ws.env.insert(
            "DB_URL".into(),
            crate::operator_env::EnvValue::Plain("postgres://…".into()),
        );
        let mut e = EditorState::new_edit("a".into(), ws);
        assert_eq!(e.change_count(), 0);
        e.pending.env.remove("DB_URL");
        assert_eq!(e.change_count(), 1);
    }

    /// Adding and removing per-role env override keys each contribute +1
    /// via the same `env_change_count` helper as workspace-level env.
    #[test]
    fn change_count_agent_env_delta() {
        use crate::workspace::WorkspaceRoleOverride;
        // Seed one role with one env key.
        let mut ws = empty_ws("/a");
        let mut role_x_env = std::collections::BTreeMap::new();
        role_x_env.insert(
            "LOG_LEVEL".into(),
            crate::operator_env::EnvValue::Plain("info".into()),
        );
        ws.roles.insert(
            "agent-x".into(),
            WorkspaceRoleOverride {
                env: role_x_env,
                claude: None,
                codex: None,
                amp: None,
                kimi: None,
                opencode: None,
                github: None,
            },
        );
        let mut e = EditorState::new_edit("a".into(), ws);
        assert_eq!(e.change_count(), 0);

        // Add a new key to pending.
        e.pending.roles.get_mut("agent-x").unwrap().env.insert(
            "DEBUG".into(),
            crate::operator_env::EnvValue::Plain("1".into()),
        );
        assert_eq!(e.change_count(), 1);

        // Remove the original key. Net delta: 2 (one add + one remove).
        e.pending
            .roles
            .get_mut("agent-x")
            .unwrap()
            .env
            .remove("LOG_LEVEL");
        assert_eq!(e.change_count(), 2);
    }

    /// Any env mutation (workspace-level or per-role) flips `is_dirty()`
    /// to true because `pending != original` in the underlying
    /// `WorkspaceConfig` `PartialEq`.
    #[test]
    fn is_dirty_from_env_mutation() {
        use crate::workspace::WorkspaceRoleOverride;

        // Workspace env path.
        let mut e = EditorState::new_edit("a".into(), empty_ws("/a"));
        assert!(!e.is_dirty());
        e.pending
            .env
            .insert("K".into(), crate::operator_env::EnvValue::Plain("v".into()));
        assert!(e.is_dirty(), "workspace env set must make state dirty");

        // Per-role env path.
        let mut e2 = EditorState::new_edit("a".into(), empty_ws("/a"));
        assert!(!e2.is_dirty());
        e2.pending.roles.insert(
            "agent-x".into(),
            WorkspaceRoleOverride {
                env: {
                    let mut m = std::collections::BTreeMap::new();
                    m.insert("K".into(), crate::operator_env::EnvValue::Plain("v".into()));
                    m
                },
                claude: None,
                codex: None,
                amp: None,
                kimi: None,
                opencode: None,
                github: None,
            },
        );
        assert!(e2.is_dirty(), "role env set must make state dirty");
    }

    #[test]
    fn create_prelude_starts_at_first_step() {
        let p = CreatePreludeState::new();
        assert!(matches!(p.step, CreateStep::PickFirstMountSrc));
    }

    // ── completed() helper — keeps name+ws invariants in lockstep ──

    #[test]
    fn completed_returns_none_when_name_missing() {
        let mut p = CreatePreludeState::new();
        p.accept_mount_src(PathBuf::from("/home/user/proj"));
        p.accept_mount_dst("/home/user/proj".into(), false);
        p.accept_workdir("/home/user/proj".into());
        // No accept_name → completed() must be None.
        assert!(p.completed().is_none());
    }

    #[test]
    fn completed_returns_none_when_mount_src_missing() {
        let mut p = CreatePreludeState::new();
        // Skip accept_mount_src and accept_mount_dst.
        p.pending_workdir = Some("/home/user/proj".into());
        p.pending_name = Some("proj".into());
        // build_workspace fails on missing src → completed() None.
        assert!(p.completed().is_none());
    }

    #[test]
    fn completed_returns_none_when_workdir_missing() {
        let mut p = CreatePreludeState::new();
        p.accept_mount_src(PathBuf::from("/home/user/proj"));
        p.accept_mount_dst("/home/user/proj".into(), false);
        // Skip accept_workdir.
        p.pending_name = Some("proj".into());
        assert!(p.completed().is_none());
    }

    #[test]
    fn completed_returns_some_when_all_fields_present() {
        let mut p = CreatePreludeState::new();
        p.accept_mount_src(PathBuf::from("/home/user/proj"));
        p.accept_mount_dst("/home/user/proj".into(), false);
        p.accept_workdir("/home/user/proj".into());
        p.accept_name("proj".into());
        let (name, ws) = p.completed().expect("all fields present");
        assert_eq!(name, "proj");
        assert_eq!(ws.workdir, "/home/user/proj");
        assert_eq!(ws.mounts.len(), 1);
        assert_eq!(ws.mounts[0].src, "/home/user/proj");
    }

    /// Pin the enum contract: round-tripping a `ManagerListRow` through
    /// `to_screen_index` / `row_at` / `selected_row` must yield the same
    /// logical row. Covers the three variants over a non-trivial saved set.
    #[test]
    fn manager_list_row_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        config.workspaces.insert("a".into(), empty_ws("/a"));
        config.workspaces.insert("b".into(), empty_ws("/b"));
        config.workspaces.insert("c".into(), empty_ws("/c"));
        let mut state = ManagerState::from_config(&config, cwd);

        let saved_count = state.workspaces.len();
        assert_eq!(state.row_count(), saved_count + 2);
        assert_eq!(state.new_workspace_row_index(), saved_count + 1);

        let rows = [
            ManagerListRow::CurrentDirectory,
            ManagerListRow::SavedWorkspace(0),
            ManagerListRow::SavedWorkspace(1),
            ManagerListRow::SavedWorkspace(2),
            ManagerListRow::NewWorkspace,
        ];
        for row in rows {
            let idx = row.to_screen_index(saved_count);
            assert_eq!(state.row_at(idx), Some(row), "row_at({idx}) for {row:?}");
            state.selected = idx;
            assert_eq!(state.selected_row(), row, "selected_row for idx={idx}");
        }

        assert_eq!(
            ManagerListRow::NewWorkspace.to_visual_index(saved_count),
            saved_count + 2
        );
        assert_eq!(state.row_at_visual_index(saved_count + 1), None);
        assert_eq!(
            state.row_at_visual_index(saved_count + 2),
            Some(ManagerListRow::NewWorkspace)
        );

        // Out-of-range index returns None.
        assert_eq!(state.row_at(saved_count + 2), None);
    }

    /// `selected_workspace_summary` must return `None` for both synthetic
    /// rows (cwd + sentinel) and `Some(&WorkspaceSummary)` for a real
    /// saved row.
    #[test]
    fn manager_selected_workspace_summary_is_none_for_synthetic_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        config.workspaces.insert("alpha".into(), empty_ws("/alpha"));
        let mut state = ManagerState::from_config(&config, cwd);

        // Current directory row.
        state.selected = ManagerListRow::CurrentDirectory.to_screen_index(1);
        assert!(state.selected_workspace_summary().is_none());
        assert!(state.is_current_dir_selected());

        // Saved workspace row.
        state.selected = ManagerListRow::SavedWorkspace(0).to_screen_index(1);
        let summary = state
            .selected_workspace_summary()
            .expect("saved row exposes summary");
        assert_eq!(summary.name, "alpha");

        // "+ New workspace" sentinel.
        state.selected = ManagerListRow::NewWorkspace.to_screen_index(1);
        assert!(state.selected_workspace_summary().is_none());
        assert!(state.is_new_workspace_selected());
    }

    #[test]
    fn global_mounts_state_persists_add_edit_remove_rename_scope_readonly() {
        let temp = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "").unwrap();
        let source_a = temp.path().join("cache-a");
        let source_b = temp.path().join("cache-b");
        std::fs::create_dir_all(&source_a).unwrap();
        std::fs::create_dir_all(&source_b).unwrap();

        let mut state = GlobalMountsState::from_config(&AppConfig::default());
        state.pending.push(crate::config::GlobalMountRow {
            scope: None,
            name: "gradle".into(),
            mount: MountConfig {
                src: source_a.display().to_string(),
                dst: "/home/agent/.gradle/caches".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        });
        state.save_to_config(&paths).unwrap();

        state.pending[0].name = "cargo".into();
        state.pending[0].mount.src = source_b.display().to_string();
        state.pending[0].mount.dst = "/home/agent/.cargo/registry".into();
        state.pending[0].mount.readonly = true;
        state.pending[0].scope = Some("chainargos/*".into());
        state.pending.push(crate::config::GlobalMountRow {
            scope: None,
            name: "remove-me".into(),
            mount: MountConfig {
                src: source_a.display().to_string(),
                dst: "/remove-me".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        });
        state.pending.retain(|row| row.name != "remove-me");
        let saved = state.save_to_config(&paths).unwrap();

        let rows = saved.list_mount_rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "cargo");
        assert_eq!(rows[0].scope.as_deref(), Some("chainargos/*"));
        assert!(rows[0].mount.readonly);
        assert_eq!(rows[0].mount.dst, "/home/agent/.cargo/registry");
        let raw = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(raw.contains("[docker.mounts.\"chainargos/*\"]"), "{raw}");
        assert!(!raw.contains("remove-me"), "{raw}");
    }

    // ── cycle_isolation_for_selected_mount ─────────────────────────────

    /// Build an editor sitting on the Mounts tab with a single Shared mount,
    /// cursor on row 0. Mirrors the readonly toggle test fixtures so the new
    /// I-hotkey tests share the same shape as the R-hotkey ones.
    fn editor_with_one_shared_mount() -> EditorState<'static> {
        use std::collections::BTreeMap;
        let ws = WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: String::new(),
            mounts: vec![MountConfig {
                src: "/host/a".into(),
                dst: "/host/a".into(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_roles: vec![],
            default_role: None,
            default_agent: None,
            last_role: None,
            env: BTreeMap::default(),
            roles: BTreeMap::default(),
            keep_awake: KeepAwakeConfig::default(),
            op_account: None,
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            github: None,
            git_pull_on_entry: false,
        };
        let mut e = EditorState::new_edit("ws".into(), ws);
        e.active_tab = EditorTab::Mounts;
        e.active_field = FieldFocus::Row(0);
        e
    }

    #[test]
    fn cycle_isolation_shared_to_worktree() {
        let mut e = editor_with_one_shared_mount();
        e.cycle_isolation_for_selected_mount();
        assert_eq!(
            e.pending.mounts[0].isolation,
            crate::isolation::MountIsolation::Worktree,
            "Shared must cycle to Worktree on first I press"
        );
    }

    #[test]
    fn cycle_isolation_worktree_back_to_shared() {
        let mut e = editor_with_one_shared_mount();
        e.cycle_isolation_for_selected_mount();
        e.cycle_isolation_for_selected_mount();
        assert_eq!(
            e.pending.mounts[0].isolation,
            crate::isolation::MountIsolation::Clone,
            "two I presses must cycle Worktree to Clone",
        );
        e.cycle_isolation_for_selected_mount();
        assert_eq!(
            e.pending.mounts[0].isolation,
            crate::isolation::MountIsolation::Shared,
            "three I presses must net back to Shared",
        );
        assert_eq!(
            e.change_count(),
            0,
            "a full cycle Shared → Worktree → Shared must net zero changes",
        );
    }

    #[test]
    fn cycle_isolation_on_sentinel_is_noop() {
        // Cursor on the `+ Add mount` sentinel (row == mounts.len()) — I must
        // not mutate mounts or trigger a change.
        let mut e = editor_with_one_shared_mount();
        e.active_field = FieldFocus::Row(e.pending.mounts.len());
        let before = e.pending.mounts.clone();
        e.cycle_isolation_for_selected_mount();
        assert_eq!(
            e.pending.mounts, before,
            "I on sentinel row must leave mounts untouched"
        );
        assert_eq!(e.change_count(), 0);
    }
}

//! Manager state machine. See docs/superpowers/specs/2026-04-23-workspace-manager-tui-design.md § 3.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
    /// An active instance under the synthetic "Current directory" row.
    /// `instance_idx` is the position within
    /// `ManagerState::current_dir_active_instances`.
    CurrentDirectoryInstance(usize),
    SavedWorkspace(usize),
    /// An active instance under a saved workspace. `(workspace_idx,
    /// instance_idx)` where `instance_idx` is the position within
    /// `ManagerState::workspace_active_instances(workspace_idx)`.
    WorkspaceInstance(usize, usize),
    NewWorkspace,
}

impl ManagerListRow {
    /// Screen index in the selectable row list. Returns `None` for
    /// `WorkspaceInstance` — instance rows are injected mid-list when their
    /// parent is expanded, so they have no fixed position. Use `index_of_row`
    /// instead when the caller may hold an instance row.
    #[must_use]
    pub const fn to_screen_index(self, saved_count: usize) -> Option<usize> {
        match self {
            Self::CurrentDirectory => Some(0),
            Self::SavedWorkspace(i) => Some(i + 1),
            Self::NewWorkspace => Some(saved_count + 1),
            Self::WorkspaceInstance(_, _) | Self::CurrentDirectoryInstance(_) => None,
        }
    }

    /// Visual-list position including the blank spacer before `NewWorkspace`.
    /// Returns `None` for `WorkspaceInstance` — same reason as `to_screen_index`.
    /// Use `visual_rows_vec` + `visual_selected` for instance row lookups.
    #[must_use]
    pub const fn to_visual_index(self, saved_count: usize) -> Option<usize> {
        match self {
            Self::CurrentDirectory => Some(0),
            Self::SavedWorkspace(i) => Some(i + 1),
            Self::NewWorkspace => {
                if saved_count > 0 {
                    Some(saved_count + 2)
                } else {
                    Some(saved_count + 1)
                }
            }
            Self::WorkspaceInstance(_, _) | Self::CurrentDirectoryInstance(_) => None,
        }
    }
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)] // independent UI focus flags, not a config-style bag
pub struct ManagerState<'a> {
    pub stage: ManagerStage<'a>,
    pub workspaces: Vec<WorkspaceSummary>,
    pub instances: Vec<crate::instance::InstanceIndexEntry>,
    pub current_dir: String,
    pub selected: usize,
    /// Modal slot at the list level (e.g. `Modal::GithubPicker`); the
    /// Editor / `CreatePrelude` stages own their own modal slots.
    pub list_modal: Option<Modal<'a>>,
    /// Passive overlay drawn on top of `list_modal` for the duration of
    /// a single frame while a blocking async operation runs (currently
    /// the console role-resolution path). Input handlers do not see it.
    pub status_overlay: Option<crate::console::widgets::status_popup::StatusPopupState>,
    pub inline_role_picker: Option<RolePickerState>,
    pub inline_agent_picker: Option<(
        crate::selector::RoleSelector,
        crate::console::widgets::agent_choice::AgentChoiceState,
    )>,
    /// Agent picker opened when the operator presses `N` on an instance row
    /// to start a new session in the running container. Carries the target
    /// `container_base`, the agent picker, and the available Claude provider
    /// list (empty when `ZAI_API_KEY` is not configured for the instance's
    /// workspace/role).
    #[allow(clippy::type_complexity)]
    pub inline_new_session_picker: Option<(
        String,
        crate::console::widgets::agent_choice::AgentChoiceState,
        Vec<(String, Vec<(String, String)>)>,
    )>,
    /// Provider picker shown after the agent is committed in
    /// `inline_new_session_picker`, when multiple providers are available.
    /// Tuple fields: (`container`, `agent`, `providers`, `selected_index`).
    #[allow(clippy::type_complexity)]
    pub inline_provider_picker: Option<(
        String,
        crate::agent::Agent,
        Vec<(String, Vec<(String, String)>)>,
        usize,
    )>,
    /// Provider picker for the initial workspace launch (before the container
    /// exists). Shown after the operator commits an agent choice and
    /// `ZAI_API_KEY` is configured. Tuple: (`role_selector`, `agent`,
    /// `providers`, `selected_index`).
    #[allow(clippy::type_complexity)]
    pub launch_provider_picker: Option<(
        crate::selector::RoleSelector,
        crate::agent::Agent,
        Vec<(String, Vec<(String, String)>)>,
        usize,
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
    /// Logical list row the pointer is hovering (lifts its background like a
    /// hovered tab). Transient; set on mouse motion, cleared off the list.
    pub hovered_list_row: Option<ManagerListRow>,
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
    /// Dedup gate: last error string from `refresh_instances`. Without
    /// this, a persistent parse error would reopen the popup on every
    /// 20 Hz tick — operators would never be able to dismiss it.
    instances_last_error: Option<String>,
    /// Which saved-workspace indices are expanded in the tree view.
    /// Indices are positions in `self.workspaces` and are only valid for
    /// the lifetime of this `ManagerState` instance — workspace changes
    /// always fully rebuild state, clearing this set.
    pub expanded_workspaces: BTreeSet<usize>,
    /// Whether the synthetic "Current directory" row is expanded to
    /// show its active instances. Mirrors `expanded_workspaces` for
    /// the one-off cwd row, which has no index into `workspaces`.
    pub current_dir_expanded: bool,
    /// Cached sessions per active instance keyed by `container_base`.
    /// Populated from manifests during `refresh_instances`.
    pub instance_sessions: HashMap<String, Vec<crate::instance::SessionRecord>>,
    /// Containers whose manifests could not be read during the last
    /// `refresh_instances` pass. Cleared on every successful index load.
    instance_session_errors: HashSet<String>,
    /// Live tab/pane snapshot per running instance keyed by
    /// `container_base`. Populated each `refresh_instances` tick by
    /// fetching from the daemon's bind-mounted socket at
    /// `~/.jackin/sockets/<container>/jackin.sock`. Missing keys mean
    /// the snapshot is unavailable (container not running, socket
    /// pre-dates the bind-mount, or the fetch failed).
    pub instance_snapshots: HashMap<String, crate::runtime::snapshot::InstanceSnapshot>,
    /// `true` when the operator has dropped cursor focus into the
    /// snapshot preview pane via Tab / →. While set, ↑/↓ navigates
    /// `preview_pane_cursor` through the flattened pane list and
    /// Enter attaches with the selected pane's focus id. Esc / ← /
    /// `BackTab` pops focus back to the workspace tree.
    pub preview_focused: bool,
    /// Operator-selected pane index within the flattened pane list
    /// of the focused instance, keyed by `container_base`. Persists
    /// across re-entries to the preview pane so the operator's last
    /// selection survives a `Esc → ↑/↓ → Tab` round-trip.
    pub preview_pane_cursor: HashMap<String, usize>,
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
    ConfirmDelete {
        name: String,
        state: ConfirmState,
    },
    /// Y/N gate on a destructive instance action. Currently used only
    /// by Purge (which now ejects + deletes preserved state in a single
    /// keystroke, so a confirm step keeps a mis-keyed `P` from
    /// destroying running work). Holds the resolved container plus
    /// human-readable label so the modal can name what is about to be
    /// destroyed.
    ConfirmInstancePurge {
        container: String,
        label: String,
        state: ConfirmState,
    },
}

#[derive(Debug)]
pub struct GlobalMountsState<'a> {
    pub selected: usize,
    pub pending: Vec<crate::config::GlobalMountRow>,
    pub original: Vec<crate::config::GlobalMountRow>,
    pub modal: Option<GlobalMountModal<'a>>,
    pub modal_parents: Vec<GlobalMountModal<'a>>,
    pub add_draft: Option<GlobalMountDraft>,
    pub error: Option<String>,
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
    /// Index of the tab cell under the pointer, repainted on mouse motion so
    /// the strip reacts to hover like the in-container multiplexer tabs.
    pub hovered_tab: Option<usize>,
    pub general: SettingsGeneralState,
    pub mounts: GlobalMountsState<'a>,
    pub env: SettingsEnvState<'a>,
    pub auth: SettingsAuthState,
    pub trust: SettingsTrustState,
    /// Error popup shown on top of all settings content. Dismissed with
    /// Enter / O / Esc; clears automatically when opened again.
    pub error_popup: Option<ErrorPopupState>,
    /// Set by the Auth-tab `g`/`G` generate action; drained by the
    /// `run_console` loop to run the global Claude OAuth-token mint.
    pub pending_token_generate: Option<PendingTokenGenerate>,
    /// Footer height (rows) the renderer last laid out, cached so mouse
    /// hit-testing subtracts the same dynamic footer the frame drew rather than
    /// a stale constant — otherwise clicks near the bottom mis-map.
    pub cached_footer_h: u16,
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
    pub modal_parents: Vec<SettingsEnvModal<'a>>,
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
    /// Set while the `g`/`G` generate action's Create-mode `OpPicker` is
    /// open, so its commit knows the pick is a token-generate (always
    /// global Claude) rather than a browse/provide pick.
    pub generating_token: bool,
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
    /// Row the pointer is hovering (lifts its background like a hovered tab).
    pub hovered: Option<usize>,
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

/// A request to mint a Claude OAuth token and write it to the chosen
/// 1Password location.
///
/// Bubbled from the auth-form generate action up to the `run_console`
/// loop, which owns `paths`, `config`, and the terminal needed to run
/// `claude setup-token`.
#[derive(Debug, Clone)]
pub struct PendingTokenGenerate {
    pub scope: crate::workspace::token_setup::TokenSetupScope,
    pub args: crate::workspace::token_setup::TokenSetupArgs,
}

#[derive(Debug)]
pub struct EditorState<'a> {
    pub mode: EditorMode,
    pub active_tab: EditorTab,
    /// W3C ARIA Tabs: when true, focus is on the tab list (←/→ cycle tabs,
    /// Tab/↓ enters content); when false, focus is in the tab panel.
    pub tab_bar_focused: bool,
    /// Index of the tab cell under the pointer, repainted on mouse motion so
    /// the strip reacts to hover like the in-container multiplexer tabs.
    pub hovered_tab: Option<usize>,
    pub active_field: FieldFocus,
    pub original: WorkspaceConfig,
    pub pending: WorkspaceConfig,
    pub modal: Option<Modal<'a>>,
    /// Parent chain backing the Esc-back rule from
    /// `docs/.../tui-design-decisions.mdx` — modals that opened a
    /// sub-modal (`open_sub_modal`) stash themselves here so cancel
    /// on the child can pop back rather than dumping to the
    /// underlying tab. Top of the vec = parent immediately under
    /// the currently-visible `modal`. Empty = the visible modal has
    /// no parent in the chain (Esc closes back to the tab as
    /// before). `clear_modal_chain` empties both this and `modal` on
    /// terminal commits where the chain has finished its job.
    pub modal_parents: Vec<Modal<'a>>,
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
    /// Mounts-tab row the pointer is hovering (lifts its background like a
    /// hovered tab). Transient; set on mouse motion.
    pub hovered_mount_row: Option<usize>,
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
    /// Set when the auth-form "generate token" action launches the
    /// `op_picker` in Create mode, so the `op_picker` commit knows the
    /// pick is a token-generate (not a browse/provide pick) and which
    /// layer it targets. Consumed (taken) by the `op_picker` commit.
    pub generating_token_target: Option<AuthFormTarget>,
    /// Set by the `op_picker` commit when `generating_token_target` was
    /// present; drained by the `run_console` loop to run the mint.
    pub pending_token_generate: Option<PendingTokenGenerate>,
    /// Footer height (rows) the renderer last laid out, cached so mouse
    /// hit-testing subtracts the same dynamic footer the frame drew rather than
    /// a stale constant — otherwise clicks near the bottom mis-map.
    pub cached_footer_h: u16,
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
            modal_parents: Vec::new(),
            add_draft: None,
            error: None,
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
        self.modal = None;
        self.modal_parents.clear();
        self.error = None;
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

impl<'a> GlobalMountsState<'a> {
    pub fn open_sub_modal(&mut self, child: GlobalMountModal<'a>) {
        if let Some(parent) = self.modal.take() {
            self.modal_parents.push(parent);
        }
        self.modal = Some(child);
    }

    pub fn pop_modal_chain(&mut self) {
        self.modal = self.modal_parents.pop();
    }

    pub fn clear_modal_chain(&mut self) {
        self.modal = None;
        self.modal_parents.clear();
    }
}

impl SettingsState<'_> {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            active_tab: SettingsTab::General,
            tab_bar_focused: true,
            hovered_tab: None,
            general: SettingsGeneralState::from_config(config),
            mounts: GlobalMountsState::from_config(config),
            env: SettingsEnvState::from_config(config),
            auth: SettingsAuthState::from_config(config),
            trust: SettingsTrustState::from_config(config),
            error_popup: None,
            pending_token_generate: None,
            cached_footer_h: 1,
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
        // A generate request queued just before the discard would
        // otherwise still be drained by the `run_console` loop and launch
        // an unwanted mint. `auth.discard()` already cleared
        // `generating_token`; the queued request lives here.
        self.pending_token_generate = None;
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
                crate::console::manager::auth_kind::AuthKind::Zai => {
                    // Z.AI auth is env-only; the credential lives in env_vars and
                    // is written via the env block path above — no auth_forward
                    // config block to commit here.
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
            modal_parents: Vec::new(),
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
        self.modal_parents.clear();
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

impl<'a> SettingsEnvState<'a> {
    pub fn open_sub_modal(&mut self, child: SettingsEnvModal<'a>) {
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

    /// See [`EditorState::drop_modal_scratch`]: when the modal chain
    /// fully unwinds, clear the env-key + picker-value scratch slots
    /// so a later commit cannot accidentally target a stale (scope, key).
    fn drop_modal_scratch(&mut self) {
        self.pending_env_key = None;
        self.pending_picker_value = None;
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
            crate::console::manager::auth_kind::AuthKind::Zai,
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
                crate::console::manager::auth_kind::AuthKind::Zai => {
                    if config.env.contains_key("ZAI_API_KEY") {
                        crate::console::manager::auth_kind::AuthMode::ApiKey
                    } else {
                        crate::console::manager::auth_kind::AuthMode::Ignore
                    }
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
            generating_token: false,
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
        self.generating_token = false;
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
            hovered: None,
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

pub(super) fn active_instances_matching<'a>(
    instances: &'a [crate::instance::InstanceIndexEntry],
    query: crate::instance::InstanceQuery<'a>,
) -> impl Iterator<Item = &'a crate::instance::InstanceIndexEntry> {
    instances.iter().filter(move |e| {
        e.matches(query)
            && matches!(
                e.status,
                crate::instance::InstanceStatus::Active | crate::instance::InstanceStatus::Running
            )
    })
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
        let selected = selected_row.to_screen_index(saved_count).unwrap_or(0);

        Self {
            stage: ManagerStage::List,
            workspaces,
            instances: Vec::new(),
            current_dir: cwd.display().to_string(),
            selected,
            list_modal: None,
            status_overlay: None,
            inline_role_picker: None,
            inline_agent_picker: None,
            inline_new_session_picker: None,
            inline_provider_picker: None,
            launch_provider_picker: None,
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
            hovered_list_row: None,
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
            expanded_workspaces: BTreeSet::new(),
            current_dir_expanded: false,
            instance_sessions: HashMap::new(),
            instance_session_errors: HashSet::new(),
            instance_snapshots: HashMap::new(),
            preview_focused: false,
            preview_pane_cursor: HashMap::new(),
        }
    }

    // ── Tree navigation helpers ────────────────────────────────────

    /// Instances that appear in the tree for workspace `ws_idx` — only
    /// `Active` / `Running` containers are shown.
    #[must_use]
    pub fn workspace_active_instances(
        &self,
        ws_idx: usize,
    ) -> Vec<&crate::instance::InstanceIndexEntry> {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return Vec::new();
        };
        let query = crate::instance::InstanceQuery {
            workspace_name: Some(ws.name.as_str()),
            workspace_label: ws.name.as_str(),
            workdir: ws.workdir.as_str(),
            role_key: None,
            agent_runtime: None,
        };
        active_instances_matching(&self.instances, query).collect()
    }

    #[must_use]
    pub fn has_active_instances(&self, ws_idx: usize) -> bool {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return false;
        };
        let query = crate::instance::InstanceQuery {
            workspace_name: Some(ws.name.as_str()),
            workspace_label: ws.name.as_str(),
            workdir: ws.workdir.as_str(),
            role_key: None,
            agent_runtime: None,
        };
        active_instances_matching(&self.instances, query)
            .next()
            .is_some()
    }

    #[must_use]
    pub fn has_current_dir_active_instances(&self) -> bool {
        let current_dir = self.current_dir.as_str();
        let query = crate::instance::InstanceQuery {
            workspace_name: None,
            workspace_label: current_dir,
            workdir: current_dir,
            role_key: None,
            agent_runtime: None,
        };
        active_instances_matching(&self.instances, query)
            .next()
            .is_some()
    }

    /// Instances in the tree for the "Current directory" synthetic row.
    #[must_use]
    pub fn current_dir_active_instances(&self) -> Vec<&crate::instance::InstanceIndexEntry> {
        let current_dir = self.current_dir.as_str();
        let query = crate::instance::InstanceQuery {
            workspace_name: None,
            workspace_label: current_dir,
            workdir: current_dir,
            role_key: None,
            agent_runtime: None,
        };
        active_instances_matching(&self.instances, query).collect()
    }

    /// Flat ordered list of selectable rows accounting for tree expansion.
    /// Instance rows appear immediately after their parent workspace row.
    fn selectable_rows_vec(&self) -> Vec<ManagerListRow> {
        let mut rows = vec![ManagerListRow::CurrentDirectory];
        if self.current_dir_expanded {
            let count = self.current_dir_active_instances().len();
            for j in 0..count {
                rows.push(ManagerListRow::CurrentDirectoryInstance(j));
            }
        }
        for (i, _) in self.workspaces.iter().enumerate() {
            rows.push(ManagerListRow::SavedWorkspace(i));
            if self.expanded_workspaces.contains(&i) {
                let count = self.workspace_active_instances(i).len();
                for j in 0..count {
                    rows.push(ManagerListRow::WorkspaceInstance(i, j));
                }
            }
        }
        rows.push(ManagerListRow::NewWorkspace);
        rows
    }

    /// Visual row list for rendering — same as `selectable_rows_vec` plus a
    /// `None` spacer before `NewWorkspace` when saved workspaces exist.
    pub fn visual_rows_vec(&self) -> Vec<Option<ManagerListRow>> {
        let mut rows: Vec<Option<ManagerListRow>> = vec![Some(ManagerListRow::CurrentDirectory)];
        if self.current_dir_expanded {
            let count = self.current_dir_active_instances().len();
            for j in 0..count {
                rows.push(Some(ManagerListRow::CurrentDirectoryInstance(j)));
            }
        }
        for (i, _) in self.workspaces.iter().enumerate() {
            rows.push(Some(ManagerListRow::SavedWorkspace(i)));
            if self.expanded_workspaces.contains(&i) {
                let count = self.workspace_active_instances(i).len();
                for j in 0..count {
                    rows.push(Some(ManagerListRow::WorkspaceInstance(i, j)));
                }
            }
        }
        if !self.workspaces.is_empty() {
            rows.push(None); // spacer before "+ New workspace"
        }
        rows.push(Some(ManagerListRow::NewWorkspace));
        rows
    }

    /// Returns the position of `row` in `selectable_rows_vec`, or `None`.
    #[must_use]
    pub fn index_of_row(&self, row: ManagerListRow) -> Option<usize> {
        self.selectable_rows_vec().iter().position(|r| *r == row)
    }

    // ── Core navigation ───────────────────────────────────────────

    /// Total number of selectable rows (includes instance rows when expanded).
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.selectable_rows_vec().len()
    }

    /// Index of the "+ New workspace" sentinel row in the selectable list.
    #[must_use]
    pub fn new_workspace_row_index(&self) -> usize {
        self.selectable_rows_vec().len().saturating_sub(1)
    }

    /// Decode a selectable-list index into a [`ManagerListRow`].
    #[must_use]
    pub fn row_at(&self, idx: usize) -> Option<ManagerListRow> {
        self.selectable_rows_vec().get(idx).copied()
    }

    /// Decode a visual-list index (may include the non-selectable spacer)
    /// into a [`ManagerListRow`]. Returns `None` for the spacer row.
    #[must_use]
    pub fn row_at_visual_index(&self, idx: usize) -> Option<ManagerListRow> {
        self.visual_rows_vec().get(idx).copied().flatten()
    }

    /// Visual-list index of the currently selected row (for ratatui
    /// highlight). Differs from `selected` when instance rows are visible.
    #[must_use]
    pub fn visual_selected(&self) -> usize {
        let selected = self.selected_row();
        self.visual_rows_vec()
            .iter()
            .position(|r| r.as_ref() == Some(&selected))
            .unwrap_or_else(|| {
                crate::debug_log!(
                    "console",
                    "visual_selected: {:?} not in visual list, clamping to 0",
                    selected
                );
                0 // CurrentDirectory is always row 0 and is never removed
            })
    }

    /// What the operator currently has highlighted.
    #[must_use]
    pub fn selected_row(&self) -> ManagerListRow {
        self.selectable_rows_vec()
            .get(self.selected)
            .copied()
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

    /// Whether the workspace tree node at `ws_idx` is expanded.
    #[must_use]
    pub fn is_workspace_expanded(&self, ws_idx: usize) -> bool {
        self.expanded_workspaces.contains(&ws_idx)
    }

    /// Recorded sessions for `container_base`, or an empty slice when none
    /// are cached (no sessions or manifest not yet loaded).
    #[must_use]
    pub fn sessions_for_instance(&self, container_base: &str) -> &[crate::instance::SessionRecord] {
        self.instance_sessions
            .get(container_base)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Returns `true` when the last `refresh_instances` pass failed to read
    /// the instance manifest for `container_base`.
    #[must_use]
    pub fn has_session_load_error(&self, container_base: &str) -> bool {
        self.instance_session_errors.contains(container_base)
    }

    /// Live tab/pane snapshot the daemon reported in the last
    /// `refresh_instances` tick, or `None` when the bind-mounted socket
    /// is absent or the fetch failed. `render_instance_details_pane`
    /// prefers this over the on-disk manifest sessions when present.
    #[must_use]
    pub fn snapshot_for_instance(
        &self,
        container_base: &str,
    ) -> Option<&crate::runtime::snapshot::InstanceSnapshot> {
        self.instance_snapshots.get(container_base)
    }

    /// Flatten the per-instance snapshot's tab/pane tree into a
    /// linear list the preview's ↑/↓ navigation can index into.
    /// Each entry is `(tab_idx, session_id)`. Empty when no
    /// snapshot exists for the container.
    #[must_use]
    pub fn flattened_preview_panes(&self, container_base: &str) -> Vec<(usize, u64)> {
        let Some(snapshot) = self.instance_snapshots.get(container_base) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (tab_idx, tab) in snapshot.tabs.iter().enumerate() {
            for pane in &tab.panes {
                out.push((tab_idx, pane.session_id));
            }
        }
        out
    }

    /// Currently-selected pane in the preview, clamped against the
    /// flattened list. Returns `None` when the snapshot is missing
    /// or the list is empty.
    #[must_use]
    pub fn preview_selected_pane(&self, container_base: &str) -> Option<(usize, u64)> {
        let panes = self.flattened_preview_panes(container_base);
        if panes.is_empty() {
            return None;
        }
        let cursor = self
            .preview_pane_cursor
            .get(container_base)
            .copied()
            .unwrap_or(0)
            .min(panes.len() - 1);
        panes.get(cursor).copied()
    }

    /// The [`WorkspaceSummary`] currently highlighted, or `None` when the
    /// selection is on Current Directory, New Workspace, or a `WorkspaceInstance`.
    #[must_use]
    pub fn selected_workspace_summary(&self) -> Option<&WorkspaceSummary> {
        if let ManagerListRow::SavedWorkspace(i) = self.selected_row() {
            self.workspaces.get(i)
        } else {
            None
        }
    }

    // ── Tree expand / collapse ────────────────────────────────────

    /// Expand the workspace tree node at `ws_idx`. No-op when already
    /// expanded or when there are no active instances.
    pub fn expand_workspace(&mut self, ws_idx: usize) {
        if !self.workspace_active_instances(ws_idx).is_empty() {
            self.expanded_workspaces.insert(ws_idx);
        }
    }

    /// Expand the synthetic "Current directory" row. No-op when
    /// already expanded or when no instances point at the cwd.
    pub fn expand_current_dir(&mut self) {
        if self.has_current_dir_active_instances() {
            self.current_dir_expanded = true;
        }
    }

    /// Collapse the synthetic "Current directory" row. When the
    /// cursor is on one of its instance children, jumps the cursor
    /// up to the parent row first.
    pub fn collapse_current_dir(&mut self) {
        if !self.current_dir_expanded {
            return;
        }
        let was_on_child = matches!(
            self.selected_row(),
            ManagerListRow::CurrentDirectoryInstance(_)
        );
        self.current_dir_expanded = false;
        if was_on_child {
            self.selected = 0; // CurrentDirectory is always row 0
        }
    }

    /// Collapse the workspace tree node at `ws_idx`. When the cursor is
    /// on a child instance row, jumps up to the workspace row.
    pub fn collapse_workspace(&mut self, ws_idx: usize) {
        if !self.expanded_workspaces.contains(&ws_idx) {
            return;
        }
        let was_on_child = matches!(
            self.selected_row(),
            ManagerListRow::WorkspaceInstance(w, _) if w == ws_idx
        );
        self.expanded_workspaces.remove(&ws_idx);
        if was_on_child {
            let rows = self.selectable_rows_vec();
            self.selected = rows
                .iter()
                .position(|r| *r == ManagerListRow::SavedWorkspace(ws_idx))
                .unwrap_or_else(|| {
                    crate::debug_log!(
                        "console",
                        "collapse_workspace: ws_idx={ws_idx} not in selectable rows, clamping to 0"
                    );
                    0 // CurrentDirectory is always row 0 and is never removed
                });
        } else {
            // Clamp in case removal shrunk the list.
            self.selected = self
                .selected
                .min(self.selectable_rows_vec().len().saturating_sub(1));
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
                // Load recorded sessions for each active/running instance.
                // These come from persisted manifests and may not reflect live
                // multiplexer state, but provide useful context without Docker exec.
                self.instance_sessions.clear();
                self.instance_session_errors.clear();
                self.instance_snapshots.clear();
                let mut snapshot_targets: Vec<String> = Vec::new();
                for entry in &self.instances {
                    if matches!(
                        entry.status,
                        crate::instance::InstanceStatus::Active
                            | crate::instance::InstanceStatus::Running
                    ) {
                        let state_dir = paths.data_dir.join(&entry.container_base);
                        match crate::instance::InstanceManifest::read(&state_dir) {
                            Ok(manifest) if !manifest.sessions.is_empty() => {
                                self.instance_sessions
                                    .insert(entry.container_base.clone(), manifest.sessions);
                            }
                            Ok(_) => {}
                            Err(e) => {
                                crate::debug_log!(
                                    "console",
                                    "manifest read failed for {}: {e:#}",
                                    entry.container_base
                                );
                                self.instance_session_errors
                                    .insert(entry.container_base.clone());
                            }
                        }
                        snapshot_targets.push(entry.container_base.clone());
                    }
                }
                let snapshot_results = fetch_snapshots_parallel(paths, &snapshot_targets);
                for (container, result) in snapshot_results {
                    match result {
                        Ok(Some(snapshot)) => {
                            self.instance_snapshots.insert(container, snapshot);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            crate::debug_log!(
                                "console",
                                "snapshot fetch failed for {container}: {e:#}"
                            );
                        }
                    }
                }
                // Evict preview cursors keyed on containers that no
                // longer have a live snapshot — otherwise the map
                // accumulates indefinitely across container churn.
                self.preview_pane_cursor
                    .retain(|key, _| self.instance_snapshots.contains_key(key));
                // Clamp `selected` after a refresh in case an instance row
                // that was selected has disappeared.
                let max = self.row_count().saturating_sub(1);
                self.selected = self.selected.min(max);
            }
            Err(error) => {
                self.instances.clear();
                self.instance_sessions.clear();
                self.instance_session_errors.clear();
                self.expanded_workspaces.clear();
                // Mirror the Ok-branch cleanup of the snapshot-derived
                // surfaces — without this they accumulate stale entries
                // keyed by container_base that no longer appears in
                // the index, and `current_dir_expanded` latched against
                // an empty instance list drifts the row count.
                self.instance_snapshots.clear();
                self.preview_pane_cursor.clear();
                self.current_dir_expanded = false;
                self.preview_focused = false;
                let message = format!("instance index error: {error}");
                if self.instances_last_error.as_deref() != Some(&message) {
                    self.list_modal = Some(Modal::ErrorPopup {
                        state: crate::console::widgets::error_popup::ErrorPopupState::new(
                            "Instance index error",
                            &message,
                        ),
                    });
                    self.instances_last_error = Some(message);
                }
            }
        }
    }

    /// Force the next `refresh_instances` call to re-read disk regardless of
    /// the throttle interval. Use after an action mutates the on-disk
    /// instance index (Stop/Purge) so the next list draw reflects the new
    /// state immediately instead of waiting up to `REFRESH_INTERVAL`.
    pub const fn force_refresh_instances(&mut self) {
        self.instances_last_refresh = None;
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

/// Fan-out snapshot fetches in parallel so the render thread's
/// wall-clock cost stays bounded by the per-fetch `SOCKET_TIMEOUT`
/// (2 s) regardless of how many active instances exist. A serial loop
/// would stall the TUI for `N × SOCKET_TIMEOUT` on a host with several
/// wedged containers. Chunks cap thread-creation churn so a host with
/// dozens of active containers does not spawn dozens of OS threads
/// per 500 ms refresh tick; each chunk's wall-clock cost is still
/// bounded by the slowest fetch in that chunk.
fn fetch_snapshots_parallel(
    paths: &crate::paths::JackinPaths,
    targets: &[String],
) -> Vec<(
    String,
    anyhow::Result<Option<crate::runtime::snapshot::InstanceSnapshot>>,
)> {
    const SNAPSHOT_FANOUT_CHUNK: usize = 8;
    let mut results = Vec::with_capacity(targets.len());
    for chunk in targets.chunks(SNAPSHOT_FANOUT_CHUNK) {
        let chunk_results = std::thread::scope(|s| {
            // Collect all `spawn` handles first so every thread starts
            // before any join blocks; folding collect+join into one
            // chain would serialise the work.
            #[allow(clippy::needless_collect)]
            let handles: Vec<_> = chunk
                .iter()
                .map(|container| {
                    let container = container.clone();
                    s.spawn(move || {
                        let result = crate::runtime::snapshot::fetch_snapshot(paths, &container);
                        (container, result)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| match h.join() {
                    Ok(pair) => pair,
                    Err(panic_payload) => {
                        // Name the panic payload so the caller's
                        // debug_log routes it through the existing
                        // failure-logging path instead of silently
                        // dropping the slot.
                        let detail = panic_payload
                            .downcast_ref::<&'static str>()
                            .map(|s| (*s).to_string())
                            .or_else(|| panic_payload.downcast_ref::<String>().cloned())
                            .unwrap_or_else(|| "<non-string panic payload>".to_string());
                        (
                            "<unknown-container>".to_string(),
                            Err(anyhow::anyhow!("snapshot worker thread panicked: {detail}")),
                        )
                    }
                })
                .collect::<Vec<_>>()
        });
        results.extend(chunk_results);
    }
    results
}

impl<'a> EditorState<'a> {
    /// Open `child` as a sub-modal of the currently-visible modal. If
    /// a modal is already open it is stashed into `modal_parents`
    /// (top of vec = nearest parent); Esc on `child` will then call
    /// `pop_modal_chain` and restore the stashed parent. Use this for
    /// every modal→modal transition unless the parent's commit is
    /// terminal (in which case use `set_modal_terminal`).
    pub fn open_sub_modal(&mut self, child: Modal<'a>) {
        if let Some(parent) = self.modal.take() {
            self.modal_parents.push(parent);
        }
        self.modal = Some(child);
    }

    /// Pop one frame from the modal chain. If `modal_parents` is
    /// non-empty the previous parent becomes visible; otherwise the
    /// chain finishes and `modal` is cleared. Mirrors
    /// `crates/jackin-capsule/src/dialog.rs::dialog_pop_one` and is
    /// the canonical "Esc went back" arm for child modals.
    pub fn pop_modal_chain(&mut self) {
        self.modal = self.modal_parents.pop();
        if self.modal.is_none() {
            self.drop_modal_scratch();
        }
    }

    /// Terminal commit: clear `modal` and the entire `modal_parents`
    /// chain so the operator lands on the underlying tab in one step.
    /// Use on the final action of a multi-step flow (env key + value
    /// both committed, role + auth form saved, etc.).
    pub fn clear_modal_chain(&mut self) {
        self.modal = None;
        self.modal_parents.clear();
        self.drop_modal_scratch();
    }

    /// Scratch slots used to thread env-key + source-picker context
    /// across child modals (e.g. `EnvKey` → `SourcePicker` → `OpPicker`).
    /// Whenever the chain unwinds to no modal, these must clear so a
    /// later unrelated commit cannot pick up stale (scope, key) and
    /// write a secret to the wrong target.
    fn drop_modal_scratch(&mut self) {
        self.pending_env_key = None;
        self.pending_picker_value = None;
    }
}

impl EditorState<'_> {
    pub fn new_edit(name: String, ws: WorkspaceConfig) -> Self {
        Self {
            mode: EditorMode::Edit { name },
            active_tab: EditorTab::General,
            tab_bar_focused: true,
            hovered_tab: None,
            active_field: FieldFocus::Row(0),
            original: ws.clone(),
            pending: ws,
            modal: None,
            modal_parents: Vec::new(),
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
            hovered_mount_row: None,
            tab_scroll_x: 0,
            tab_scroll_y: 0,
            tab_content_scroll_focused: false,
            tab_content_width: 0,
            tab_content_height: 0,
            generating_token_target: None,
            pending_token_generate: None,
            cached_footer_h: 1,
        }
    }

    pub fn new_create() -> Self {
        let empty = WorkspaceConfig::default();
        Self {
            mode: EditorMode::Create,
            active_tab: EditorTab::General,
            tab_bar_focused: true,
            hovered_tab: None,
            active_field: FieldFocus::Row(0),
            original: empty.clone(),
            pending: empty,
            modal: None,
            modal_parents: Vec::new(),
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
            hovered_mount_row: None,
            tab_scroll_x: 0,
            tab_scroll_y: 0,
            tab_content_scroll_focused: false,
            tab_content_width: 0,
            tab_content_height: 0,
            generating_token_target: None,
            pending_token_generate: None,
            cached_footer_h: 1,
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

        state.instances_last_refresh = Some(std::time::Instant::now());
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
    fn refresh_instances_clears_on_index_error() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = crate::paths::JackinPaths::for_tests(tmp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        std::fs::write(paths.data_dir.join("instances.json"), b"not json").unwrap();
        let bogus = paths.data_dir.join("jackin-bogus-k7p9m2xq");
        std::fs::create_dir_all(bogus.join(".jackin")).unwrap();
        std::fs::write(bogus.join(".jackin/instance.json"), b"not json").unwrap();

        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.refresh_instances(&paths);

        assert!(state.instances.is_empty());
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
            let idx = row.to_screen_index(saved_count).unwrap();
            assert_eq!(state.row_at(idx), Some(row), "row_at({idx}) for {row:?}");
            state.selected = idx;
            assert_eq!(state.selected_row(), row, "selected_row for idx={idx}");
        }

        assert_eq!(
            ManagerListRow::NewWorkspace.to_visual_index(saved_count),
            Some(saved_count + 2)
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
        state.selected = ManagerListRow::CurrentDirectory.to_screen_index(1).unwrap();
        assert!(state.selected_workspace_summary().is_none());
        assert!(state.is_current_dir_selected());

        // Saved workspace row.
        state.selected = ManagerListRow::SavedWorkspace(0)
            .to_screen_index(1)
            .unwrap();
        let summary = state
            .selected_workspace_summary()
            .expect("saved row exposes summary");
        assert_eq!(summary.name, "alpha");

        // "+ New workspace" sentinel.
        state.selected = ManagerListRow::NewWorkspace.to_screen_index(1).unwrap();
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

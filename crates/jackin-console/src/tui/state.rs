// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Concrete console manager state and type bindings.
//!
//! `ManagerState` is the single central struct that the host console TUI
//! owns across its entire lifetime. All field types are lower-crate types
//! (from `jackin-core`, `jackin-config`, `jackin-env`, `jackin-protocol`,
//! `jackin-tui`, and this crate) so the root binary can depend on this
//! module without creating a circular dependency.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use ratatui::layout::Rect;

use jackin_config::{AppConfig, MountConfig, WorkspaceConfig};
use jackin_core::EnvValue;
use jackin_env::OpCache;
use jackin_tui::components::{
    ConfirmState, ContainerInfoState, ErrorPopupState, FocusOwner, TextInputState,
};
use jackin_tui::runtime::BlockingSubscription;

use crate::tui::auth::AuthKind;
use crate::tui::components::confirm_save::ConfirmSaveState;
use crate::tui::components::file_browser::FileBrowserState;
use crate::tui::components::github_picker::GithubPickerState;
use crate::tui::components::mount_dst_choice::MountDstChoiceState;
use crate::tui::components::provider_picker::ProviderPickerState as GenericProviderPickerState;
use crate::tui::components::scope_picker::ScopePickerState;
use crate::tui::components::source_picker::SourcePickerState;
use crate::tui::components::workdir_pick::WorkdirPickState;
use crate::tui::op_picker::OpPickerState;

pub use crate::mount_info_cache::MountInfoCache;
pub use crate::tui::focus::MountScrollFocus;
pub use crate::tui::model::SecretsPickerTarget;
pub use crate::tui::screens::editor::model::{
    AuthRow as GenericAuthRow, CreateStep, EditorHoverTarget, EditorMode, EditorTab, ExitIntent,
    FieldFocus, FileBrowserTarget, SecretsEnterPlan, SecretsRow, SecretsScopeTag, TextInputTarget,
};
pub use crate::tui::screens::settings::model::{
    AuthFormFocus, GlobalMountConfirm, GlobalMountDraft, GlobalMountTextTarget, SettingsEnvConfirm,
    SettingsEnvEnterPlan, SettingsEnvOpPickerTarget, SettingsEnvRow, SettingsEnvScope,
    SettingsEnvTextTarget, SettingsGeneralState, SettingsHoverTarget, SettingsTab,
    SettingsTrustRow, SettingsTrustState,
};
pub use crate::tui::screens::workspaces::model::{
    ManagerHoverTarget, ManagerListRow, WorkspaceSummary,
};
pub use crate::tui::split::{
    DEFAULT_SPLIT_PCT, DragState, MAX_SPLIT_PCT, MIN_SPLIT_PCT, clamp_split,
};

// ── Concrete effect type ────────────────────────────────────────────────────

/// Root effect vocabulary bound to concrete lower-crate types.
pub type ManagerEffect = crate::tui::effect::ConsoleManagerEffect<
    jackin_core::RoleSelector,
    jackin_config::RoleSource,
    jackin_core::OpRef,
>;

/// Concrete workspace-save effect parameterized with lower-crate types.
pub type WorkspaceSaveEffect = crate::tui::effect::WorkspaceSaveEffect<
    MountConfig,
    PendingSaveCommit,
    jackin_core::IsolationRecord,
    WorkspaceConfig,
>;

// ── Concrete refresh snapshot type ──────────────────────────────────────────

/// Concrete instance-refresh snapshot parameterized with lower-crate types.
///
/// The type alias lives here so both `ManagerState` field types and
/// the root binary's `ManagerMessage` binding can reference the same
/// concrete shape without spelling it out everywhere.
pub type ManagerInstanceRefreshSnapshot = crate::tui::subscriptions::InstanceRefreshSnapshot<
    jackin_core::InstanceIndexEntry,
    jackin_core::SessionRecord,
    jackin_protocol::InstanceSnapshot,
>;
pub type ManagerConfigSaveResult =
    crate::tui::subscriptions::ConfigSaveResult<AppConfig, jackin_config::RoleSource>;

// ── Type aliases ────────────────────────────────────────────────────────────

/// Provider picker bound to its follow-up context.
///
/// The context is whatever the next step needs: the target `container`
/// (existing-instance "new session" flow) or the `RoleSelector` (initial
/// workspace launch). Carries the resolved `Provider` list so a selection
/// cannot reference a provider/env pair that drifted from its label; the
/// index is clamped by `move_up` / `move_down` and read back through
/// `selected_provider`.
pub type ProviderPickerState<C> =
    GenericProviderPickerState<C, jackin_core::Agent, jackin_protocol::Provider>;
pub type AgentChoiceState =
    crate::tui::components::agent_choice::AgentChoiceState<jackin_core::Agent>;
pub type RolePickerState =
    crate::tui::components::role_picker::RolePickerState<jackin_core::RoleSelector>;

pub type ManagerStage<'a> = crate::tui::model::ConsoleManagerStage<
    CreatePreludeState<'a>,
    EditorState<'a>,
    SettingsState<'a>,
>;

pub type GlobalMountsState<'a> = crate::tui::screens::settings::model::GlobalMountsState<
    jackin_config::GlobalMountRow,
    SettingsModal<'a>,
>;

pub type SettingsState<'a> = crate::tui::screens::settings::model::SettingsState<
    GlobalMountsState<'a>,
    SettingsEnvState<'a>,
    SettingsAuthState,
    SettingsTrustState,
    ErrorPopupState,
    PendingTokenGenerate,
>;

pub type SettingsEnvConfig = crate::tui::screens::settings::model::SettingsEnvConfig<EnvValue>;

pub type PendingSaveCommit = crate::tui::screens::editor::model::PendingSaveCommit<MountConfig>;
pub type EditorSaveFlow = crate::tui::screens::editor::model::EditorSaveFlow<PendingSaveCommit>;
pub type AuthFormTarget = crate::tui::screens::settings::model::AuthFormTarget<AuthKind>;

pub type AuthForm = crate::tui::components::auth_panel::AuthForm<EnvValue>;
pub type AuthRow = GenericAuthRow<AuthKind>;
pub type SettingsAuthRow =
    crate::tui::screens::settings::model::SettingsAuthRow<AuthKind, crate::tui::auth::AuthMode>;
pub type ConfirmTarget =
    crate::tui::screens::editor::model::ConfirmTarget<jackin_config::RoleSource, PendingSaveCommit>;

pub type SettingsEnvState<'a> =
    crate::tui::screens::settings::model::SettingsEnvState<EnvValue, SettingsModal<'a>>;

pub type SettingsModal<'a> = crate::tui::screens::settings::model::SettingsModal<
    EnvValue,
    TextInputState<'a>,
    SourcePickerState,
    OpPickerState,
    FileBrowserState,
    MountDstChoiceState,
    RolePickerState,
    ScopePickerState,
    ConfirmState,
    ConfirmSaveState<MountConfig>,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
>;

pub type SettingsAuthState = crate::tui::screens::settings::model::SettingsAuthState<
    EnvValue,
    SettingsModal<'static>,
    PendingOpCommit,
>;

pub type PendingTokenGenerate = crate::tui::subscriptions::PendingTokenGenerate<
    jackin_env::TokenSetupScope,
    jackin_env::TokenSetupArgs,
>;

pub type EditorState<'a> = crate::tui::screens::editor::model::EditorState<
    MountInfoCache,
    Modal<'a>,
    EditorSaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>;

pub type PendingOpCommit = crate::tui::subscriptions::PendingOpCommit<jackin_core::OpRef>;

pub type PendingMountInfoRefresh = crate::tui::message::PendingMountInfoRefresh;

pub type MountInfoRefreshTarget = crate::tui::message::MountInfoRefreshTarget;

pub type PendingFileBrowserListing = crate::services::file_browser::FileBrowserListingResult;

pub type PendingFileBrowserCommit = crate::tui::file_browser::FileBrowserCommitResult;

pub type PendingDriftCheck =
    crate::tui::subscriptions::PendingDriftCheck<jackin_core::DriftDetection, PendingSaveCommit>;

pub type PendingIsolationCleanup =
    crate::tui::subscriptions::PendingIsolationCleanup<PendingSaveCommit>;

pub type PendingRoleLoad = crate::tui::subscriptions::PendingRoleLoad<jackin_config::RoleSource>;

pub type Modal<'a> = crate::tui::model::ConsoleModal<
    TextInputTarget,
    TextInputState<'a>,
    FileBrowserTarget,
    FileBrowserState,
    MountDstChoiceState,
    WorkdirPickState,
    ConfirmTarget,
    ConfirmState,
    jackin_tui::components::SaveDiscardState,
    GithubPickerState,
    ConfirmSaveState<MountConfig>,
    ErrorPopupState,
    ContainerInfoState,
    jackin_tui::components::StatusPopupState,
    OpPickerState,
    RolePickerState,
    SourcePickerState,
    ScopePickerState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
    SecretsScopeTag,
>;

pub type CreatePreludeState<'a> = crate::tui::model::ConsoleCreatePreludeState<Modal<'a>>;

// ── ManagerState ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ManagerState<'a> {
    pub stage: ManagerStage<'a>,
    pub workspaces: Vec<WorkspaceSummary>,
    pub instances: Vec<jackin_core::InstanceIndexEntry>,
    pub current_dir: String,
    pub selected: usize,
    /// Modal slot at the list level (e.g. `Modal::GithubPicker`); the
    /// Editor / `CreatePrelude` stages own their own modal slots.
    pub list_modal: Option<Modal<'a>>,
    /// Passive overlay drawn on top of `list_modal` for the duration of
    /// a single frame while a blocking async operation runs (currently
    /// the console role-resolution path). Input handlers do not see it.
    pub status_overlay: Option<jackin_tui::components::StatusPopupState>,
    pub inline_role_picker: Option<RolePickerState>,
    pub inline_agent_picker: Option<(jackin_core::RoleSelector, AgentChoiceState)>,
    /// Agent picker opened when the operator presses `N` on an instance row
    /// to start a new session in the running container. Carries the target
    /// `container_base`, the agent picker, and a provider list. The list is
    /// currently always empty: host config cannot prove which `ZAI_API_KEY`
    /// the already-running daemon captured, so provider choice for a running
    /// container is made in the multiplexer (daemon-owned), not here. The
    /// field stays so a future daemon-queried list can populate it.
    pub inline_new_session_picker:
        Option<(String, AgentChoiceState, Vec<jackin_protocol::Provider>)>,
    /// Provider picker shown after the agent is committed in
    /// `inline_new_session_picker` when its provider list has 2+ entries.
    /// Dormant while that list is always empty (see above); kept wired for
    /// the future daemon-queried flow. Context is the target `container`.
    pub inline_provider_picker: Option<ProviderPickerState<String>>,
    /// Provider picker for the initial workspace launch (before the container
    /// exists). Shown after the operator commits an agent choice and
    /// `ZAI_API_KEY` is configured. Context is the `RoleSelector`.
    pub launch_provider_picker: Option<ProviderPickerState<jackin_core::RoleSelector>>,
    pub list_mounts_scroll_x: u16,
    pub list_mounts_scroll_y: u16,
    pub list_global_mounts_scroll_x: u16,
    pub list_global_mounts_scroll_y: u16,
    pub list_role_global_mounts_scroll_x: u16,
    pub list_role_global_mounts_scroll_y: u16,
    pub list_roles_scroll_x: u16,
    pub list_roles_scroll_y: u16,
    pub list_focus_owner: FocusOwner<MountScrollFocus>,
    pub list_names_scroll_x: u16,
    pub list_names_scroll_y: u16,
    pub list_split_pct: u16,
    pub drag_state: Option<DragState>,
    pub hover_target: Option<ManagerHoverTarget>,
    pub mount_info_cache: MountInfoCache,
    /// Process-lifetime cache of `op` structural metadata, threaded
    /// into the picker on open. Carries no credentials — see
    /// `op_cache.rs`.
    pub op_cache: Rc<RefCell<OpCache>>,
    /// Mirrored from `ConsoleState::op_available` (probed once at
    /// startup) so the Secrets-tab editor can disable the
    /// source-picker's 1Password choice without re-probing.
    pub op_available: bool,
    /// Typed non-TUI work requested by input/update code. The root run loop
    /// drains and executes these outside the input dispatcher.
    pub(in crate::tui) pending_effects: Vec<ManagerEffect>,
    /// Last known terminal size, updated at the top of every render
    /// frame. Used by keyboard handlers to compute `viewport_h` for
    /// cursor-to-viewport scroll adjustment without needing a render pass.
    pub cached_term_size: Rect,
    /// Throttle the per-tick `InstanceIndex::read_or_rebuild` poll —
    /// state on disk can't change at the 20 Hz render cadence and the
    /// rebuild path walks every container directory.
    pub instances_last_refresh: Option<std::time::Instant>,
    pub(in crate::tui) instances_refresh_interval: std::time::Duration,
    pub(in crate::tui) instances_refresh_generation: u64,
    pub(in crate::tui) instances_refresh_rx:
        Option<BlockingSubscription<(u64, Result<ManagerInstanceRefreshSnapshot, String>)>>,
    pub(in crate::tui) mount_info_refresh_rx: Option<BlockingSubscription<PendingMountInfoRefresh>>,
    pub(in crate::tui) file_browser_listing_rx:
        Option<BlockingSubscription<PendingFileBrowserListing>>,
    pub(in crate::tui) file_browser_commit_rx:
        Option<BlockingSubscription<PendingFileBrowserCommit>>,
    pub(in crate::tui) config_save_rx: Option<BlockingSubscription<ManagerConfigSaveResult>>,
    /// Dedup gate: last error string from `refresh_instances`. Without
    /// this, a persistent parse error would reopen the popup on every
    /// 20 Hz tick — operators would never be able to dismiss it.
    pub(in crate::tui) instances_last_error: Option<String>,
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
    pub instance_sessions: HashMap<String, Vec<jackin_core::SessionRecord>>,
    /// Containers whose manifests could not be read during the last
    /// `refresh_instances` pass. Cleared on every successful index load.
    pub(in crate::tui) instance_session_errors: HashSet<String>,
    /// Live tab/pane snapshot per running instance keyed by
    /// `container_base`. Populated each `refresh_instances` tick by
    /// fetching from the daemon's bind-mounted socket at
    /// `~/.jackin/sockets/<container>/jackin.sock`. Missing keys mean
    /// the snapshot is unavailable (container not running, socket
    /// pre-dates the bind-mount, or the fetch failed).
    pub instance_snapshots: HashMap<String, jackin_protocol::InstanceSnapshot>,
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

// ── Impls ───────────────────────────────────────────────────────────────────

impl crate::tui::model::ConsoleManagerModalBlockPresence for ManagerState<'_> {
    fn list_modal_open(&self) -> bool {
        self.list_modal.is_some()
    }

    fn editor_modal_open(&self) -> bool {
        matches!(&self.stage, ManagerStage::Editor(editor) if editor.modal.is_some())
    }
}

/// Filter instances matching a query that are `Active` or `Running`.
pub fn active_instances_matching<'a>(
    instances: &'a [jackin_core::InstanceIndexEntry],
    query: jackin_core::InstanceQuery<'a>,
) -> impl Iterator<Item = &'a jackin_core::InstanceIndexEntry> {
    instances.iter().filter(move |e| {
        e.matches(query)
            && matches!(
                e.status,
                jackin_core::InstanceStatus::Active | jackin_core::InstanceStatus::Running
            )
    })
}

/// Filter instances matching a query that are visible in the console tree —
/// every status except `Purged` (no on-disk state) and `Superseded`
/// (replaced by a newer instance). Live and failed/stopped instances
/// alike appear so the operator can restore, restart, or delete them (D15).
pub fn visible_instances_matching<'a>(
    instances: &'a [jackin_core::InstanceIndexEntry],
    query: jackin_core::InstanceQuery<'a>,
) -> impl Iterator<Item = &'a jackin_core::InstanceIndexEntry> {
    instances.iter().filter(move |e| {
        e.matches(query)
            && !matches!(
                e.status,
                jackin_core::InstanceStatus::Purged | jackin_core::InstanceStatus::Superseded
            )
    })
}

/// Add a role to a workspace editor and select its row.
pub fn add_role_to_workspace_editor(editor: &mut EditorState<'_>, config: &AppConfig, key: &str) {
    if let Some(idx) = crate::tui::screens::editor::update::add_role_to_workspace_editor(
        &mut editor.pending.allowed_roles,
        config.roles.keys(),
        key,
    ) {
        editor.select_row(idx);
    }
}

/// Open the role trust confirm dialog.
pub fn open_role_trust_confirm(
    editor: &mut EditorState<'_>,
    key: String,
    source: jackin_config::RoleSource,
) {
    let state = crate::tui::screens::editor::view::role_trust_confirm_state(
        key.clone(),
        source.git.clone(),
    );
    editor.modal = Some(Modal::Confirm {
        target: ConfirmTarget::TrustRoleSource { key, source },
        state,
    });
}

/// Open an editor action error popup with the given error.
pub fn open_editor_action_error(editor: &mut EditorState<'_>, err: &dyn std::fmt::Display) {
    jackin_diagnostics::telemetry_debug!(
        "editor",
        "failed to apply confirmed editor action: {err}"
    );
    editor.open_error_popup(
        crate::tui::components::error_popup::editor_action_error_popup_state(err),
    );
}

/// Open a role-input error popup with the given message.
pub fn open_role_input_error(editor: &mut EditorState<'_>, message: &str) {
    jackin_diagnostics::telemetry_debug!("role", "showing direct role-load error popup: {message}");
    editor.open_error_popup(
        crate::tui::components::error_popup::role_load_error_popup_state(message),
    );
}

mod manager;
pub mod update;

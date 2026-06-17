//! Manager state machine for the jackin' console TUI.
//!
//! Central `ManagerState` struct and the `ManagerStage` enum that drives
//! which screen (workspaces list, editor, settings) is active. Also owns the
//! modal stack, subscription handles, and all transient UI state (selection,
//! draft edits, pending async work).
//!
//! Not responsible for: rendering (`jackin-console` and `jackin-tui` crates),
//! or side-effecting operations (those are `ManagerEffect` values returned by
//! input handlers in `console/tui/input/`).

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use ratatui::layout::Rect;

use crate::config::AppConfig;
use crate::console::domain::InstanceRefreshSnapshot;
use crate::console::tui::effect::ManagerEffect;
use crate::operator_env::OpCache;
use crate::workspace::{MountConfig, WorkspaceConfig};
use jackin_console::tui::auth::AuthKind;
use jackin_console::tui::auth_config::{
    app_github_env, panel_mode_requires_credential, resolve_panel_mode, role_override_present,
    settings_auth_rows_from_app_config,
};

use crate::console::tui::components::auth_panel::AuthForm;
use crate::console::tui::op_picker::OpPickerState;
use crate::selector::RolePickerState;
use jackin_console::tui::components::confirm_save::ConfirmSaveState;
use jackin_console::tui::components::file_browser::FileBrowserState;
use jackin_console::tui::components::github_picker::GithubPickerState;
use jackin_console::tui::components::mount_dst_choice::MountDstChoiceState;
use jackin_console::tui::components::provider_picker::ProviderPickerState as GenericProviderPickerState;
use jackin_console::tui::components::scope_picker::ScopePickerState;
use jackin_console::tui::components::source_picker::SourcePickerState;
use jackin_console::tui::components::workdir_pick::WorkdirPickState;
use jackin_tui::components::{
    ConfirmState, ContainerInfoState, ErrorPopupState, FocusOwner, TextInputState,
};
use jackin_tui::runtime::BlockingSubscription;

pub(crate) use jackin_console::mount_diff::classify_mount_diffs;
pub use jackin_console::mount_info_cache::MountInfoCache;
pub use jackin_console::tui::screens::workspaces::model::{ManagerListRow, WorkspaceSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerHoverTarget {
    ListRow(ManagerListRow),
}

// WorkspaceSummarySource impl for WorkspaceConfig now lives in jackin-console.

pub(crate) type MountDiff<'a> = jackin_console::mount_diff::MountDiff<'a, MountConfig>;

// MountDiffItem and MountSource impls for MountConfig and GlobalMountRow now live in jackin-console.

/// Provider picker bound to its follow-up context.
///
/// The context is whatever the next step needs: the target `container`
/// (existing-instance "new session" flow) or the `RoleSelector` (initial
/// workspace launch). Carries the resolved `Provider` list so a selection
/// cannot reference a provider/env pair that drifted from its label; the
/// index is clamped by `move_up` / `move_down` and read back through
/// `selected_provider`.
pub type ProviderPickerState<C> =
    GenericProviderPickerState<C, crate::agent::Agent, jackin_protocol::Provider>;

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
    pub status_overlay: Option<jackin_tui::components::StatusPopupState>,
    pub inline_role_picker: Option<RolePickerState>,
    pub inline_agent_picker: Option<(
        crate::selector::RoleSelector,
        crate::agent::AgentChoiceState,
    )>,
    /// Agent picker opened when the operator presses `N` on an instance row
    /// to start a new session in the running container. Carries the target
    /// `container_base`, the agent picker, and a provider list. The list is
    /// currently always empty: host config cannot prove which `ZAI_API_KEY`
    /// the already-running daemon captured, so provider choice for a running
    /// container is made in the multiplexer (daemon-owned), not here. The
    /// field stays so a future daemon-queried list can populate it.
    #[allow(clippy::type_complexity)]
    pub inline_new_session_picker: Option<(
        String,
        crate::agent::AgentChoiceState,
        Vec<jackin_protocol::Provider>,
    )>,
    /// Provider picker shown after the agent is committed in
    /// `inline_new_session_picker` when its provider list has 2+ entries.
    /// Dormant while that list is always empty (see above); kept wired for
    /// the future daemon-queried flow. Context is the target `container`.
    pub inline_provider_picker: Option<ProviderPickerState<String>>,
    /// Provider picker for the initial workspace launch (before the container
    /// exists). Shown after the operator commits an agent choice and
    /// `ZAI_API_KEY` is configured. Context is the `RoleSelector`.
    pub launch_provider_picker: Option<ProviderPickerState<crate::selector::RoleSelector>>,
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
    pending_effects: Vec<ManagerEffect>,
    /// Last known terminal size, updated at the top of every render
    /// frame. Used by keyboard handlers to compute `viewport_h` for
    /// cursor-to-viewport scroll adjustment without needing a render pass.
    pub cached_term_size: Rect,
    /// Throttle the per-tick `InstanceIndex::read_or_rebuild` poll —
    /// state on disk can't change at the 20 Hz render cadence and the
    /// rebuild path walks every container directory.
    instances_last_refresh: Option<std::time::Instant>,
    instances_refresh_generation: u64,
    instances_refresh_rx:
        Option<BlockingSubscription<(u64, Result<InstanceRefreshSnapshot, String>)>>,
    mount_info_refresh_rx: Option<BlockingSubscription<PendingMountInfoRefresh>>,
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

pub use jackin_console::tui::focus::MountScrollFocus;
pub use jackin_console::tui::split::{
    DEFAULT_SPLIT_PCT, DragState, MAX_SPLIT_PCT, MIN_SPLIT_PCT, clamp_split,
};

pub type ManagerStage<'a> = jackin_console::tui::app::ConsoleManagerStage<
    CreatePreludeState<'a>,
    EditorState<'a>,
    SettingsState<'a>,
>;

pub type GlobalMountsState<'a> = jackin_console::tui::screens::settings::model::GlobalMountsState<
    crate::config::GlobalMountRow,
    GlobalMountModal<'a>,
>;

pub type SettingsState<'a> = jackin_console::tui::screens::settings::model::SettingsState<
    GlobalMountsState<'a>,
    SettingsEnvState<'a>,
    SettingsAuthState,
    SettingsTrustState,
    ErrorPopupState,
    PendingTokenGenerate,
>;

pub use jackin_console::tui::screens::editor::model::{
    AuthRow as GenericAuthRow, CreateStep, EditorHoverTarget, EditorMode, EditorTab, ExitIntent,
    FieldFocus, FileBrowserTarget, SecretsEnterPlan, SecretsRow, SecretsScopeTag, TextInputTarget,
};
pub use jackin_console::tui::screens::settings::model::{
    AuthFormFocus, GlobalMountConfirm, GlobalMountDraft, GlobalMountTextTarget, SettingsEnvConfirm,
    SettingsEnvEnterPlan, SettingsEnvRow, SettingsEnvScope, SettingsEnvTextTarget,
    SettingsGeneralState, SettingsHoverTarget, SettingsTab, SettingsTrustRow, SettingsTrustState,
};
pub use jackin_console::tui::screens::settings::update::{
    settings_map_change_count, settings_vec_change_count,
};

pub type SettingsEnvConfig =
    jackin_console::tui::screens::settings::model::SettingsEnvConfig<crate::operator_env::EnvValue>;
pub type PendingSaveCommit =
    jackin_console::tui::screens::editor::model::PendingSaveCommit<MountConfig>;
pub type EditorSaveFlow =
    jackin_console::tui::screens::editor::model::EditorSaveFlow<PendingSaveCommit>;
pub type AuthFormTarget = jackin_console::tui::screens::settings::model::AuthFormTarget<AuthKind>;
pub type AuthRow = GenericAuthRow<AuthKind>;
pub type SettingsAuthRow = jackin_console::tui::screens::settings::model::SettingsAuthRow<
    AuthKind,
    jackin_console::tui::auth::AuthMode,
>;
pub type ConfirmTarget = jackin_console::tui::screens::editor::model::ConfirmTarget<
    crate::config::RoleSource,
    PendingSaveCommit,
>;

pub(crate) fn open_role_resolution_error(
    editor: &mut EditorState<'_>,
    raw: &str,
    source_url: Option<&String>,
    err: &anyhow::Error,
) {
    use jackin_console::tui::components::error_popup::{
        configured_role_load_error_message, repository_role_load_error_message,
    };
    crate::debug_log!(
        "role",
        "showing role-load error popup for raw={raw:?}: {err:?}"
    );
    let message = source_url.map_or_else(
        || configured_role_load_error_message(raw),
        |source_url| {
            repository_role_load_error_message(raw, source_url, friendly_role_resolution_error(err))
        },
    );
    editor.modal = Some(Modal::ErrorPopup {
        state: jackin_console::tui::components::error_popup::role_load_error_popup_state(message),
    });
}

pub(crate) fn open_editor_action_error(editor: &mut EditorState<'_>, err: &dyn std::fmt::Display) {
    crate::debug_log!("editor", "failed to apply confirmed editor action: {err}");
    editor.modal = Some(Modal::ErrorPopup {
        state: jackin_console::tui::components::error_popup::editor_action_error_popup_state(err),
    });
}

pub(crate) fn open_role_input_error(editor: &mut EditorState<'_>, message: &str) {
    crate::debug_log!("role", "showing direct role-load error popup: {message}");
    editor.modal = Some(Modal::ErrorPopup {
        state: jackin_console::tui::components::error_popup::role_load_error_popup_state(message),
    });
}

/// Translate a runtime role-resolution error into the operator-facing
/// blurb shown beneath the role-input dialog.
///
/// When adding a `RepoError` variant, add the corresponding match arm
/// here. Errors that were never wrapped as `RepoError` (e.g. fs/IO
/// errors raised before the clone) hit the fallback branch — generic
/// rather than mis-classified.
fn friendly_role_resolution_error(err: &anyhow::Error) -> String {
    use jackin_console::tui::components::error_popup::{
        generic_role_repository_error_message, invalid_role_repository_message,
        role_repository_remote_mismatch_message, role_repository_unavailable_message,
    };

    if let Some(repo_err) = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<crate::runtime::RepoError>())
    {
        return match repo_err {
            crate::runtime::RepoError::CloneFailed(_) => {
                role_repository_unavailable_message().into()
            }
            crate::runtime::RepoError::RemoteMismatch => {
                role_repository_remote_mismatch_message().into()
            }
            crate::runtime::RepoError::InvalidRoleRepo(detail) => {
                invalid_role_repository_message(humanize_invalid_role_repo(detail))
            }
        };
    }
    generic_role_repository_error_message().into()
}

/// Render a `RoleRepoValidationError` for the role-input popup.
///
/// `Missing(path)` is shown as the basename only — the full repo path
/// is operator-noise here since the popup already says which role they
/// asked for. Other variants fall back to the typed `Display` impl with
/// any trailing period trimmed (the surrounding sentence adds its own).
fn humanize_invalid_role_repo(err: &crate::repo::RoleRepoValidationError) -> String {
    use crate::repo::RoleRepoValidationError as V;
    match err {
        V::Missing(path) => {
            let file = path
                .file_name()
                .and_then(|name| name.to_str())
                .map_or_else(|| path.display().to_string(), str::to_owned);
            jackin_console::tui::components::error_popup::missing_role_repository_file_message(file)
        }
        _ => err.to_string().trim_end_matches('.').to_owned(),
    }
}

pub(crate) fn open_role_trust_confirm(
    editor: &mut EditorState<'_>,
    key: String,
    source: crate::config::RoleSource,
) {
    let state = jackin_console::tui::screens::editor::view::role_trust_confirm_state(
        key.clone(),
        source.git.clone(),
    );
    editor.modal = Some(Modal::Confirm {
        target: ConfirmTarget::TrustRoleSource { key, source },
        state,
    });
}

pub(crate) fn add_role_to_workspace_editor(
    editor: &mut EditorState<'_>,
    config: &AppConfig,
    key: &str,
) {
    if let Some(idx) = jackin_console::tui::screens::editor::update::add_role_to_workspace_editor(
        &mut editor.pending.allowed_roles,
        config.roles.keys(),
        key,
    ) {
        editor.active_field = FieldFocus::Row(idx);
    }
}

pub fn auth_flat_rows(editor: &EditorState<'_>, config: &AppConfig) -> Vec<AuthRow> {
    let synthesized = synthesize_appconfig_for_auth(editor, config);
    let ws_name = workspace_name_for_panel(editor);
    jackin_console::tui::screens::editor::update::auth_flat_rows(
        editor.auth_selected_kind,
        AuthKind::WORKSPACE_PANEL_KINDS.iter().copied(),
        &editor.pending.roles,
        editor.pending.allowed_roles.len(),
        &editor.auth_expanded,
        &jackin_console::tui::screens::editor::update::AuthFlatRowPredicates {
            role_override_present: &|kind, role| role_override_present(*kind, role),
            effective_mode_needs_credential: &|kind, role| {
                panel_mode_requires_credential(&synthesized, &ws_name, role, *kind)
            },
            effective_mode_supports_source_folder: &|kind, role| {
                let mode = resolve_panel_mode(&synthesized, *kind, &ws_name, role);
                jackin_console::tui::auth::auth_mode_supports_source_folder(*kind, mode)
            },
        },
    )
}

pub fn secrets_flat_rows(editor: &EditorState<'_>) -> Vec<SecretsRow> {
    jackin_console::tui::screens::editor::update::secrets_flat_rows(
        &editor.pending.env,
        &editor.pending.roles,
        &editor.secrets_expanded,
        |role| &role.env,
    )
}

pub fn settings_env_flat_rows(state: &SettingsState<'_>) -> Vec<SettingsEnvRow> {
    jackin_console::tui::screens::settings::update::settings_env_flat_rows(
        &state.env.pending,
        &state.env.expanded,
    )
}

/// Merge live global blocks with `editor.pending` for the active
/// workspace so the Auth panel renders pending edits before save.
pub(crate) fn synthesize_appconfig_for_auth(
    state: &EditorState<'_>,
    config: &AppConfig,
) -> AppConfig {
    jackin_console::tui::auth_config::synthesize_app_config_for_workspace_auth(
        config,
        workspace_name_for_panel(state),
        state.pending.clone(),
    )
}

/// Resolve the workspace key used by the Auth panel. In Edit mode this is
/// the existing workspace name; in Create mode we use `pending_name` if set,
/// otherwise a stable placeholder ("(new workspace)") so the panel can still
/// render with the pending values populated.
pub(crate) fn workspace_name_for_panel(state: &EditorState<'_>) -> String {
    jackin_console::tui::screens::editor::view::editor_name_value(
        &state.mode,
        state.pending_name.as_deref(),
        "(new workspace)",
    )
}

/// Map a flattened auth row index (the cursor) into the
/// `AuthFormTarget` the form modal should be opened against. Returns
/// `None` for non-form rows (`AuthKindRow`, source previews, `RoleHeader`,
/// `AddSentinel`, `Spacer`) so callers can dispatch them separately.
pub(crate) fn resolve_auth_row_target(
    state: &EditorState<'_>,
    config: &AppConfig,
    row: usize,
) -> Option<AuthFormTarget> {
    let rows = auth_flat_rows(state, config);
    jackin_console::tui::screens::editor::update::resolve_auth_form_target(&rows, row)
}

pub type SettingsEnvState<'a> = jackin_console::tui::screens::settings::model::SettingsEnvState<
    crate::operator_env::EnvValue,
    SettingsEnvModal<'a>,
>;

pub type SettingsEnvModal<'a> = jackin_console::tui::screens::settings::model::SettingsEnvModal<
    TextInputState<'a>,
    SourcePickerState,
    OpPickerState,
    RolePickerState,
    ScopePickerState,
    ConfirmState,
>;

pub type SettingsAuthState = jackin_console::tui::screens::settings::model::SettingsAuthState<
    crate::operator_env::EnvValue,
    SettingsAuthModal<'static>,
    PendingOpCommit,
>;

pub type SettingsAuthModal<'a> = jackin_console::tui::screens::settings::model::SettingsAuthModal<
    TextInputState<'a>,
    SourcePickerState,
    OpPickerState,
    FileBrowserState,
    AuthFormTarget,
    AuthForm,
    AuthFormFocus,
>;

pub type GlobalMountModal<'a> = jackin_console::tui::screens::settings::model::GlobalMountModal<
    TextInputState<'a>,
    FileBrowserState,
    MountDstChoiceState,
    ScopePickerState,
    RolePickerState,
    ConfirmState,
    ConfirmSaveState<MountConfig>,
>;

pub type PendingTokenGenerate = jackin_console::tui::subscriptions::PendingTokenGenerate<
    crate::workspace::token_setup::TokenSetupScope,
    crate::workspace::token_setup::TokenSetupArgs,
>;

pub type EditorState<'a> = jackin_console::tui::screens::editor::model::EditorState<
    WorkspaceConfig,
    MountInfoCache,
    Modal<'a>,
    EditorSaveFlow,
    crate::operator_env::EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>;

pub type PendingOpCommit =
    jackin_console::tui::subscriptions::PendingOpCommit<crate::operator_env::OpRef>;

pub(crate) type PendingMountInfoRefresh = jackin_console::tui::message::PendingMountInfoRefresh;

pub(crate) type MountInfoRefreshTarget = jackin_console::tui::message::MountInfoRefreshTarget;

pub type PendingDriftCheck = jackin_console::tui::subscriptions::PendingDriftCheck<
    crate::runtime::drift::DriftDetection,
    PendingSaveCommit,
>;

pub type PendingIsolationCleanup =
    jackin_console::tui::subscriptions::PendingIsolationCleanup<PendingSaveCommit>;

pub type PendingRoleLoad =
    jackin_console::tui::subscriptions::PendingRoleLoad<crate::config::RoleSource>;

fn settings_global_mounts_from_config(config: &AppConfig) -> GlobalMountsState<'static> {
    let rows = config.list_mount_rows();
    GlobalMountsState {
        selected: 0,
        pending: rows.clone(),
        original: rows,
        mount_info_cache: MountInfoCache::default(),
        modal: None,
        modal_parents: Vec::new(),
        add_draft: None,
        error: None,
        scroll_x: 0,
        scroll_y: 0,
        exit_requested: false,
    }
}

pub(crate) fn settings_state_from_config(config: &AppConfig) -> SettingsState<'static> {
    SettingsState {
        active_tab: SettingsTab::General,
        focus_owner: FocusOwner::TabBar,
        hover_target: None,
        general: SettingsGeneralState::from_values(config.git.coauthor_trailer, config.git.dco),
        mounts: settings_global_mounts_from_config(config),
        env: settings_env_from_config(config),
        auth: settings_auth_from_config(config),
        trust: settings_trust_from_config(config),
        error_popup: None,
        pending_token_generate: None,
        cached_footer_h: 1,
    }
}

pub(crate) trait SettingsStateExt {
    fn is_dirty(&self) -> bool;
    fn change_count(&self) -> usize;
    fn discard(&mut self);
    fn remove_zai_key_when_auth_ignored(&mut self);
    fn mark_saved(&mut self);
}

impl SettingsStateExt for SettingsState<'_> {
    fn is_dirty(&self) -> bool {
        self.general.is_dirty()
            || self.mounts.is_dirty()
            || self.env.is_dirty()
            || self.auth.is_dirty()
            || self.trust.is_dirty()
    }

    fn change_count(&self) -> usize {
        self.general.change_count()
            + settings_vec_change_count(&self.mounts.original, &self.mounts.pending)
            + self.env.change_count()
            + settings_vec_change_count(&self.auth.original, &self.auth.pending)
            + settings_map_change_count(&self.auth.original_github_env, &self.auth.github_env)
            + settings_vec_change_count(&self.trust.original, &self.trust.pending)
    }

    fn discard(&mut self) {
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

    fn remove_zai_key_when_auth_ignored(&mut self) {
        jackin_console::tui::auth_config::clear_ignored_env_only_settings_auth_keys(
            &self.auth.pending,
            &mut self.env.pending.env,
        );
    }

    fn mark_saved(&mut self) {
        self.general.mark_clean();
        self.mounts.original = self.mounts.pending.clone();
        self.env.original = self.env.pending.clone();
        self.auth.original = self.auth.pending.clone();
        self.auth.original_github_env = self.auth.github_env.clone();
        self.trust.original = self.trust.pending.clone();
    }
}

fn settings_env_from_config(config: &AppConfig) -> SettingsEnvState<'static> {
    let pending =
        jackin_console::tui::screens::settings::model::settings_env_config_from_app_config(config);
    SettingsEnvState {
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
    }
}

fn settings_auth_from_config(config: &AppConfig) -> SettingsAuthState {
    let github_env = app_github_env(config);
    let pending = settings_auth_rows_from_app_config(config);
    SettingsAuthState {
        selected: 0,
        selected_kind: None,
        original: pending.clone(),
        pending,
        github_env: github_env.clone(),
        original_github_env: github_env,
        modal: None,
        modal_parents: Vec::new(),
        generating_token: false,
        error: None,
        pending_op_commit: None,
        scroll_y: 0,
    }
}

fn settings_trust_from_config(config: &AppConfig) -> SettingsTrustState {
    let pending =
        jackin_console::tui::screens::settings::model::settings_trust_rows_from_app_config(config);
    SettingsTrustState::from_rows(pending)
}

pub type Modal<'a> = jackin_console::tui::app::ConsoleModal<
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

pub type CreatePreludeState<'a> = jackin_console::tui::app::ConsoleCreatePreludeState<Modal<'a>>;

// ── Impls ──────────────────────────────────────────────────────────

pub(crate) fn active_instances_matching<'a>(
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

mod manager;

pub trait EditorStateExt {
    fn is_dirty(&self) -> bool;
    fn change_count(&self) -> usize;
    fn cycle_isolation_for_selected_mount(&mut self);
}

impl EditorStateExt for EditorState<'_> {
    fn is_dirty(&self) -> bool {
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
    fn change_count(&self) -> usize {
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
        n += settings_map_change_count(&self.original.env, &self.pending.env);
        // Per-role overrides: union the keys; an role present on
        // only one side counts its whole env map / claude / codex /
        // github block as added/removed.
        let agent_keys: BTreeSet<&String> = self
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
            n += settings_map_change_count(orig_env, pend_env);
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
    fn cycle_isolation_for_selected_mount(&mut self) {
        let FieldFocus::Row(n) = self.active_field;
        jackin_console::tui::screens::editor::update::cycle_mount_isolation_at(
            &mut self.pending.mounts,
            n,
        );
    }
}

pub(crate) trait CreatePreludeWorkspaceExt {
    /// Produce the `WorkspaceConfig` for commit. Returns None if any
    /// required field is missing.
    fn build_workspace(&self) -> Option<WorkspaceConfig>;

    /// The wizard is complete iff a name, a mount source, a mount dst,
    /// and a workdir have all been captured. Returns the owned pair the
    /// dispatcher needs to transition to the editor.
    fn completed(&self) -> Option<(String, WorkspaceConfig)>;
}

impl CreatePreludeWorkspaceExt for CreatePreludeState<'_> {
    fn build_workspace(&self) -> Option<WorkspaceConfig> {
        let src = self.pending_mount_src.as_ref()?;
        let dst = self.pending_mount_dst.as_ref()?;
        let workdir = self.pending_workdir.as_ref()?;

        Some(WorkspaceConfig {
            workdir: workdir.clone(),
            mounts: vec![MountConfig {
                src: src.display().to_string(),
                dst: dst.clone(),
                readonly: self.pending_readonly,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..WorkspaceConfig::default()
        })
    }

    fn completed(&self) -> Option<(String, WorkspaceConfig)> {
        let name = self.pending_name.clone()?;
        let workspace = self.build_workspace()?;
        Some((name, workspace))
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

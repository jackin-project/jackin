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
use crate::console::domain::{
    InstanceRefreshSnapshot, app_github_env, eligible_role_keys_for_override,
    panel_mode_requires_credential, role_override_present,
};
use crate::console::tui::effect::ManagerEffect;
use crate::operator_env::OpCache;
use crate::workspace::{MountConfig, WorkspaceConfig};
use jackin_console::tui::auth::AuthKind;

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
use jackin_tui::components::{ConfirmState, ContainerInfoState, ErrorPopupState, TextInputState};
use jackin_tui::runtime::{BlockingSubscription, Subscription, SubscriptionPoll};

pub(crate) use jackin_console::mount_diff::classify_mount_diffs;
pub use jackin_console::mount_info_cache::MountInfoCache;
pub use jackin_console::tui::screens::workspaces::model::{ManagerListRow, WorkspaceSummary};

fn workspace_summary_from_config(
    name: &str,
    ws: &crate::workspace::WorkspaceConfig,
) -> WorkspaceSummary {
    WorkspaceSummary::from_source(name, ws)
}

// WorkspaceSummarySource impl for WorkspaceConfig now lives in jackin-console.

pub(crate) type MountDiff<'a> =
    jackin_console::mount_diff::MountDiff<'a, crate::workspace::MountConfig>;

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
    pub list_scroll_focus: Option<MountScrollFocus>,
    pub list_names_scroll_x: u16,
    pub list_names_focused: bool,
    pub list_split_pct: u16,
    pub drag_state: Option<DragState>,
    /// Logical list row the pointer is hovering (lifts its background like a
    /// hovered tab). Transient; set on mouse motion, cleared off the list.
    pub hovered_list_row: Option<ManagerListRow>,
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
    AuthRow as GenericAuthRow, CreateStep, EditorMode, EditorTab, ExitIntent, FieldFocus,
    FileBrowserTarget, SecretsEnterPlan, SecretsRow, SecretsScopeTag, TextInputTarget,
};
pub use jackin_console::tui::screens::settings::model::{
    AuthFormFocus, GlobalMountConfirm, GlobalMountDraft, GlobalMountTextTarget, SettingsEnvConfirm,
    SettingsEnvEnterPlan, SettingsEnvRow, SettingsEnvScope, SettingsEnvTextTarget,
    SettingsGeneralState, SettingsTab, SettingsTrustRow, SettingsTrustState,
};
pub use jackin_console::tui::screens::settings::update::{
    settings_map_change_count, settings_vec_change_count,
};

pub type SettingsEnvConfig =
    jackin_console::tui::screens::settings::model::SettingsEnvConfig<crate::operator_env::EnvValue>;
pub type PendingSaveCommit =
    jackin_console::tui::screens::editor::model::PendingSaveCommit<crate::workspace::MountConfig>;
pub type EditorSaveFlow =
    jackin_console::tui::screens::editor::model::EditorSaveFlow<PendingSaveCommit>;
pub type AuthFormTarget = jackin_console::tui::screens::settings::model::AuthFormTarget<
    jackin_console::tui::auth::AuthKind,
>;
pub type AuthRow = GenericAuthRow<jackin_console::tui::auth::AuthKind>;
pub type SettingsAuthRow = jackin_console::tui::screens::settings::model::SettingsAuthRow<
    jackin_console::tui::auth::AuthKind,
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
                .map_or_else(|| path.display().to_string(), str::to_string);
            jackin_console::tui::components::error_popup::missing_role_repository_file_message(file)
        }
        _ => err.to_string().trim_end_matches('.').to_string(),
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
    if !editor.pending.allowed_roles.is_empty()
        && !editor.pending.allowed_roles.iter().any(|role| role == key)
    {
        editor.pending.allowed_roles.push(key.to_string());
    }

    if let Some(idx) = config.roles.keys().position(|role| role == key) {
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
        |kind, role| role_override_present(*kind, role),
        |kind, role| panel_mode_requires_credential(&synthesized, &ws_name, role, *kind),
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
    settings_env_state_flat_rows(&state.env)
}

pub fn settings_env_state_flat_rows(state: &SettingsEnvState<'_>) -> Vec<SettingsEnvRow> {
    jackin_console::tui::screens::settings::update::settings_env_flat_rows(
        &state.pending,
        &state.expanded,
    )
}

pub(crate) fn eligible_agents_for_override(
    editor: &EditorState<'_>,
    config: &AppConfig,
) -> Vec<String> {
    eligible_role_keys_for_override(config, &editor.pending)
}

/// Merge live global blocks with `editor.pending` for the active
/// workspace so the Auth panel renders pending edits before save.
pub(crate) fn synthesize_appconfig_for_auth(
    state: &EditorState<'_>,
    config: &AppConfig,
) -> AppConfig {
    let mut synthesized = AppConfig {
        claude: config.claude.clone(),
        codex: config.codex.clone(),
        amp: config.amp.clone(),
        opencode: config.opencode.clone(),
        github: config.github.clone(),
        env: config.env.clone(),
        roles: config.roles.clone(),
        ..AppConfig::default()
    };
    let ws_name = workspace_name_for_panel(state);
    synthesized
        .workspaces
        .insert(ws_name, state.pending.clone());
    synthesized
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
/// `None` for non-form rows (`AuthKindRow`, `RoleHeader`, `AddSentinel`,
/// `Spacer`) so callers can dispatch them separately.
pub(crate) fn resolve_auth_row_target(
    state: &EditorState<'_>,
    config: &AppConfig,
    row: usize,
) -> Option<AuthFormTarget> {
    let rows = auth_flat_rows(state, config);
    match rows.get(row)? {
        AuthRow::WorkspaceMode { kind } | AuthRow::WorkspaceSource { kind } => {
            Some(AuthFormTarget::Workspace { kind: *kind })
        }
        AuthRow::RoleMode { role, kind } | AuthRow::RoleSource { role, kind } => {
            Some(AuthFormTarget::WorkspaceRole {
                role: role.clone(),
                kind: *kind,
            })
        }
        _ => None,
    }
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
    ConfirmSaveState<crate::workspace::MountConfig>,
>;

pub type PendingTokenGenerate = jackin_console::tui::subscriptions::PendingTokenGenerate<
    crate::workspace::token_setup::TokenSetupScope,
    crate::workspace::token_setup::TokenSetupArgs,
>;

pub(crate) fn token_generate_scope_label(
    req: &PendingTokenGenerate,
) -> jackin_console::tui::run::TokenGenerateScopeLabel<'_> {
    use crate::workspace::token_setup::TokenSetupScope;

    match &req.scope {
        TokenSetupScope::Workspace(name) => {
            jackin_console::tui::run::TokenGenerateScopeLabel::Workspace(name)
        }
        TokenSetupScope::WorkspaceRole { workspace, role } => {
            jackin_console::tui::run::TokenGenerateScopeLabel::WorkspaceRole { workspace, role }
        }
        TokenSetupScope::Global => jackin_console::tui::run::TokenGenerateScopeLabel::Global,
    }
}

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
        scroll_focused: false,
        exit_requested: false,
    }
}

pub(crate) fn settings_state_from_config(config: &AppConfig) -> SettingsState<'static> {
    SettingsState {
        active_tab: SettingsTab::General,
        tab_bar_focused: true,
        hovered_tab: None,
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
        for row in &self.auth.pending {
            // Remove the credential env var for env-only provider kinds when
            // the operator commits Ignore (meaning: no credential at global scope).
            if row.mode == jackin_console::tui::auth::AuthMode::Ignore
                && matches!(
                    row.kind,
                    jackin_console::tui::auth::AuthKind::Zai
                        | jackin_console::tui::auth::AuthKind::Minimax
                )
                && let Some(env_key) = row
                    .kind
                    .required_env_var(jackin_console::tui::auth::AuthMode::ApiKey)
            {
                self.env.pending.env.remove(env_key);
            }
        }
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
    let pending = SettingsEnvConfig {
        env: config.env.clone(),
        roles: config
            .roles
            .iter()
            .map(|(role, source)| (role.clone(), source.env.clone()))
            .collect(),
    };
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
        scroll_focused: false,
    }
}

fn settings_auth_from_config(config: &AppConfig) -> SettingsAuthState {
    let github_env = app_github_env(config);
    let pending = AuthKind::SETTINGS_KINDS
        .iter()
        .copied()
        .map(|kind| SettingsAuthRow {
            kind,
            mode: crate::console::domain::resolve_panel_mode(config, kind, "", ""),
        })
        .collect::<Vec<_>>();
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
        scroll_focused: false,
    }
}

fn settings_trust_from_config(config: &AppConfig) -> SettingsTrustState {
    let pending = config
        .roles
        .iter()
        .map(|(role, source)| SettingsTrustRow {
            role: role.clone(),
            git: source.git.clone(),
            trusted: source.trusted,
        })
        .collect::<Vec<_>>();
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
    ConfirmSaveState<crate::workspace::MountConfig>,
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

impl ManagerState<'_> {
    pub(in crate::console) const fn list_scroll_x_mut(
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

    pub(in crate::console) const fn list_scroll_y_mut(
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

    pub(in crate::console) const fn reset_list_scroll(&mut self) {
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
            .map(|(name, ws)| workspace_summary_from_config(name, ws))
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
            list_names_focused: true,
            list_split_pct: DEFAULT_SPLIT_PCT,
            drag_state: None,
            hovered_list_row: None,
            mount_info_cache: MountInfoCache::default(),
            op_cache,
            op_available,
            pending_effects: Vec::new(),
            cached_term_size: Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            },
            instances_last_refresh: None,
            instances_refresh_generation: 0,
            instances_refresh_rx: None,
            mount_info_refresh_rx: None,
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

    pub(crate) fn request_effect(&mut self, effect: ManagerEffect) {
        self.pending_effects.push(effect);
    }

    pub(crate) fn drain_effects(&mut self) -> Vec<ManagerEffect> {
        std::mem::take(&mut self.pending_effects)
    }

    #[allow(clippy::missing_const_for_fn)]
    pub(crate) fn take_pending_token_generate(&mut self) -> Option<PendingTokenGenerate> {
        match &mut self.stage {
            ManagerStage::Editor(editor) => editor.pending_token_generate.take(),
            ManagerStage::Settings(settings) => settings.pending_token_generate.take(),
            _ => None,
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
        let workspace_instance_counts = self.workspace_instance_counts();
        jackin_console::tui::screens::workspaces::update::selectable_rows(
            jackin_console::tui::screens::workspaces::update::WorkspaceRowLayout {
                current_dir_expanded: self.current_dir_expanded,
                current_dir_instance_count: self.current_dir_active_instances().len(),
                workspace_instance_counts: &workspace_instance_counts,
                expanded_workspaces: &self.expanded_workspaces,
            },
        )
    }

    /// Visual row list for rendering — same as `selectable_rows_vec` plus a
    /// `None` spacer before `NewWorkspace` when saved workspaces exist.
    pub fn visual_rows_vec(&self) -> Vec<Option<ManagerListRow>> {
        let workspace_instance_counts = self.workspace_instance_counts();
        jackin_console::tui::screens::workspaces::update::visual_rows(
            jackin_console::tui::screens::workspaces::update::WorkspaceRowLayout {
                current_dir_expanded: self.current_dir_expanded,
                current_dir_instance_count: self.current_dir_active_instances().len(),
                workspace_instance_counts: &workspace_instance_counts,
                expanded_workspaces: &self.expanded_workspaces,
            },
        )
    }

    fn workspace_instance_counts(&self) -> Vec<usize> {
        self.workspaces
            .iter()
            .enumerate()
            .map(|(i, _)| self.workspace_active_instances(i).len())
            .collect()
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

    pub(crate) fn poll_instance_refresh(
        &mut self,
    ) -> Option<Result<InstanceRefreshSnapshot, String>> {
        self.drain_instance_refresh()
    }

    pub(crate) fn next_instance_refresh_generation_if_due(&mut self) -> Option<u64> {
        const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
        if self.instances_refresh_rx.is_some() {
            return None;
        }
        let now = std::time::Instant::now();
        if let Some(last) = self.instances_last_refresh
            && now.duration_since(last) < REFRESH_INTERVAL
        {
            return None;
        }
        self.instances_last_refresh = Some(now);
        self.instances_refresh_generation = self.instances_refresh_generation.wrapping_add(1);
        Some(self.instances_refresh_generation)
    }

    #[cfg(test)]
    pub(crate) const fn instance_refresh_in_flight(&self) -> bool {
        self.instances_refresh_rx.is_some()
    }

    pub(crate) fn begin_instance_refresh(
        &mut self,
        rx: BlockingSubscription<(u64, Result<InstanceRefreshSnapshot, String>)>,
    ) {
        self.instances_refresh_rx = Some(rx);
    }

    pub(crate) const fn mount_info_refresh_in_flight(&self) -> bool {
        self.mount_info_refresh_rx.is_some()
    }

    pub(crate) fn begin_mount_info_refresh(
        &mut self,
        rx: BlockingSubscription<PendingMountInfoRefresh>,
    ) {
        self.mount_info_refresh_rx = Some(rx);
    }

    pub(crate) fn poll_mount_info_refresh(&mut self) -> Option<PendingMountInfoRefresh> {
        let rx = self.mount_info_refresh_rx.as_mut()?;
        let result = match rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => {
                self.mount_info_refresh_rx = None;
                return None;
            }
        };
        self.mount_info_refresh_rx = None;
        Some(result)
    }

    pub(in crate::console) fn apply_mount_info_refresh(
        &mut self,
        result: PendingMountInfoRefresh,
    ) -> bool {
        match result.target {
            MountInfoRefreshTarget::ManagerList => {
                self.mount_info_cache.store_entries(result.entries);
            }
            MountInfoRefreshTarget::Editor => {
                let ManagerStage::Editor(editor) = &mut self.stage else {
                    return false;
                };
                editor.mount_info_cache.store_entries(result.entries);
            }
            MountInfoRefreshTarget::SettingsMounts => {
                let ManagerStage::Settings(settings) = &mut self.stage else {
                    return false;
                };
                settings
                    .mounts
                    .mount_info_cache
                    .store_entries(result.entries);
            }
        }
        true
    }

    pub(crate) fn active_mount_info_sources(
        &self,
        config: &AppConfig,
    ) -> Option<(MountInfoRefreshTarget, Vec<String>)> {
        match &self.stage {
            ManagerStage::List => {
                let mut sources = BTreeSet::new();
                sources.insert(self.current_dir.clone());
                for workspace in config.workspaces.values() {
                    sources.extend(workspace.mounts.iter().map(|mount| mount.src.clone()));
                }
                sources.extend(
                    config
                        .list_mount_rows()
                        .into_iter()
                        .map(|row| row.mount.src),
                );
                Some((
                    MountInfoRefreshTarget::ManagerList,
                    sources.into_iter().collect(),
                ))
            }
            ManagerStage::Editor(editor) => {
                let sources = editor
                    .pending
                    .mounts
                    .iter()
                    .map(|mount| mount.src.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                (!sources.is_empty()).then_some((MountInfoRefreshTarget::Editor, sources))
            }
            ManagerStage::Settings(settings) => {
                let sources = settings
                    .mounts
                    .pending
                    .iter()
                    .map(|row| row.mount.src.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                (!sources.is_empty()).then_some((MountInfoRefreshTarget::SettingsMounts, sources))
            }
            ManagerStage::CreatePrelude(_)
            | ManagerStage::ConfirmDelete { .. }
            | ManagerStage::ConfirmInstancePurge { .. } => None,
        }
    }

    /// Poll the in-flight drift check started by a save operation.
    ///
    /// Returns `Some(check)` when the check has a result ready, taking
    /// ownership of the `PendingDriftCheck` so the caller can continue the
    /// save flow. Returns `None` when the check is still running or there is
    /// no pending check.
    pub(crate) fn poll_pending_drift_check(
        &mut self,
    ) -> Option<(
        PendingDriftCheck,
        anyhow::Result<crate::runtime::drift::DriftDetection>,
    )> {
        let ManagerStage::Editor(editor) = &mut self.stage else {
            return None;
        };
        let check = editor.pending_drift_check.as_mut()?;
        let result = match check.rx.poll_next() {
            SubscriptionPoll::Ready(result) => Some(result),
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => Some(Err(anyhow::anyhow!(
                jackin_console::tui::subscriptions::drift_check_worker_disconnected_message()
            ))),
        };
        let ManagerStage::Editor(editor) = &mut self.stage else {
            unreachable!()
        };
        let check = editor.pending_drift_check.take().expect("polled above");
        result.map(|r| (check, r))
    }

    pub(crate) fn poll_pending_isolation_cleanup(
        &mut self,
    ) -> Option<(PendingIsolationCleanup, anyhow::Result<()>)> {
        let ManagerStage::Editor(editor) = &mut self.stage else {
            return None;
        };
        let cleanup = editor.pending_isolation_cleanup.as_mut()?;
        let result = match cleanup.rx.poll_next() {
            SubscriptionPoll::Ready(result) => Some(result),
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => Some(Err(anyhow::anyhow!(
                jackin_console::tui::subscriptions::isolation_cleanup_worker_disconnected_message()
            ))),
        };
        let ManagerStage::Editor(editor) = &mut self.stage else {
            unreachable!()
        };
        let cleanup = editor
            .pending_isolation_cleanup
            .take()
            .expect("polled above");
        result.map(|r| (cleanup, r))
    }

    pub(crate) fn poll_pending_role_load(
        &mut self,
    ) -> Option<(PendingRoleLoad, anyhow::Result<()>)> {
        let ManagerStage::Editor(editor) = &mut self.stage else {
            return None;
        };
        let load = editor.pending_role_load.as_mut()?;
        let result = match load.rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => Err(anyhow::anyhow!(
                jackin_console::tui::subscriptions::role_loader_worker_disconnected_message()
            )),
        };
        let ManagerStage::Editor(editor) = &mut self.stage else {
            unreachable!()
        };
        let load = editor.pending_role_load.take().expect("polled above");
        Some((load, result))
    }

    /// Poll the in-flight 1Password op-ref read for the auth-form op picker commit.
    ///
    /// Returns `Some((op_ref, result, is_settings))` when the read has finished,
    /// taking ownership so the caller can apply it via `_committed` or `_failed`.
    /// `is_settings` is `true` when the pending commit belongs to the Settings
    /// auth state rather than the editor auth form.
    /// Returns `None` when the read is still in progress or no commit is pending.
    #[allow(clippy::collapsible_if)]
    pub(crate) fn poll_pending_op_commit(
        &mut self,
    ) -> Option<(crate::operator_env::OpRef, anyhow::Result<()>, bool)> {
        // Editor path.
        if let ManagerStage::Editor(editor) = &mut self.stage {
            if let Some(pending) = editor.pending_op_commit.as_mut() {
                let result = match pending.rx.poll_next() {
                    SubscriptionPoll::Ready(result) => Some(result),
                    SubscriptionPoll::Pending => None,
                    SubscriptionPoll::Closed => Some(Err(anyhow::anyhow!(
                        jackin_console::tui::subscriptions::op_read_worker_disconnected_message()
                    ))),
                };
                if result.is_some() {
                    let ManagerStage::Editor(editor) = &mut self.stage else {
                        unreachable!()
                    };
                    let pending = editor.pending_op_commit.take().expect("polled above");
                    return result.map(|r| (pending.op_ref, r, false));
                }
            }
        }
        // Settings path.
        if let ManagerStage::Settings(settings) = &mut self.stage {
            if let Some(pending) = settings.auth.pending_op_commit.as_mut() {
                let result = match pending.rx.poll_next() {
                    SubscriptionPoll::Ready(result) => Some(result),
                    SubscriptionPoll::Pending => None,
                    SubscriptionPoll::Closed => Some(Err(anyhow::anyhow!(
                        jackin_console::tui::subscriptions::op_read_worker_disconnected_message()
                    ))),
                };
                if result.is_some() {
                    let ManagerStage::Settings(settings) = &mut self.stage else {
                        unreachable!()
                    };
                    let pending = settings
                        .auth
                        .pending_op_commit
                        .take()
                        .expect("polled above");
                    return result.map(|r| (pending.op_ref, r, true));
                }
            }
        }
        None
    }

    fn drain_instance_refresh(&mut self) -> Option<Result<InstanceRefreshSnapshot, String>> {
        let rx = self.instances_refresh_rx.as_mut()?;
        match rx.poll_next() {
            SubscriptionPoll::Ready((generation, result)) => {
                self.instances_refresh_rx = None;
                if generation == self.instances_refresh_generation {
                    Some(result)
                } else {
                    None
                }
            }
            SubscriptionPoll::Pending => {
                // Worker still running — keep the receiver.
                None
            }
            SubscriptionPoll::Closed => {
                self.instances_refresh_rx = None;
                let message =
                    jackin_console::tui::subscriptions::instance_refresh_worker_disconnected_message();
                Some(Err(message.into()))
            }
        }
    }

    pub(crate) fn apply_instance_refresh(
        &mut self,
        result: Result<InstanceRefreshSnapshot, String>,
    ) {
        match result {
            Ok(snapshot) => self.apply_instance_refresh_snapshot(snapshot),
            Err(error) => self.apply_instance_refresh_error(&error),
        }
    }

    pub(crate) fn open_list_error_popup(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.list_modal = Some(Modal::ErrorPopup {
            state: jackin_console::tui::components::error_popup::error_popup_state(title, message),
        });
    }

    fn apply_instance_refresh_snapshot(&mut self, snapshot: InstanceRefreshSnapshot) {
        self.instances = snapshot.instances;
        self.instance_sessions = snapshot.sessions;
        self.instance_session_errors = snapshot.session_errors;
        self.instance_snapshots = snapshot.snapshots;
        self.instances_last_error = None;
        // Evict preview cursors keyed on containers that no longer have
        // a live snapshot, otherwise the map accumulates indefinitely
        // across container churn.
        self.preview_pane_cursor
            .retain(|key, _| self.instance_snapshots.contains_key(key));
        // Clamp `selected` after a refresh in case an instance row that
        // was selected has disappeared.
        let max = self.row_count().saturating_sub(1);
        self.selected = self.selected.min(max);
    }

    fn apply_instance_refresh_error(&mut self, error: &str) {
        self.instances.clear();
        self.instance_sessions.clear();
        self.instance_session_errors.clear();
        self.expanded_workspaces.clear();
        // Mirror the Ok-branch cleanup of the snapshot-derived
        // surfaces — without this they accumulate stale entries keyed
        // by container_base that no longer appears in the index, and
        // `current_dir_expanded` latched against an empty instance list
        // drifts the row count.
        self.instance_snapshots.clear();
        self.preview_pane_cursor.clear();
        self.current_dir_expanded = false;
        self.preview_focused = false;
        let message =
            jackin_console::tui::components::error_popup::instance_index_error_message(error);
        if self.instances_last_error.as_deref() != Some(&message) {
            self.open_list_error_popup(
                jackin_console::tui::components::error_popup::instance_index_error_title(),
                &message,
            );
            self.instances_last_error = Some(message);
        }
    }

    /// Force the next `refresh_instances` call to re-read disk regardless of
    /// the throttle interval. Use after an action mutates the on-disk
    /// instance index (Stop/Purge) so the next list draw reflects the new
    /// state immediately instead of waiting up to `REFRESH_INTERVAL`.
    pub fn force_refresh_instances(&mut self) {
        self.instances_last_refresh = None;
        self.instances_refresh_generation = self.instances_refresh_generation.wrapping_add(1);
        self.instances_refresh_rx = None;
    }

    /// Test helper: force the next `refresh_instances` call to hit disk
    /// regardless of the throttle interval.
    #[cfg(test)]
    pub fn force_refresh_instances_for_test(&mut self) {
        self.instances_last_refresh = None;
        self.instances_refresh_generation = self.instances_refresh_generation.wrapping_add(1);
        self.instances_refresh_rx = None;
    }

    pub(crate) fn tick_active_animation(&mut self) -> bool {
        let mut dirty = false;
        if let Some(Modal::OpPicker { state }) = self.list_modal.as_mut() {
            dirty |= state.tick();
        }
        match &mut self.stage {
            ManagerStage::Editor(editor) => {
                if let Some(Modal::OpPicker { state }) = editor.modal.as_mut() {
                    dirty |= state.tick();
                }
            }
            ManagerStage::Settings(settings) => {
                if let Some(SettingsEnvModal::OpPicker { state }) = settings.env.modal.as_mut() {
                    dirty |= state.tick();
                }
                if let Some(SettingsAuthModal::OpPicker { state }) = settings.auth.modal.as_mut() {
                    dirty |= state.tick();
                }
            }
            ManagerStage::List
            | ManagerStage::CreatePrelude(_)
            | ManagerStage::ConfirmDelete { .. }
            | ManagerStage::ConfirmInstancePurge { .. } => {}
        }
        dirty
    }
}

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

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Concrete console state type bindings.

use super::{
    AuthFormFocus, FileBrowserTarget, GenericAuthRow, MountInfoCache, SecretsScopeTag,
    SettingsTrustState, TextInputTarget, UsageScreenState,
};

use crate::tui::components::{ConfirmState, ErrorPopupState, TextInputState};

use jackin_config::{AppConfig, MountConfig, WorkspaceConfig};
use jackin_core::EnvValue;

use crate::tui::auth::AuthKind;
use crate::tui::components::account_picker::AccountPickerState as GenericAccountPickerState;
use crate::tui::components::confirm_save::ConfirmSaveState;
use crate::tui::components::container_info_surface::ContainerInfoState;
use crate::tui::components::file_browser::FileBrowserState;
use crate::tui::components::github_picker::GithubPickerState;
use crate::tui::components::mount_dst_choice::MountDstChoiceState;
use crate::tui::components::scope_picker::ScopePickerState;
use crate::tui::components::source_picker::SourcePickerState;
use crate::tui::components::workdir_pick::WorkdirPickState;
use crate::tui::op_picker::OpPickerState;

/// Console-owned Usage route state: the persistent screen plus its
/// visibility. Grouped so `ManagerState` stays under the excessive-bools
/// budget and the two fields move together.
#[derive(Debug, Default)]
pub struct UsageRouteState {
    /// Focus/scroll/refresh state, created on first open and kept alive so
    /// the heartbeat keeps refreshing while the route is offscreen.
    pub screen: Option<UsageScreenState>,
    /// Whether the Usage route is currently visible.
    pub visible: bool,
}

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
    crate::services::launch::LiveInstanceAdmission,
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
pub type AccountPickerState<C> =
    GenericAccountPickerState<C, jackin_core::Agent, crate::services::launch::AccountChoice>;
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
>;

pub type SettingsEnvConfig = crate::tui::screens::settings::model::SettingsEnvConfig<EnvValue>;

pub type PendingSaveCommit = crate::tui::screens::editor::model::PendingSaveCommit<MountConfig>;
pub type EditorSaveFlow = crate::tui::screens::editor::model::EditorSaveFlow<PendingSaveCommit>;
pub type AuthFormTarget = crate::tui::screens::settings::model::AuthFormTarget<AuthKind>;

pub type AuthForm = crate::tui::components::auth_panel::AuthForm<EnvValue>;
pub type AuthRow = GenericAuthRow<AuthKind>;
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

pub type EditorState<'a> = crate::tui::screens::editor::model::EditorState<
    MountInfoCache,
    Modal<'a>,
    EditorSaveFlow,
    EnvValue,
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
    crate::tui::components::SaveDiscardState,
    GithubPickerState,
    ConfirmSaveState<MountConfig>,
    ErrorPopupState,
    ContainerInfoState,
    crate::tui::components::StatusPopupState,
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

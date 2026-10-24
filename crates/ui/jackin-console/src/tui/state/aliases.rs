// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

#[derive(Debug, Default)]
pub struct UsageRouteState {
    pub screen: Option<UsageScreenState>,
    pub visible: bool,
    painted_pointer: Option<crate::tui::screens::usage::UsagePaintReceipt>,
}

impl UsageRouteState {
    /// Revoke any input authority from the most recently painted Usage frame.
    pub fn clear_painted_pointer(&mut self) {
        self.painted_pointer = None;
    }

    /// Publish a Usage receipt only when it still matches the live route state.
    pub fn publish_painted_pointer(
        &mut self,
        area: ratatui::layout::Rect,
        receipt: Option<crate::tui::screens::usage::UsagePaintReceipt>,
    ) {
        self.painted_pointer = receipt.filter(|receipt| {
            self.visible
                && self
                    .screen
                    .as_ref()
                    .is_some_and(|screen| receipt.matches(area, screen))
        });
    }

    pub(crate) fn painted_row_target(
        &self,
        area: ratatui::layout::Rect,
        x: u16,
        y: u16,
    ) -> Option<crate::tui::screens::usage::UsageRowTarget> {
        if !self.visible {
            return None;
        }
        let screen = self.screen.as_ref()?;
        let receipt = self.painted_pointer.as_ref()?;
        receipt
            .matches(area, screen)
            .then(|| receipt.target_at(x, y))
            .flatten()
    }

    pub(crate) fn painted_wheel_action(
        &self,
        area: ratatui::layout::Rect,
        x: u16,
        y: u16,
        direction: crate::tui::screens::usage::WheelDirection,
    ) -> Option<crate::tui::screens::usage::UsageWheelAction> {
        if !self.visible {
            return None;
        }
        let screen = self.screen.as_ref()?;
        self.painted_pointer
            .as_ref()?
            .wheel_action(screen, area, x, y, direction)
    }

    pub(crate) fn select_painted_row(
        &mut self,
        area: ratatui::layout::Rect,
        x: u16,
        y: u16,
    ) -> bool {
        let Some(target) = self.painted_row_target(area, x, y) else {
            return false;
        };
        let Some(screen) = self.screen.as_mut() else {
            return false;
        };
        let selected = screen.select_rendered_target(&target);
        if selected {
            self.clear_painted_pointer();
        }
        selected
    }

    pub(crate) fn apply_painted_wheel_action(
        &mut self,
        area: ratatui::layout::Rect,
        x: u16,
        y: u16,
        direction: crate::tui::screens::usage::WheelDirection,
    ) -> bool {
        let Some(action) = self.painted_wheel_action(area, x, y, direction) else {
            return false;
        };
        let Some(receipt) = self.painted_pointer.as_ref() else {
            return false;
        };
        let (painted_detail_scroll, maximum_detail_scroll) = receipt.detail_scroll_bounds();
        let Some(screen) = self.screen.as_mut() else {
            return false;
        };
        match action {
            crate::tui::screens::usage::UsageWheelAction::ScrollDetail(delta) => {
                screen.apply_painted_detail_wheel(
                    delta,
                    painted_detail_scroll,
                    maximum_detail_scroll,
                );
            }
            crate::tui::screens::usage::UsageWheelAction::MoveSelection(_) => {
                screen.apply_wheel_action(action);
            }
        }
        self.clear_painted_pointer();
        true
    }
}

pub type ManagerEffect = crate::tui::effect::ConsoleManagerEffect<
    jackin_core::RoleSelector,
    jackin_config::RoleSource,
    jackin_core::OpRef,
>;

pub type WorkspaceSaveEffect = crate::tui::effect::WorkspaceSaveEffect<
    MountConfig,
    PendingSaveCommit,
    jackin_core::IsolationRecord,
    WorkspaceConfig,
>;

pub type ManagerInstanceRefreshSnapshot = crate::tui::subscriptions::InstanceRefreshSnapshot<
    jackin_core::InstanceIndexEntry,
    jackin_core::SessionRecord,
    jackin_protocol::InstanceSnapshot,
    crate::services::launch::LiveInstanceAdmission,
>;
pub type ManagerConfigSaveResult =
    crate::tui::subscriptions::ConfigSaveResult<AppConfig, jackin_config::RoleSource>;

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

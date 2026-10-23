// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    AccountPickerState, AgentChoiceState, DragState, ManagerConfigSaveResult, ManagerEffect,
    ManagerHoverTarget, ManagerInstanceRefreshSnapshot, ManagerStage, Modal, MountInfoCache,
    MountScrollFocus, PendingFileBrowserCommit, PendingFileBrowserListing, PendingMountInfoRefresh,
    RolePickerState, UsageRouteState, UsageScreenState, WorkspaceSummary,
};
use ratatui::layout::Rect;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use crate::tui::focus::TabFocus;
use crate::tui::model::{
    ConsoleInputDispatchFacts, ConsoleInputDispatchPlan, ConsoleManagerInputPresence,
    ConsoleManagerStageRoute, ConsolePaintedInputOwner, ConsolePaintedInputOwnerFacts,
    console_painted_input_owner_plan, console_usage_surface_painted,
};
use crate::tui::runtime::BlockingSubscription;
use crate::tui::screens::workspaces::view::{
    WorkspaceSidebarFacts, WorkspaceSidebarPlan, workspace_sidebar_plan,
};

use jackin_env::OpCache;

#[derive(Debug)]
pub struct ManagerState<'a> {
    pub stage: ManagerStage<'a>,
    pub workspaces: Vec<WorkspaceSummary>,
    pub instances: Vec<jackin_core::InstanceIndexEntry>,
    pub current_dir: String,
    pub selected: usize,
    pub list_modal: Option<Modal<'a>>,
    pub status_overlay: Option<crate::tui::components::StatusPopupState>,
    pub keyboard_help: Option<termrock::widgets::KeyboardHelpState>,
    pub inline_role_picker: Option<RolePickerState>,
    pub inline_agent_picker: Option<(jackin_core::RoleSelector, AgentChoiceState)>,
    pub inline_new_session_picker: Option<(
        String,
        AgentChoiceState,
        Vec<crate::services::launch::AccountChoice>,
    )>,
    pub inline_account_picker: Option<AccountPickerState<String>>,
    pub launch_account_picker: Option<AccountPickerState<jackin_core::RoleSelector>>,
    pub list_mounts_scroll: termrock::widgets::ScrollAreaState,
    pub list_global_mounts_scroll: termrock::widgets::ScrollAreaState,
    pub list_role_global_mounts_scroll: termrock::widgets::ScrollAreaState,
    pub list_roles_scroll: termrock::widgets::ScrollAreaState,
    pub list_focus_owner: TabFocus<MountScrollFocus>,
    pub list_names_scroll: termrock::widgets::ScrollAreaState,
    pub list_split_pct: u16,
    pub drag_state: Option<DragState>,
    pub hover_target: Option<ManagerHoverTarget>,
    pub hover: termrock::interaction::HoverState<crate::tui::input::mouse::ConsoleHoverTarget>,
    pub mount_info_cache: MountInfoCache,
    pub op_cache: Rc<RefCell<OpCache>>,
    pub op_available: bool,
    pub(in crate::tui) pending_effects: Vec<ManagerEffect>,
    pub cached_term_size: Rect,
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
    pub(in crate::tui) account_scan_rx: Option<
        BlockingSubscription<(
            u64,
            Result<crate::tui::screens::settings::model::AccountScanOutcome, String>,
        )>,
    >,
    pub(in crate::tui) instances_last_error: Option<String>,
    pub expanded_workspaces: BTreeSet<usize>,
    pub current_dir_expanded: bool,
    pub instance_sessions: HashMap<String, Vec<jackin_core::SessionRecord>>,
    pub(in crate::tui) instance_session_errors: HashSet<String>,
    pub live_instance_admissions:
        HashMap<String, Vec<crate::services::launch::LiveInstanceAdmission>>,
    pub instance_snapshots: HashMap<String, jackin_protocol::InstanceSnapshot>,
    pub preview_focused: bool,
    pub preview_pane_cursor: HashMap<String, usize>,
    pub usage: UsageRouteState,
    pub usage_snapshot: UsageScreenState,
}

impl ConsoleManagerInputPresence for ManagerState<'_> {
    fn allows_base_surface_input(&self) -> bool {
        match self.painted_input_owner() {
            ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::Stage(route)) => {
                match route {
                    ConsoleManagerStageRoute::List
                    | ConsoleManagerStageRoute::Editor
                    | ConsoleManagerStageRoute::Settings
                    | ConsoleManagerStageRoute::CreatePrelude => true,
                    ConsoleManagerStageRoute::Workflows => {
                        matches!(&self.stage, ManagerStage::Workflows(screen) if screen.confirm().is_none())
                    }
                    ConsoleManagerStageRoute::ConfirmDelete
                    | ConsoleManagerStageRoute::ConfirmInstancePurge => false,
                }
            }
            ConsolePaintedInputOwner::KeyboardHelp
            | ConsolePaintedInputOwner::StatusPopup
            | ConsolePaintedInputOwner::Usage
            | ConsolePaintedInputOwner::Dispatch(_) => false,
        }
    }
}

impl ManagerState<'_> {
    #[must_use]
    pub fn painted_input_owner(&self) -> ConsolePaintedInputOwner {
        let stage_route = self.stage.route();
        let usage_surface_painted = console_usage_surface_painted(stage_route, self.usage.visible);
        let sidebar_plan =
            if matches!(stage_route, ConsoleManagerStageRoute::List) && !usage_surface_painted {
                workspace_sidebar_plan(WorkspaceSidebarFacts {
                    inline_account_picker_open: self.inline_account_picker.is_some(),
                    launch_account_picker_open: self.launch_account_picker.is_some(),
                    inline_new_session_picker_open: self.inline_new_session_picker.is_some(),
                    inline_agent_picker_open: self.inline_agent_picker.is_some(),
                    inline_role_picker_open: self.inline_role_picker.is_some(),
                })
            } else {
                WorkspaceSidebarPlan::ListNames
            };
        let list_modal_painted =
            matches!(stage_route, ConsoleManagerStageRoute::List) && self.list_modal.is_some();
        let modal_facts = self.stage.modal_facts();
        let dispatch = ConsoleInputDispatchFacts {
            keyboard_help_open: self.keyboard_help.is_some(),
            list_modal_open: list_modal_painted,
            inline_new_session_picker_open: matches!(
                sidebar_plan,
                WorkspaceSidebarPlan::InlineNewSessionPicker
            ),
            inline_account_picker_open: matches!(
                sidebar_plan,
                WorkspaceSidebarPlan::InlineAccountPicker
            ),
            launch_account_picker_open: matches!(
                sidebar_plan,
                WorkspaceSidebarPlan::LaunchAccountPicker
            ),
            inline_agent_picker_open: matches!(
                sidebar_plan,
                WorkspaceSidebarPlan::InlineAgentPicker
            ),
            inline_role_picker_open: matches!(sidebar_plan, WorkspaceSidebarPlan::InlineRolePicker),
            editor_modal_open: modal_facts.editor_modal_open,
            settings_error_popup_open: modal_facts.settings_error_popup_open,
            settings_mounts_modal_open: modal_facts.settings_mounts_modal_open,
            settings_env_modal_open: modal_facts.settings_env_modal_open,
            settings_auth_modal_open: modal_facts.settings_auth_modal_open,
            create_prelude_modal_open: modal_facts.create_prelude_modal_open,
            stage_route,
        };

        console_painted_input_owner_plan(ConsolePaintedInputOwnerFacts {
            dispatch,
            status_popup_painted: self.status_overlay.is_some(),
            usage_surface_painted,
        })
    }

    pub fn clear_pointer_state(&mut self) {
        self.drag_state = None;
        self.hover_target = None;
        self.hover = termrock::interaction::HoverState::default();
        match &mut self.stage {
            ManagerStage::Editor(editor) => editor.set_hover_target(None),
            ManagerStage::Settings(settings) => settings.set_hover_target(None),
            ManagerStage::List
            | ManagerStage::CreatePrelude(_)
            | ManagerStage::Workflows(_)
            | ManagerStage::ConfirmDelete { .. }
            | ManagerStage::ConfirmInstancePurge { .. } => {}
        }
        if let Some(Modal::ContainerInfo { state: info }) = self.list_modal.as_mut() {
            info.set_hovered_row(None);
        }
    }
}

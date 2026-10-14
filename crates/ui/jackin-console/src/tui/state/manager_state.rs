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
use crate::tui::runtime::BlockingSubscription;

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

impl crate::tui::model::ConsoleManagerModalBlockPresence for ManagerState<'_> {
    fn list_modal_open(&self) -> bool {
        self.list_modal.is_some()
    }

    fn editor_modal_open(&self) -> bool {
        matches!(&self.stage, ManagerStage::Editor(editor) if editor.modal.is_some())
    }
}

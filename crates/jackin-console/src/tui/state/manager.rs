use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use ratatui::layout::Rect;

use jackin_config::AppConfig;
use jackin_tui::components::FocusOwner;
use jackin_tui::runtime::{BlockingSubscription, Subscription, SubscriptionPoll};

use crate::tui::app::{
    ConsoleAnimationTick, ConsoleManagerStageState, LaunchAgentPromptManagerState,
    LaunchProviderPickerManagerState, LaunchRolePromptManagerState,
};
use crate::tui::message::{MountInfoRefreshSourceFacts, mount_info_refresh_source_plan};
use crate::tui::screens::workspaces::model::hovered_list_row;
use crate::tui::screens::workspaces::update::{
    PreviewFocusState, PreviewPaneCursorState, WorkspaceListHoverState,
    WorkspaceListSelectionState, WorkspaceTreeDisclosureState,
    collapsed_current_dir_selected_index, collapsed_workspace_selected_index,
    initial_workspace_selected_index, preview_pane_selected_index, selected_index,
    workspace_last_selectable_index, workspace_list_current_directory_selected,
    workspace_list_new_workspace_selected, workspace_list_saved_workspace_index, workspace_row_at,
    workspace_row_at_visual_index, workspace_row_index, workspace_selected_row,
    workspace_visual_selected_index,
};
use crate::tui::subscriptions::{
    InstanceRefreshThrottleState, forced_instance_refresh_generation,
    instance_refresh_throttle_plan,
};
use crate::tui::update::{
    InlineNewSessionPickerState, InlinePickerDismissalState, InlineProviderPickerState,
    ListModalState, ListShellState, StatusOverlayState,
};
use jackin_env::OpCache;

use super::{
    DEFAULT_SPLIT_PCT, ManagerConfigSaveResult, ManagerEffect, ManagerInstanceRefreshSnapshot,
    ManagerListRow, ManagerStage, ManagerState, Modal, MountInfoCache, MountInfoRefreshTarget,
    MountScrollFocus, PendingDriftCheck, PendingFileBrowserCommit, PendingFileBrowserListing,
    PendingIsolationCleanup, PendingMountInfoRefresh, PendingRoleLoad, PendingTokenGenerate,
    WorkspaceSummary, active_instances_matching,
};

impl ManagerState<'_> {
    pub const fn list_scroll_x_mut(&mut self, focus: MountScrollFocus) -> &mut u16 {
        match focus {
            MountScrollFocus::Workspace => &mut self.list_mounts_scroll_x,
            MountScrollFocus::Global => &mut self.list_global_mounts_scroll_x,
            MountScrollFocus::RoleGlobal => &mut self.list_role_global_mounts_scroll_x,
            MountScrollFocus::Roles => &mut self.list_roles_scroll_x,
        }
    }

    pub const fn list_scroll_y_mut(&mut self, focus: MountScrollFocus) -> &mut u16 {
        match focus {
            MountScrollFocus::Workspace => &mut self.list_mounts_scroll_y,
            MountScrollFocus::Global => &mut self.list_global_mounts_scroll_y,
            MountScrollFocus::RoleGlobal => &mut self.list_role_global_mounts_scroll_y,
            MountScrollFocus::Roles => &mut self.list_roles_scroll_y,
        }
    }

    pub const fn reset_list_scroll(&mut self) {
        self.list_mounts_scroll_x = 0;
        self.list_mounts_scroll_y = 0;
        self.list_global_mounts_scroll_x = 0;
        self.list_global_mounts_scroll_y = 0;
        self.list_role_global_mounts_scroll_x = 0;
        self.list_role_global_mounts_scroll_y = 0;
        self.list_roles_scroll_x = 0;
        self.list_roles_scroll_y = 0;
        self.list_focus_owner = FocusOwner::TabBar;
        self.list_names_scroll_x = 0;
        self.list_names_scroll_y = 0;
    }

    pub const fn list_names_focused(&self) -> bool {
        self.list_focus_owner.is_tab_bar()
    }

    pub fn set_list_names_focused(&mut self, focused: bool) {
        if focused {
            self.list_focus_owner = FocusOwner::TabBar;
        } else if self.list_names_focused() {
            self.list_focus_owner = FocusOwner::Content(MountScrollFocus::Workspace);
        }
    }

    pub const fn list_scroll_focus(&self) -> Option<MountScrollFocus> {
        match self.list_focus_owner {
            FocusOwner::Content(focus) => Some(focus),
            FocusOwner::TabBar => None,
        }
    }

    pub fn set_list_scroll_focus(&mut self, focus: Option<MountScrollFocus>) {
        self.list_focus_owner = focus.map_or(FocusOwner::TabBar, FocusOwner::Content);
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
            .map(|(name, ws)| WorkspaceSummary::from_source(name, ws))
            .collect();

        let saved_count = workspaces.len();
        let matching_saved = jackin_config::find_saved_workspace_for_cwd(config, cwd)
            .and_then(|(name, _)| workspaces.iter().position(|w| w.name == name));
        let selected = initial_workspace_selected_index(saved_count, matching_saved);

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
            list_focus_owner: FocusOwner::TabBar,
            list_names_scroll_x: 0,
            list_names_scroll_y: 0,
            list_split_pct: DEFAULT_SPLIT_PCT,
            drag_state: None,
            hover_target: None,
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
            file_browser_listing_rx: None,
            file_browser_commit_rx: None,
            config_save_rx: None,
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

    pub fn request_effect(&mut self, effect: ManagerEffect) {
        self.pending_effects.push(effect);
    }

    pub fn drain_effects(&mut self) -> Vec<ManagerEffect> {
        std::mem::take(&mut self.pending_effects)
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn take_pending_token_generate(&mut self) -> Option<PendingTokenGenerate> {
        self.stage.take_pending_token_generate()
    }

    // ── Tree navigation helpers ────────────────────────────────────

    /// Instances that appear in the tree for workspace `ws_idx` — only
    /// `Active` / `Running` containers are shown.
    #[must_use]
    pub fn workspace_active_instances(
        &self,
        ws_idx: usize,
    ) -> Vec<&jackin_core::instance::InstanceIndexEntry> {
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return Vec::new();
        };
        let query = jackin_core::instance::InstanceQuery {
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
        let query = jackin_core::instance::InstanceQuery {
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
        let query = jackin_core::instance::InstanceQuery {
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
    pub fn current_dir_active_instances(&self) -> Vec<&jackin_core::instance::InstanceIndexEntry> {
        let current_dir = self.current_dir.as_str();
        let query = jackin_core::instance::InstanceQuery {
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
        crate::tui::screens::workspaces::update::selectable_rows(
            crate::tui::screens::workspaces::update::WorkspaceRowLayout {
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
        crate::tui::screens::workspaces::update::visual_rows(
            crate::tui::screens::workspaces::update::WorkspaceRowLayout {
                current_dir_expanded: self.current_dir_expanded,
                current_dir_instance_count: self.current_dir_active_instances().len(),
                workspace_instance_counts: &workspace_instance_counts,
                expanded_workspaces: &self.expanded_workspaces,
            },
        )
    }

    #[must_use]
    pub const fn hovered_list_row(&self) -> Option<ManagerListRow> {
        hovered_list_row(self.hover_target)
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
        workspace_row_index(&self.selectable_rows_vec(), row)
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
        workspace_last_selectable_index(self.selectable_rows_vec().len())
    }

    /// Decode a selectable-list index into a [`ManagerListRow`].
    #[must_use]
    pub fn row_at(&self, idx: usize) -> Option<ManagerListRow> {
        workspace_row_at(&self.selectable_rows_vec(), idx)
    }

    /// Decode a visual-list index (may include the non-selectable spacer)
    /// into a [`ManagerListRow`]. Returns `None` for the spacer row.
    #[must_use]
    pub fn row_at_visual_index(&self, idx: usize) -> Option<ManagerListRow> {
        workspace_row_at_visual_index(&self.visual_rows_vec(), idx)
    }

    /// Visual-list index of the currently selected row (for ratatui
    /// highlight). Differs from `selected` when instance rows are visible.
    #[must_use]
    pub fn visual_selected(&self) -> usize {
        let selected = self.selected_row();
        let visual_rows = self.visual_rows_vec();
        workspace_visual_selected_index(&visual_rows, selected).unwrap_or_else(|| {
            jackin_diagnostics::debug_log!(
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
        workspace_selected_row(&self.selectable_rows_vec(), self.selected)
    }

    /// Convenience: `true` when the selection is on the synthetic
    /// "Current directory" row.
    #[must_use]
    pub fn is_current_dir_selected(&self) -> bool {
        workspace_list_current_directory_selected(self.selected_row())
    }

    /// Convenience: `true` when the selection is on the "+ New workspace"
    /// sentinel.
    #[must_use]
    pub fn is_new_workspace_selected(&self) -> bool {
        workspace_list_new_workspace_selected(self.selected_row())
    }

    /// Whether the workspace tree node at `ws_idx` is expanded.
    #[must_use]
    pub fn is_workspace_expanded(&self, ws_idx: usize) -> bool {
        self.expanded_workspaces.contains(&ws_idx)
    }

    /// Recorded sessions for `container_base`, or an empty slice when none
    /// are cached (no sessions or manifest not yet loaded).
    #[must_use]
    pub fn sessions_for_instance(
        &self,
        container_base: &str,
    ) -> &[jackin_core::instance::SessionRecord] {
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
    ) -> Option<&jackin_protocol::InstanceSnapshot> {
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
        let cursor = preview_pane_selected_index(
            panes.len(),
            self.preview_pane_cursor.get(container_base).copied(),
        )?;
        panes.get(cursor).copied()
    }

    /// The [`WorkspaceSummary`] currently highlighted, or `None` when the
    /// selection is on Current Directory, New Workspace, or a `WorkspaceInstance`.
    #[must_use]
    pub fn selected_workspace_summary(&self) -> Option<&WorkspaceSummary> {
        workspace_list_saved_workspace_index(self.selected_row())
            .and_then(|i| self.workspaces.get(i))
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
        let selected = collapsed_current_dir_selected_index(self.selected_row());
        self.current_dir_expanded = false;
        if let Some(selected) = selected {
            self.selected = selected;
        }
    }

    /// Collapse the workspace tree node at `ws_idx`. When the cursor is
    /// on a child instance row, jumps up to the workspace row.
    pub fn collapse_workspace(&mut self, ws_idx: usize) {
        if !self.expanded_workspaces.contains(&ws_idx) {
            return;
        }
        let selected_row = self.selected_row();
        self.expanded_workspaces.remove(&ws_idx);
        let rows = self.selectable_rows_vec();
        self.selected =
            collapsed_workspace_selected_index(&rows, self.selected, selected_row, ws_idx)
                .unwrap_or_else(|| {
                    jackin_diagnostics::debug_log!(
                        "console",
                        "collapse_workspace: ws_idx={ws_idx} not in selectable rows, clamping to 0"
                    );
                    0 // CurrentDirectory is always row 0 and is never removed
                });
    }

    pub fn poll_instance_refresh(
        &mut self,
    ) -> Option<Result<ManagerInstanceRefreshSnapshot, String>> {
        self.drain_instance_refresh()
    }

    pub fn next_instance_refresh_generation_if_due(&mut self) -> Option<u64> {
        let now = std::time::Instant::now();
        let plan = instance_refresh_throttle_plan(
            InstanceRefreshThrottleState {
                in_flight: self.instances_refresh_rx.is_some(),
                last_refresh: self.instances_last_refresh,
                generation: self.instances_refresh_generation,
            },
            now,
        );
        self.instances_last_refresh = plan.last_refresh;
        self.instances_refresh_generation = plan.generation;
        plan.start_generation
    }

    pub const fn instance_refresh_in_flight(&self) -> bool {
        self.instances_refresh_rx.is_some()
    }

    pub fn begin_instance_refresh(
        &mut self,
        rx: BlockingSubscription<(u64, Result<ManagerInstanceRefreshSnapshot, String>)>,
    ) {
        self.instances_refresh_rx = Some(rx);
    }

    pub const fn mount_info_refresh_in_flight(&self) -> bool {
        self.mount_info_refresh_rx.is_some()
    }

    pub fn begin_mount_info_refresh(&mut self, rx: BlockingSubscription<PendingMountInfoRefresh>) {
        self.mount_info_refresh_rx = Some(rx);
    }

    pub fn begin_file_browser_listing(
        &mut self,
        rx: BlockingSubscription<PendingFileBrowserListing>,
    ) {
        self.file_browser_listing_rx = Some(rx);
    }

    pub const fn file_browser_listing_in_flight(&self) -> bool {
        self.file_browser_listing_rx.is_some()
    }

    pub fn begin_file_browser_commit(
        &mut self,
        rx: BlockingSubscription<PendingFileBrowserCommit>,
    ) {
        self.file_browser_commit_rx = Some(rx);
    }

    pub const fn file_browser_commit_in_flight(&self) -> bool {
        self.file_browser_commit_rx.is_some()
    }

    pub fn begin_config_save(&mut self, rx: BlockingSubscription<ManagerConfigSaveResult>) {
        self.config_save_rx = Some(rx);
    }

    pub const fn config_save_in_flight(&self) -> bool {
        self.config_save_rx.is_some()
    }

    pub fn poll_mount_info_refresh(&mut self) -> Option<PendingMountInfoRefresh> {
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

    pub fn poll_file_browser_listing(&mut self) -> Option<PendingFileBrowserListing> {
        let rx = self.file_browser_listing_rx.as_mut()?;
        let result = match rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => {
                self.file_browser_listing_rx = None;
                return None;
            }
        };
        self.file_browser_listing_rx = None;
        Some(result)
    }

    pub fn poll_file_browser_commit(&mut self) -> Option<PendingFileBrowserCommit> {
        let rx = self.file_browser_commit_rx.as_mut()?;
        let result = match rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => {
                self.file_browser_commit_rx = None;
                return None;
            }
        };
        self.file_browser_commit_rx = None;
        Some(result)
    }

    pub fn poll_config_save(&mut self) -> Option<ManagerConfigSaveResult> {
        let rx = self.config_save_rx.as_mut()?;
        let result = match rx.poll_next() {
            SubscriptionPoll::Ready(result) => result,
            SubscriptionPoll::Pending => return None,
            SubscriptionPoll::Closed => {
                self.config_save_rx = None;
                return Some(ManagerConfigSaveResult::Settings(Err(anyhow::anyhow!(
                    crate::tui::subscriptions::config_save_worker_disconnected_message()
                ))));
            }
        };
        self.config_save_rx = None;
        Some(result)
    }

    pub fn apply_mount_info_refresh(&mut self, result: PendingMountInfoRefresh) -> bool {
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

    pub fn active_mount_info_sources(
        &self,
        config: &AppConfig,
    ) -> Option<(MountInfoRefreshTarget, Vec<String>)> {
        let facts = match &self.stage {
            ManagerStage::List => MountInfoRefreshSourceFacts::ManagerList {
                current_dir: self.current_dir.clone(),
                workspace_mount_sources: config
                    .workspaces
                    .values()
                    .flat_map(|workspace| workspace.mounts.iter().map(|mount| mount.src.clone()))
                    .collect(),
                global_mount_sources: config
                    .list_mount_rows()
                    .into_iter()
                    .map(|row| row.mount.src)
                    .collect(),
            },
            ManagerStage::Editor(editor) => MountInfoRefreshSourceFacts::Editor {
                mount_sources: editor
                    .pending
                    .mounts
                    .iter()
                    .map(|mount| mount.src.clone())
                    .collect(),
            },
            ManagerStage::Settings(settings) => MountInfoRefreshSourceFacts::SettingsMounts {
                mount_sources: settings
                    .mounts
                    .pending
                    .iter()
                    .map(|row| row.mount.src.clone())
                    .collect(),
            },
            ManagerStage::CreatePrelude(_)
            | ManagerStage::ConfirmDelete { .. }
            | ManagerStage::ConfirmInstancePurge { .. } => MountInfoRefreshSourceFacts::Inactive,
        };

        mount_info_refresh_source_plan(facts).map(|plan| (plan.target, plan.sources))
    }

    /// Poll the in-flight drift check started by a save operation.
    ///
    /// Returns `Some(check)` when the check has a result ready, taking
    /// ownership of the `PendingDriftCheck` so the caller can continue the
    /// save flow. Returns `None` when the check is still running or there is
    /// no pending check.
    pub fn poll_pending_drift_check(
        &mut self,
    ) -> Option<(
        PendingDriftCheck,
        anyhow::Result<jackin_core::DriftDetection>,
    )> {
        self.stage.poll_pending_drift_check()
    }

    pub fn poll_pending_isolation_cleanup(
        &mut self,
    ) -> Option<(PendingIsolationCleanup, anyhow::Result<()>)> {
        self.stage.poll_pending_isolation_cleanup()
    }

    pub fn poll_pending_role_load(&mut self) -> Option<(PendingRoleLoad, anyhow::Result<()>)> {
        self.stage.poll_pending_role_load()
    }

    pub fn poll_pending_op_commit(
        &mut self,
    ) -> Option<(jackin_core::OpRef, anyhow::Result<()>, bool)> {
        self.stage.poll_pending_op_commit().map(|resolution| {
            (
                resolution.op_ref,
                resolution.result,
                matches!(
                    resolution.origin,
                    crate::tui::app::ConsolePendingOpCommitOrigin::Settings
                ),
            )
        })
    }

    fn drain_instance_refresh(&mut self) -> Option<Result<ManagerInstanceRefreshSnapshot, String>> {
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
                    crate::tui::subscriptions::instance_refresh_worker_disconnected_message();
                Some(Err(message.into()))
            }
        }
    }

    pub fn apply_instance_refresh(
        &mut self,
        result: Result<ManagerInstanceRefreshSnapshot, String>,
    ) {
        match result {
            Ok(snapshot) => self.apply_instance_refresh_snapshot(snapshot),
            Err(error) => self.apply_instance_refresh_error(&error),
        }
    }

    pub fn open_list_error_popup(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.open_error_popup_modal(crate::tui::components::error_popup::error_popup_state(
            title, message,
        ));
    }

    pub fn apply_instance_refresh_snapshot(&mut self, snapshot: ManagerInstanceRefreshSnapshot) {
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
        self.selected = selected_index(self.selected, self.row_count());
    }

    pub fn apply_instance_refresh_error(&mut self, error: &str) {
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
        let message = crate::tui::components::error_popup::instance_index_error_message(error);
        if self.instances_last_error.as_deref() != Some(&message) {
            self.open_list_error_popup(
                crate::tui::components::error_popup::instance_index_error_title(),
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
        self.instances_refresh_generation =
            forced_instance_refresh_generation(self.instances_refresh_generation);
        self.instances_refresh_rx = None;
    }

    /// Test helper: force the next `refresh_instances` call to hit disk
    /// regardless of the throttle interval.
    pub fn force_refresh_instances_for_test(&mut self) {
        self.instances_last_refresh = None;
        self.instances_refresh_generation =
            forced_instance_refresh_generation(self.instances_refresh_generation);
        self.instances_refresh_rx = None;
    }

    pub fn tick_active_animation(&mut self) -> bool {
        let mut dirty = false;
        if let Some(modal) = self.list_modal.as_mut() {
            dirty |= modal.tick_active_animation();
        }
        dirty |= self.stage.tick_active_animation();
        dirty
    }
}

impl WorkspaceTreeDisclosureState for ManagerState<'_> {
    fn collapse_workspace(&mut self, index: usize) {
        Self::collapse_workspace(self, index);
    }

    fn collapse_current_dir(&mut self) {
        Self::collapse_current_dir(self);
    }

    fn expand_workspace(&mut self, index: usize) {
        Self::expand_workspace(self, index);
    }

    fn expand_current_dir(&mut self) {
        Self::expand_current_dir(self);
    }
}

impl WorkspaceListSelectionState for ManagerState<'_> {
    fn clear_inline_role_picker(&mut self) {
        self.inline_role_picker = None;
    }

    fn clear_inline_agent_picker(&mut self) {
        self.inline_agent_picker = None;
    }

    fn clear_inline_new_session_picker(&mut self) {
        self.inline_new_session_picker = None;
    }

    fn clear_inline_provider_picker(&mut self) {
        self.inline_provider_picker = None;
    }

    fn clear_launch_provider_picker(&mut self) {
        self.launch_provider_picker = None;
    }

    fn reset_list_scroll(&mut self) {
        Self::reset_list_scroll(self);
    }

    fn set_selected(&mut self, selected: usize) {
        self.selected = selected;
    }
}

impl ConsoleManagerStageState<ManagerStage<'static>> for ManagerState<'_> {
    fn set_manager_stage(&mut self, stage: ManagerStage<'static>) {
        self.stage = stage;
    }
}

impl LaunchAgentPromptManagerState<jackin_core::RoleSelector, jackin_core::Agent>
    for ManagerState<'_>
{
    fn open_launch_agent_prompt(
        &mut self,
        role: jackin_core::RoleSelector,
        picker: crate::tui::components::agent_choice::AgentChoiceState<jackin_core::Agent>,
    ) {
        self.inline_agent_picker = Some((role, picker));
    }

    fn clear_launch_role_prompt(&mut self) {
        self.inline_role_picker = None;
    }
}

impl LaunchRolePromptManagerState<jackin_core::RoleSelector> for ManagerState<'_> {
    fn open_launch_role_prompt(
        &mut self,
        picker: crate::tui::components::role_picker::RolePickerState<jackin_core::RoleSelector>,
    ) {
        self.inline_role_picker = Some(picker);
    }
}

impl
    LaunchProviderPickerManagerState<
        jackin_core::RoleSelector,
        jackin_core::Agent,
        jackin_protocol::Provider,
    > for ManagerState<'_>
{
    fn open_launch_provider_picker(
        &mut self,
        picker: crate::tui::components::provider_picker::ProviderPickerState<
            jackin_core::RoleSelector,
            jackin_core::Agent,
            jackin_protocol::Provider,
        >,
    ) {
        self.launch_provider_picker = Some(picker);
    }
}

impl WorkspaceListHoverState for ManagerState<'_> {
    fn set_workspace_list_hover_target(&mut self, target: Option<super::ManagerHoverTarget>) {
        self.hover_target = target;
    }
}

impl StatusOverlayState for ManagerState<'_> {
    fn set_status_overlay(&mut self, overlay: Option<jackin_tui::components::StatusPopupState>) {
        self.status_overlay = overlay;
    }
}

impl ListModalState for ManagerState<'_> {
    fn open_container_info_modal(&mut self, state: jackin_tui::components::ContainerInfoState) {
        self.list_modal = Some(Modal::ContainerInfo { state });
    }

    fn open_error_popup_modal(&mut self, state: jackin_tui::components::ErrorPopupState) {
        self.list_modal = Some(Modal::ErrorPopup { state });
    }

    fn open_github_picker_modal(
        &mut self,
        state: crate::tui::components::github_picker::GithubPickerState,
    ) {
        self.list_modal = Some(Modal::GithubPicker { state });
    }

    fn dismiss_list_modal(&mut self) {
        self.list_modal = None;
    }
}

impl InlinePickerDismissalState for ManagerState<'_> {
    fn clear_inline_new_session_picker(&mut self) {
        self.inline_new_session_picker = None;
    }

    fn clear_inline_role_picker(&mut self) {
        self.inline_role_picker = None;
    }

    fn clear_inline_agent_picker(&mut self) {
        self.inline_agent_picker = None;
    }

    fn clear_inline_provider_picker(&mut self) {
        self.inline_provider_picker = None;
    }

    fn clear_launch_provider_picker(&mut self) {
        self.launch_provider_picker = None;
    }
}

impl InlineNewSessionPickerState<String, jackin_core::Agent, jackin_protocol::Provider>
    for ManagerState<'_>
{
    fn set_inline_new_session_picker(
        &mut self,
        context: String,
        picker: crate::tui::components::agent_choice::AgentChoiceState<jackin_core::Agent>,
        providers: Vec<jackin_protocol::Provider>,
    ) {
        self.inline_new_session_picker = Some((context, picker, providers));
    }
}

impl InlineProviderPickerState<String, jackin_core::Agent, jackin_protocol::Provider>
    for ManagerState<'_>
{
    fn set_inline_provider_picker(
        &mut self,
        picker: crate::tui::components::provider_picker::ProviderPickerState<
            String,
            jackin_core::Agent,
            jackin_protocol::Provider,
        >,
    ) {
        self.inline_provider_picker = Some(picker);
    }
}

impl PreviewFocusState for ManagerState<'_> {
    fn set_preview_focused(&mut self, focused: bool) {
        self.preview_focused = focused;
    }
}

impl PreviewPaneCursorState for ManagerState<'_> {
    fn set_preview_pane_cursor(&mut self, container: &str, cursor: usize) {
        self.preview_pane_cursor
            .insert(container.to_owned(), cursor);
    }
}

impl crate::tui::screens::workspaces::update::WorkspaceListScrollState for ManagerState<'_> {
    fn list_names_scroll_x(&self) -> u16 {
        self.list_names_scroll_x
    }

    fn set_list_names_scroll_x(&mut self, value: u16) {
        self.list_names_scroll_x = value;
    }

    fn block_scroll_x(&self, focus: MountScrollFocus) -> u16 {
        match focus {
            MountScrollFocus::Workspace => self.list_mounts_scroll_x,
            MountScrollFocus::Global => self.list_global_mounts_scroll_x,
            MountScrollFocus::RoleGlobal => self.list_role_global_mounts_scroll_x,
            MountScrollFocus::Roles => self.list_roles_scroll_x,
        }
    }

    fn set_block_scroll_x(&mut self, focus: MountScrollFocus, value: u16) {
        *self.list_scroll_x_mut(focus) = value;
    }

    fn block_scroll_y(&self, focus: MountScrollFocus) -> u16 {
        match focus {
            MountScrollFocus::Workspace => self.list_mounts_scroll_y,
            MountScrollFocus::Global => self.list_global_mounts_scroll_y,
            MountScrollFocus::RoleGlobal => self.list_role_global_mounts_scroll_y,
            MountScrollFocus::Roles => self.list_roles_scroll_y,
        }
    }

    fn set_block_scroll_y(&mut self, focus: MountScrollFocus, value: u16) {
        *self.list_scroll_y_mut(focus) = value;
    }
}

impl ManagerState<'_> {
    pub fn apply_op_picker_op_ref_committed_for_editor(&mut self, op_ref: jackin_core::OpRef) {
        let ManagerStage::Editor(editor) = &mut self.stage else {
            return;
        };
        if !crate::tui::auth_config::ModalAuthFormOpRefApply::apply_auth_op_ref(
            &mut editor.modal,
            &mut editor.modal_parents,
            crate::tui::screens::settings::model::AuthFormFocus::Save,
            op_ref,
        ) {
            jackin_diagnostics::debug_log!(
                "auth",
                "AUTH005 apply_op_picker_op_ref_committed_for_editor: \
                 pending_auth_form_return missing — async OpRef commit dropped"
            );
        }
    }

    pub fn apply_op_picker_commit_failed_for_editor(&mut self, error: &anyhow::Error) {
        let ManagerStage::Editor(editor) = &mut self.stage else {
            return;
        };
        editor.open_error_popup(
            crate::tui::components::error_popup::op_read_failed_error_popup_state(error),
        );
    }

    pub fn apply_op_picker_op_ref_committed_for_settings(&mut self, op_ref: jackin_core::OpRef) {
        let ManagerStage::Settings(settings) = &mut self.stage else {
            return;
        };
        let Some(super::SettingsAuthModal::AuthForm {
            target,
            mut state,
            literal_buffer,
            ..
        }) = settings.auth.pop_parent_modal()
        else {
            jackin_diagnostics::debug_log!(
                "auth",
                "apply_op_picker_op_ref_committed_for_settings: modal_parents missing \
                 — async OpRef commit dropped"
            );
            return;
        };
        state.set_op_ref(op_ref);
        settings.auth.set_modal(super::SettingsAuthModal::AuthForm {
            target,
            state,
            focus: crate::tui::screens::settings::model::AuthFormFocus::Save,
            literal_buffer,
        });
    }

    pub fn apply_op_picker_commit_failed_for_settings(&mut self, error: &anyhow::Error) {
        let ManagerStage::Settings(settings) = &mut self.stage else {
            return;
        };
        settings.auth.set_error(
            crate::tui::screens::settings::view::settings_auth_op_read_failed_message(error),
        );
    }
}

impl ListShellState for ManagerState<'_> {
    fn set_drag_state(&mut self, drag: Option<crate::tui::split::DragState>) {
        self.drag_state = drag;
    }

    fn set_list_split_pct(&mut self, pct: u16) {
        self.list_split_pct = pct;
    }
}

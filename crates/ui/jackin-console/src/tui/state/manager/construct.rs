// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ManagerState` construction from config.

use ratatui::layout::Rect;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use jackin_config::AppConfig;

use crate::tui::screens::workspaces::update::initial_workspace_selected_index;

use super::super::{
    DEFAULT_SPLIT_PCT, ManagerStage, ManagerState, MountInfoCache, MountScrollFocus,
    WorkspaceSummary,
};
use jackin_env::OpCache;

impl ManagerState<'_> {
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
            keyboard_help: None,
            inline_role_picker: None,
            inline_agent_picker: None,
            inline_new_session_picker: None,
            inline_account_picker: None,
            launch_account_picker: None,
            list_mounts_scroll: crate::tui::scroll_block::console_scroll_area_state(),
            list_global_mounts_scroll: crate::tui::scroll_block::console_scroll_area_state(),
            list_role_global_mounts_scroll: crate::tui::scroll_block::console_scroll_area_state(),
            list_roles_scroll: crate::tui::scroll_block::console_scroll_area_state(),
            list_focus_owner: crate::tui::focus::TabFocus::tab_bar(MountScrollFocus::Workspace),
            list_names_scroll: crate::tui::scroll_block::console_scroll_area_state(),
            list_split_pct: DEFAULT_SPLIT_PCT,
            drag_state: None,
            hover_target: None,
            hover: termrock::interaction::HoverState::default(),
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
            instances_refresh_interval: crate::tui::subscriptions::INSTANCE_REFRESH_INTERVAL,
            instances_refresh_generation: 0,
            instances_refresh_rx: None,
            mount_info_refresh_rx: None,
            file_browser_listing_rx: None,
            file_browser_commit_rx: None,
            config_save_rx: None,
            account_scan_rx: None,
            instances_last_error: None,
            expanded_workspaces: BTreeSet::new(),
            current_dir_expanded: false,
            instance_sessions: HashMap::new(),
            instance_session_errors: HashSet::new(),
            live_instance_admissions: HashMap::new(),
            instance_snapshots: HashMap::new(),
            preview_focused: false,
            preview_pane_cursor: HashMap::new(),
            usage: super::super::UsageRouteState::default(),
            usage_snapshot: super::super::UsageScreenState::default(),
        }
    }
}

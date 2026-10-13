// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `EditorState` construction, focus, and tab plans.

use super::super::super::{EditorFocusTarget, EditorMode, EditorState, EditorTab, FieldFocus};
use crate::tui::focus::ConsoleFocusTarget;
use jackin_config::WorkspaceConfig;
use std::collections::BTreeSet;
use std::marker::PhantomData;

impl<
    MountInfoCache,
    Modal,
    SaveFlow,
    EnvValue,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
>
    EditorState<
        MountInfoCache,
        Modal,
        SaveFlow,
        EnvValue,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >
{
    pub fn new_edit(name: String, ws: WorkspaceConfig) -> Self
    where
        WorkspaceConfig: Clone,
        MountInfoCache: Default,
        SaveFlow: Default,
    {
        Self {
            mode: EditorMode::Edit { name },
            active_tab: EditorTab::General,
            focus_owner: crate::tui::focus::TabFocus::tab_bar(EditorFocusTarget::TabContent),
            hover_target: None,
            active_field: FieldFocus::Row(0),
            original: ws.clone(),
            pending: ws,
            mount_info_cache: MountInfoCache::default(),
            modal: None,
            modal_parents: Vec::new(),
            pending_name: None,
            exit_after_save: None,
            save_flow: SaveFlow::default(),
            unmasked_rows: BTreeSet::default(),
            secrets_expanded: BTreeSet::default(),
            _env_value: PhantomData,
            workspace_mounts_scroll: crate::tui::scroll_block::console_scroll_area_state(),
            tab_scroll: crate::tui::scroll_block::console_scroll_area_state(),
            tab_content_width: 0,
            tab_content_height: 0,

            pending_role_load: None,
            pending_drift_check: None,
            pending_isolation_cleanup: None,
            pending_op_commit: None,
            cached_footer_h: 1,
        }
    }

    #[must_use]
    pub fn focus_owner(&self) -> ConsoleFocusTarget<EditorFocusTarget> {
        self.focus_owner.focused()
    }

    pub fn set_focus_owner(&mut self, owner: ConsoleFocusTarget<EditorFocusTarget>) {
        match owner {
            ConsoleFocusTarget::TabBar => self.focus_owner.focus_tab_bar(),
            ConsoleFocusTarget::Content(content) => self.focus_owner.focus_content(content),
        }
    }

    pub fn apply_tab_move_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorTabMovePlan,
    ) {
        self.active_tab = plan.active_tab;
        self.set_tab_bar_focused(plan.tab_bar_focused);
        self.active_field = FieldFocus::Row(plan.active_row);
        crate::tui::scroll_block::scroll_area_set_x(&mut self.tab_scroll, plan.tab_scroll_x);
        crate::tui::scroll_block::scroll_area_set_y(&mut self.tab_scroll, plan.tab_scroll_y);
        if plan.tab_bar_focused {
            self.set_workspace_mounts_scroll_focused(false);
            self.set_tab_content_scroll_focused(false);
        }
        if plan.clear_secret_view_state {
            self.unmasked_rows.clear();
            self.secrets_expanded.clear();
        }
    }

    pub fn apply_tab_select_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorTabSelectPlan,
    ) {
        self.active_tab = plan.active_tab;
        self.set_tab_bar_focused(plan.tab_bar_focused);
        self.active_field = FieldFocus::Row(plan.active_row);
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
        if plan.clear_secret_view_state {
            self.unmasked_rows.clear();
            self.secrets_expanded.clear();
        }
    }

    pub fn apply_field_selection_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorFieldSelectionPlan,
    ) {
        self.active_field = FieldFocus::Row(plan.active_row);
        crate::tui::scroll_block::scroll_area_set_y(&mut self.tab_scroll, plan.tab_scroll_y);
    }

    pub fn apply_mount_row_select_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorMountRowSelectPlan,
    ) {
        self.active_field = FieldFocus::Row(plan.active_row);
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
    }

    pub fn select_row(&mut self, row: usize) {
        self.active_field = FieldFocus::Row(row);
    }

    pub fn select_auth_row(&mut self, row: usize) {
        self.select_row(row);
    }

    pub fn apply_tab_horizontal_scroll_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorHorizontalScrollPlan,
    ) {
        crate::tui::scroll_block::scroll_area_set_x(&mut self.tab_scroll, plan.scroll_x);
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
        self.set_tab_content_scroll_focused(plan.tab_content_scroll_focused);
    }

    pub fn apply_workspace_mounts_horizontal_scroll_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorHorizontalScrollPlan,
    ) {
        crate::tui::scroll_block::scroll_area_set_x(
            &mut self.workspace_mounts_scroll,
            plan.scroll_x,
        );
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
        self.set_tab_content_scroll_focused(plan.tab_content_scroll_focused);
    }

    pub fn apply_scroll_focus_plan(
        &mut self,
        plan: crate::tui::screens::editor::update::EditorScrollFocusPlan,
    ) {
        self.set_workspace_mounts_scroll_focused(plan.workspace_mounts_scroll_focused);
        self.set_tab_content_scroll_focused(plan.tab_content_scroll_focused);
    }

    #[must_use]
    pub fn tab_bar_focused(&self) -> bool {
        self.focus_owner.is_tab_bar()
    }
}

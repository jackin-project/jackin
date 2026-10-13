// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Global mounts settings state.

use super::{
    SettingsMountsTakeExit, SettingsPanelChangeCount, SettingsPanelDirty, SettingsPanelDiscard,
    SettingsPanelMarkSaved, SettingsPanelTakeError,
};

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountConfirm {
    Remove,
    Save,
    Sensitive,
    Discard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountTextTarget {
    AddScope,
    AddName,
    AddSource,
    AddDestination,
    Source,
    Destination,
    Scope,
    Rename,
}

#[derive(Debug)]
pub struct GlobalMountsState<Row, Modal> {
    pub selected: usize,
    pub pending: Vec<Row>,
    pub original: Vec<Row>,
    pub mount_info_cache: crate::mount_info_cache::MountInfoCache,
    pub modals: crate::tui::modal_chain::ModalChain<Modal>,
    pub add_draft: Option<GlobalMountDraft>,
    pub error: Option<String>,
    pub scroll: termrock::widgets::ScrollAreaState,
    /// Dispatcher pops back to the workspace list when set.
    pub exit_requested: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct GlobalMountsSaveRefs<'a, Row> {
    pub original: &'a [Row],
    pub pending: &'a [Row],
}

impl<Row, Modal> GlobalMountsState<Row, Modal> {
    #[must_use]
    pub fn from_rows(rows: Vec<Row>) -> Self
    where
        Row: Clone,
    {
        Self {
            selected: 0,
            pending: rows.clone(),
            original: rows,
            mount_info_cache: crate::mount_info_cache::MountInfoCache::default(),
            modals: crate::tui::modal_chain::ModalChain::new(),
            add_draft: None,
            error: None,
            scroll: crate::tui::scroll_block::console_scroll_area_state(),
            exit_requested: false,
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool
    where
        Row: PartialEq,
    {
        self.pending != self.original
    }

    #[must_use]
    pub fn save_refs(&self) -> GlobalMountsSaveRefs<'_, Row> {
        GlobalMountsSaveRefs {
            original: &self.original,
            pending: &self.pending,
        }
    }

    pub fn discard(&mut self)
    where
        Row: Clone,
    {
        self.pending = self.original.clone();
        self.mount_info_cache.clear();
        self.selected = self.selected.min(self.pending.len().saturating_sub(1));
        self.add_draft = None;
        self.modals.clear();
        self.error = None;
    }

    pub fn apply_selection_plan(
        &mut self,
        plan: crate::tui::screens::settings::update::SettingsSelectionScrollPlan,
    ) {
        self.selected = plan.selected;
        crate::tui::scroll_block::scroll_area_set_y(&mut self.scroll, plan.scroll_y);
    }

    pub fn apply_horizontal_scroll(&mut self, scroll_x: u16) {
        crate::tui::scroll_block::scroll_area_set_x(&mut self.scroll, scroll_x);
    }

    pub fn mark_saved(&mut self)
    where
        Row: Clone,
    {
        self.original = self.pending.clone();
        self.mount_info_cache.clear();
    }

    pub fn open_sub_modal(&mut self, child: Modal) {
        self.modals.open_sub(child);
    }

    pub fn start_add_draft(&mut self) {
        self.add_draft = Some(GlobalMountDraft::default());
        self.modals.clear();
    }

    pub fn remove_row_and_select(&mut self, remove_index: usize, selected: usize) {
        self.pending.remove(remove_index);
        self.selected = selected;
    }

    pub fn pop_modal_chain(&mut self) {
        self.modals.pop();
    }

    pub fn pop_modal_chain_and_clear_add_draft_if_closed(&mut self) {
        self.pop_modal_chain();
        if !self.modals.is_open() {
            self.add_draft = None;
        }
    }

    pub fn clear_modal_chain(&mut self) {
        self.modals.clear();
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    pub const fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    pub fn take_exit_requested(&mut self) -> bool {
        std::mem::take(&mut self.exit_requested)
    }
}

impl<Row, Modal> SettingsPanelTakeError for GlobalMountsState<Row, Modal> {
    fn take_panel_error(&mut self) -> Option<String> {
        self.take_error()
    }
}

impl<Row, Modal> SettingsMountsTakeExit for GlobalMountsState<Row, Modal> {
    fn take_mounts_exit_requested(&mut self) -> bool {
        self.take_exit_requested()
    }
}

impl<Modal> GlobalMountsState<jackin_config::GlobalMountRow, Modal> {
    #[must_use]
    pub fn content_width(&self) -> usize {
        crate::tui::mount_display::settings_global_config_mounts_content_width_with_cache(
            &self.pending,
            &self.mount_info_cache,
        )
    }

    pub fn add_row_and_close(&mut self, row: jackin_config::GlobalMountRow, selected: usize) {
        self.pending.push(row);
        self.selected = selected;
        self.clear_modal_chain();
    }

    pub fn toggle_selected_readonly(&mut self) {
        if let Some(row) = self.pending.get_mut(self.selected) {
            row.mount.readonly = !row.mount.readonly;
        }
    }
}

impl<Row, Modal> SettingsPanelDirty for GlobalMountsState<Row, Modal>
where
    Row: PartialEq,
{
    fn panel_is_dirty(&self) -> bool {
        self.is_dirty()
    }
}

impl<Row, Modal> SettingsPanelChangeCount for GlobalMountsState<Row, Modal>
where
    Row: PartialEq,
{
    fn panel_change_count(&self) -> usize {
        crate::tui::screens::settings::update::settings_vec_change_count(
            &self.original,
            &self.pending,
        )
    }
}

impl<Row, Modal> SettingsPanelDiscard for GlobalMountsState<Row, Modal>
where
    Row: Clone,
{
    fn panel_discard(&mut self) {
        self.discard();
    }
}

impl<Row, Modal> SettingsPanelMarkSaved for GlobalMountsState<Row, Modal>
where
    Row: Clone,
{
    fn panel_mark_saved(&mut self) {
        self.mark_saved();
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GlobalMountDraft {
    pub name: String,
    pub src: String,
    pub dst: String,
    pub scope: Option<String>,
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `ManagerState` popups, snapshots, and animation.

use crate::tui::model::ConsoleAnimationTick;

use crate::tui::screens::workspaces::update::selected_index;
use crate::tui::subscriptions::forced_instance_refresh_generation;
use crate::tui::update::ListModalState;

use super::super::{ManagerInstanceRefreshSnapshot, ManagerState};

impl ManagerState<'_> {
    pub fn open_list_error_popup(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.open_error_popup_modal(crate::tui::components::error_popup::error_popup_state(
            title, message,
        ));
    }

    pub fn apply_instance_refresh_snapshot(&mut self, snapshot: ManagerInstanceRefreshSnapshot) {
        self.instances = snapshot.instances;
        self.instance_sessions = snapshot.sessions;
        self.instance_session_errors = snapshot.session_errors;
        self.live_instance_admissions = snapshot.admissions;
        self.instance_snapshots = snapshot.snapshots;
        self.instances_refresh_interval = snapshot.next_interval;
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
        self.live_instance_admissions.clear();
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

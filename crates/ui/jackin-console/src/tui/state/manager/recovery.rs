// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Manager recovery record and shell state.

use crate::tui::update::ListShellState;

use super::super::ManagerState;

pub(crate) fn record_manager_recovery() {
    let _recorded = jackin_telemetry::record_recovered_degradation();
}

impl ListShellState for ManagerState<'_> {
    fn set_drag_state(&mut self, drag: Option<crate::tui::split::DragState>) {
        self.drag_state = drag;
    }

    fn set_list_split_pct(&mut self, pct: u16) {
        self.list_split_pct = pct;
    }
}

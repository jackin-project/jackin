// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `ContainerInfo` dialog constructors and state helpers extracted from the
//! main coordinator impl. `ContainerInfoDiagnostics` is re-exported at the
//! parent level.

use super::{ContainerInfoDiagnostics, Dialog};

impl Dialog {
    pub fn new_container_info(
        container_name: String,
        role: String,
        focused_agent: Option<String>,
        workdir: String,
        diagnostics: ContainerInfoDiagnostics,
    ) -> Self {
        Self::ContainerInfo {
            container_name,
            role,
            focused_agent,
            workdir,
            diagnostics,
            copied_row: None,
            hovered_row: None,
            scroll: jackin_tui::components::DialogBodyScroll::new(),
        }
    }

    /// Build the shared [`ContainerInfoState`](jackin_tui::components::ContainerInfoState)
    /// for the `ContainerInfo` ("Debug info") dialog from the accumulating
    /// [`DebugInfo`](jackin_tui::components::DebugInfo) model — the single
    /// source of rows/order/labels/copy-affordances shared with the host
    /// console and launch cockpit. Returns `None` for other dialog variants.
    ///
    /// Run id / diagnostics-log rows are included only under `--debug`, matching
    /// the host. Versions are the exact `jackin --version` / `jackin-capsule
    /// --version` strings.
    pub(crate) fn container_info_state(
        &self,
    ) -> Option<jackin_tui::components::ContainerInfoState> {
        self.container_info_state_with_debug(crate::logging::debug_enabled())
    }

    pub(crate) fn container_info_state_with_debug(
        &self,
        debug_enabled: bool,
    ) -> Option<jackin_tui::components::ContainerInfoState> {
        let Self::ContainerInfo {
            container_name,
            role,
            focused_agent,
            workdir,
            diagnostics,
            copied_row,
            hovered_row,
            scroll,
        } = self
        else {
            return None;
        };
        let agent_label = focused_agent
            .as_deref()
            .and_then(jackin_tui::agent_display_name)
            .or(focused_agent.as_deref())
            .unwrap_or("(shell)")
            .to_owned();
        let debug = debug_enabled && !diagnostics.invocation_id.is_empty();
        let mut state = jackin_tui::components::DebugInfo {
            jackin_version: Some(diagnostics.host_version.clone()),
            capsule_version: Some(env!("JACKIN_CAPSULE_VERSION").to_owned()),
            container_id: Some(container_name.clone()),
            role: (!role.is_empty()).then(|| role.clone()),
            agent: Some(agent_label),
            target: (!workdir.is_empty()).then(|| workdir.clone()),
            run_id: debug.then(|| diagnostics.invocation_id.clone()),
            diagnostics_log_path: None,
        }
        .into_state();
        if let Some(row) = *copied_row {
            state.mark_copied(row);
        }
        state.set_hovered_row(*hovered_row);
        state.scroll = scroll.clone();
        Some(state)
    }

    /// Update the hovered copyable row of the `ContainerInfo` dialog from a
    /// pointer hit at `(row, col)` (1-based). Returns true when the hovered
    /// row changed (the caller redraws so the link hover colour updates).
    /// No-op for other dialog variants.
    pub fn set_container_info_hover(
        &mut self,
        row: u16,
        col: u16,
        term_rows: u16,
        term_cols: u16,
    ) -> bool {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let area = ratatui::layout::Rect {
            x: box_col,
            y: box_row,
            width,
            height,
        };
        let hit = self.container_info_state().and_then(|state| {
            jackin_tui::components::container_info_copy_payload_at(area, &state, col, row)
                .map(|(idx, _)| idx)
        });
        if let Self::ContainerInfo { hovered_row, .. } = self
            && *hovered_row != hit
        {
            *hovered_row = hit;
            return true;
        }
        false
    }
}

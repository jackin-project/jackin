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
            scroll: termrock::scroll::DialogScroll::new(),
        }
    }

    /// Build the shared [`ContainerInfoState`](crate::tui::components::container_info_surface::ContainerInfoState)
    /// for the `ContainerInfo` ("Debug info") dialog from the accumulating
    /// [`DebugInfo`](crate::tui::components::container_info_surface::DebugInfo) model — the single
    /// source of rows/order/labels/copy-affordances shared with the host
    /// console and launch cockpit. Returns `None` for other dialog variants.
    ///
    /// Run id / diagnostics-log rows are included only under `--debug`, matching
    /// the host. Versions are the exact `jackin --version` / `jackin-capsule
    /// --version` strings.
    pub(crate) fn container_info_state(
        &self,
    ) -> Option<crate::tui::components::container_info_surface::ContainerInfoState> {
        self.container_info_state_with_debug(crate::logging::debug_enabled())
    }

    pub(crate) fn container_info_state_with_debug(
        &self,
        debug_enabled: bool,
    ) -> Option<crate::tui::components::container_info_surface::ContainerInfoState> {
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
            .and_then(crate::tui::components::agent_display_name)
            .or(focused_agent.as_deref())
            .unwrap_or("(shell)")
            .to_owned();
        let debug = debug_enabled && !diagnostics.run_id.is_empty();
        // Pass the absolute path so the `file://` href the model builds is
        // valid; `run_log_href` already carries it (`file://<abs>`).
        let log_path = debug.then_some(()).and_then(|()| {
            diagnostics
                .run_log_href
                .as_deref()
                .and_then(|href| href.strip_prefix("file://"))
                .map(str::to_owned)
        });
        let mut state = crate::tui::components::container_info_surface::DebugInfo {
            jackin_version: Some(diagnostics.host_version.clone()),
            capsule_version: Some(env!("JACKIN_CAPSULE_VERSION").to_owned()),
            container_id: Some(container_name.clone()),
            role: (!role.is_empty()).then(|| role.clone()),
            agent: Some(agent_label),
            target: (!workdir.is_empty()).then(|| workdir.clone()),
            run_id: debug.then(|| diagnostics.run_id.clone()),
            diagnostics_log_path: log_path,
        }
        .into_state();
        if debug && let Some(href) = diagnostics.run_log_href.as_deref() {
            state.push_row(
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Reveal diagnostics",
                    diagnostics.run_log_display.clone(),
                )
                .hyperlink(href.to_owned()),
            );
        } else if debug && !diagnostics.run_id.is_empty() {
            state.push_row(
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Telemetry",
                    diagnostics.run_log_display.clone(),
                ),
            );
        }
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
            crate::tui::components::container_info_surface::container_info_copy_payload_at(
                area, &state, col, row,
            )
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

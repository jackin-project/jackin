// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{ContainerInfoDiagnostics, Dialog};

impl Dialog {
    #[must_use]
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
        let debug = debug_enabled && !diagnostics.invocation_id.is_empty();
        let mut state = crate::tui::components::container_info_surface::DebugInfo {
            jackin_version: Some(diagnostics.host_version.clone()),
            capsule_version: Some(env!("JACKIN_CAPSULE_VERSION").to_owned()),
            container_id: Some(container_name.clone()),
            role: (!role.is_empty()).then(|| role.clone()),
            agent: Some(agent_label),
            target: (!workdir.is_empty()).then(|| workdir.clone()),
            run_id: debug.then(|| diagnostics.invocation_id.clone()),
        }
        .into_state();
        if let Some(row) = *copied_row {
            state.mark_copied(row);
        }
        state.set_hovered_row(*hovered_row);
        state.scroll = scroll.clone();
        Some(state)
    }

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

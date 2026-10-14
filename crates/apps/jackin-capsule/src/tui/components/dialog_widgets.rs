// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::layout::Rect;

pub(crate) mod usage;
#[expect(
    unused_imports,
    reason = "re-exports consumed by tests + sibling modules"
)]
pub(crate) use usage::{
    usage_body_rect, usage_content_width, usage_dialog_inner_area, usage_info_lines_for_width,
    usage_info_required_height, usage_line_width, usage_panel_title, usage_provider_display_label,
    usage_scroll_inputs, usage_tab_strip_area, usage_tab_strip_index_at, usage_tab_strip_labels,
    usage_tab_strip_width,
};

#[derive(Debug, Clone)]
pub(crate) enum PickerItem {
    Item(String),
    Section(String),
}

#[derive(Debug, Clone)]
pub(crate) enum DialogRatatuiSnapshot {
    ConfirmAction {
        title: String,
        body: String,
        selected_yes: bool,
        data_loss: bool,
    },
    FilterPicker {
        title: String,
        filter: String,
        items: Vec<PickerItem>,
        selected: usize,
        show_filter: bool,
    },
    TextInputDialog {
        dialog_title: String,
        label: String,
        value: String,
        cursor: usize,
    },
    ErrorPopup(crate::tui::components::dialog::SpawnFailureState),
    DebugInfo(crate::tui::components::container_info_surface::ContainerInfoState),
    UsageInfo {
        state: crate::tui::components::container_info_surface::ContainerInfoState,
        tabs: Vec<(String, bool)>,
        tab_bar_focused: bool,
        hovered_tab: Option<usize>,
    },
}

impl DialogRatatuiSnapshot {
    pub(crate) fn scroll_axes(&self, block_area: Rect) -> termrock::scroll::ScrollAxes {
        match self {
            Self::DebugInfo(state) => termrock::scroll::dialog_scroll_axes(
                state.content_width(),
                state.content_height(),
                block_area,
            ),
            Self::UsageInfo { state, tabs, .. } => {
                let (content_width, content_height, scroll_rect) =
                    usage_scroll_inputs(block_area, state);
                let width = content_width.max(usage_tab_strip_width(tabs));
                termrock::scroll::dialog_scroll_axes(width, content_height, scroll_rect)
            }
            _ => termrock::scroll::ScrollAxes::none(),
        }
    }
}

mod render;
mod snapshot;
pub(crate) use render::render_dialog_ratatui;

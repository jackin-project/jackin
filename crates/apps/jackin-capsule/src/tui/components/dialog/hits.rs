// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Dialog hit-testing and overlay box geometry.

use super::github_context::GithubContextView;
use super::input::{
    PickerRow, close_target_filtered_indices, dialog_list_row_clickable, picker_filtered_rows,
    split_direction_filtered_indices,
};
use super::{
    CONTAINER_INFO_WIDTH, ConfirmKind, Dialog, EXIT_DIRTY_ROWS, GITHUB_OPEN_CI_ROW,
    GITHUB_OPEN_PR_ROW, PALETTE_WIDTH, file_url_path,
};

use crate::tui::components::modal_rects::{ModalRectSpec, modal_rect};
use crate::tui::components::palette::palette_filtered_indices;

impl Dialog {
    /// Return true when `(row, col)` is a dialog hit target that will
    /// perform an action on click. The daemon uses this to drive OSC 22
    /// pointer-shape feedback without duplicating dialog layout maths.
    #[must_use]
    pub fn clickable_at(
        &self,
        row: u16,
        col: u16,
        term_rows: u16,
        term_cols: u16,
        github: Option<&GithubContextView<'_>>,
    ) -> bool {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let area = ratatui::layout::Rect {
            x: box_col,
            y: box_row,
            width,
            height,
        };
        let inside_box =
            row >= box_row && row < box_row + height && col >= box_col && col < box_col + width;
        if !inside_box {
            return false;
        }
        match self {
            Self::RenameTab { .. }
            | Self::ExportFile { .. }
            | Self::ExecPicker(_)
            | Self::SpawnFailure(_) => false,
            Self::ContainerInfo { .. } => {
                let area = ratatui::layout::Rect {
                    x: box_col,
                    y: box_row,
                    width,
                    height,
                };
                self.container_info_state().is_some_and(|state| {
                    crate::tui::components::container_info_surface::container_info_copy_payload_at(area, &state, col, row)
                        .is_some()
                        || crate::tui::components::container_info_surface::container_info_hyperlink_payload_at(
                            area, &state, col, row,
                        )
                        .is_some_and(|(_, href)| file_url_path(&href).is_some())
                })
            }
            Self::GitHubContext { .. } => {
                let area = ratatui::layout::Rect {
                    x: box_col,
                    y: box_row,
                    width,
                    height,
                };
                self.github_context_state(github).is_some_and(|state| {
                    crate::tui::components::container_info_surface::container_info_copy_payload_at(area, &state, col, row)
                        .is_some()
                        || crate::tui::components::container_info_surface::container_info_hyperlink_payload_at(
                            area, &state, col, row,
                        )
                        .is_some_and(|(idx, _)| {
                            matches!(idx, GITHUB_OPEN_PR_ROW | GITHUB_OPEN_CI_ROW)
                        })
                })
            }
            Self::Usage { view, selected, .. } => {
                Self::usage_tab_index_at(view, *selected, area, row, col).is_some()
            }
            Self::ConfirmAction { .. } => true,
            Self::CommandPalette {
                filter,
                close_label,
                ..
            } => dialog_list_row_clickable(
                row,
                box_row,
                palette_filtered_indices(filter, *close_label).len(),
            ),
            Self::SplitDirectionPicker { filter, .. } => dialog_list_row_clickable(
                row,
                box_row,
                split_direction_filtered_indices(filter).len(),
            ),
            Self::CloseTargetPicker { filter, .. } => {
                dialog_list_row_clickable(row, box_row, close_target_filtered_indices(filter).len())
            }
            Self::AgentPicker { agents, filter, .. } => {
                let first_item_row = box_row + 3;
                let visible = picker_filtered_rows(agents, filter);
                if row < first_item_row
                    || row
                        >= first_item_row
                            .saturating_add(u16::try_from(visible.len()).unwrap_or(u16::MAX))
                {
                    return false;
                }
                matches!(
                    visible[(row - first_item_row) as usize],
                    PickerRow::Agent(_) | PickerRow::Shell
                )
            }
            // Keyboard-only modals — no click targets.
            Self::ExitDirty { .. } | Self::ExitInspect { .. } => false,
        }
    }

    /// Box geometry the dialog will render with for `term_rows` /
    /// `term_cols`. Returned as `(row, col, height, width)`. Kept
    /// next to the render functions so any layout change updates
    /// both surfaces at once.
    ///
    /// Height clamps to the area below the status bar so a very small
    /// terminal does not paint past the bottom edge (which would
    /// scroll the host terminal and destroy the operator's pane
    /// content) and does not overlap row 0 (the brand pill / tab
    /// strip). The dialog can render unusable when the terminal is
    /// pathologically small; the trade-off is that the host terminal
    /// stays in a recoverable state regardless.
    pub(crate) fn box_rect(&self, term_rows: u16, term_cols: u16) -> (u16, u16, u16, u16) {
        // Filterable dialogs reserve 2 extra rows: one for the filter
        // input and one for the separator above the items list. Item
        // count tracks the *filtered* set so the box shrinks as the
        // operator narrows the matches.
        let natural_height = match self {
            Self::CommandPalette {
                filter,
                close_label,
                ..
            } => {
                let items = u16::try_from(palette_filtered_indices(filter, *close_label).len())
                    .unwrap_or(u16::MAX);
                items.saturating_add(4) // top + filter + pad + items + bottom
            }
            Self::SplitDirectionPicker { filter, .. } => {
                let items = u16::try_from(split_direction_filtered_indices(filter).len())
                    .unwrap_or(u16::MAX);
                items.saturating_add(4)
            }
            Self::CloseTargetPicker { filter, .. } => {
                let items =
                    u16::try_from(close_target_filtered_indices(filter).len()).unwrap_or(u16::MAX);
                items.saturating_add(4)
            }
            Self::AgentPicker { agents, filter, .. } => {
                let items =
                    u16::try_from(picker_filtered_rows(agents, filter).len()).unwrap_or(u16::MAX);
                items.saturating_add(4)
            }
            Self::RenameTab { .. } | Self::ExportFile { .. } => 5,
            Self::ContainerInfo { .. } => self.container_info_state().map_or(10, |state| {
                crate::tui::components::container_info_surface::container_info_required_height(
                    &state,
                )
            }),
            Self::GitHubContext { .. } => 11,
            Self::Usage { .. } => self.usage_state().map_or(10, |state| {
                crate::tui::components::dialog_widgets::usage_info_required_height(&state)
            }),
            Self::SpawnFailure(state) => {
                let inner_width = PALETTE_WIDTH.saturating_sub(2);
                let rows = state
                    .message
                    .lines()
                    .map(|line| {
                        termrock::text::display_cols(line)
                            .div_ceil(usize::from(inner_width).max(1))
                            .max(1)
                    })
                    .sum::<usize>();
                u16::try_from(rows.saturating_add(2)).unwrap_or(u16::MAX)
            }
            // 9 = border(2) + leading(1) + question(1) + empty(1) + message(1) + spacer(1) + button(1) + trailing(1)
            // Matches the canonical symmetric dialog layout (Defect 5).
            // Exit shows the shared data-loss variant (extra warning notes), so
            // size it from that state rather than the fixed single-line height.
            Self::ConfirmAction { kind, .. } => match kind {
                ConfirmKind::Exit => 10,
                ConfirmKind::ClosePane | ConfirmKind::CloseTab => 9,
            },
            // No filter row: top border + items + bottom border.
            // Top border + command line + separator + one row per credential +
            // hint + bottom border.
            Self::ExecPicker(state) => u16::try_from(state.items.len())
                .unwrap_or(u16::MAX)
                .saturating_add(5),
            Self::ExitDirty { summary, .. } => u16::try_from(summary.len() + EXIT_DIRTY_ROWS.len())
                .unwrap_or(u16::MAX)
                .saturating_add(4),
            Self::ExitInspect { lines, .. } => u16::try_from(lines.len())
                .unwrap_or(u16::MAX)
                .saturating_add(4),
        };
        let content_height = crate::tui::layout::available_content_rows(term_rows).max(3);
        let max_height = if matches!(self, Self::Usage { .. }) {
            content_height.saturating_sub(1).max(3)
        } else {
            content_height
        };
        let height = natural_height.min(max_height);
        let top_row = crate::tui::components::status_bar::STATUS_BAR_ROWS;
        let area_y = if matches!(self, Self::Usage { .. }) {
            top_row.saturating_add(1)
        } else {
            top_row
        };
        let area_height = if matches!(self, Self::Usage { .. }) {
            content_height.saturating_sub(1)
        } else {
            content_height
        };
        let area = ratatui::layout::Rect::new(0, area_y, term_cols, area_height);
        let spec = match self {
            Self::ContainerInfo { .. } | Self::GitHubContext { .. } => ModalRectSpec::MaxWidthMin {
                max_width: CONTAINER_INFO_WIDTH,
                min_width: PALETTE_WIDTH,
                side_margin: 4,
                height,
            },
            Self::Usage { .. } => ModalRectSpec::TopAlignedMaxWidthMin {
                max_width: CONTAINER_INFO_WIDTH,
                min_width: PALETTE_WIDTH,
                side_margin: 4,
                height,
            },
            // Exit data-loss confirm has two warning notes wider than PALETTE_WIDTH.
            // Use the shared Details width percentage (70%) so the notes don't truncate.
            Self::ConfirmAction {
                kind: ConfirmKind::Exit,
                ..
            } => ModalRectSpec::PercentClamp {
                width_pct: 70,
                min_width: PALETTE_WIDTH,
                side_margin: 4,
                height,
            },
            _ => ModalRectSpec::Exact {
                width: PALETTE_WIDTH,
                height,
            },
        };
        let rect = modal_rect(area, spec);
        (rect.y, rect.x, rect.height, rect.width)
    }
}

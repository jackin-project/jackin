// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    CLOSE_TARGET_ITEMS, Dialog, DialogAction, EXIT_DIRTY_ROWS, ExitDirtyRow, SPLIT_DIRECTION_ITEMS,
};

use super::input::{
    PickerRow, close_target_filtered_indices, first_selectable_idx, picker_filtered_rows,
    printable_filter_char, split_direction_filtered_indices, step_selectable,
};

use crate::tui::components::palette::{PALETTE_ITEMS, palette_filtered_indices};
use crate::tui::keymap::raw_bytes_to_chord;
use crate::tui::keymap::{FILTER_LIST_KEYMAP, FilterListAction};

impl Dialog {
    #[expect(
        clippy::too_many_lines,
        reason = "Filter-list key dispatcher with one arm per key binding. \
                  Each arm carries its focused state transition; extracting \
                  arms into sub-dispatchers would obscure per-binding readability."
    )]
    pub(crate) fn handle_filter_list_key(&mut self, key: &[u8]) -> DialogAction {
        match raw_bytes_to_chord(key).and_then(|chord| FILTER_LIST_KEYMAP.dispatch(chord)) {
            Some(FilterListAction::Dismiss) => match self {
                Self::ExitDirty { .. } => DialogAction::ExitDirty(ExitDirtyRow::Keep),
                _ => DialogAction::Dismiss,
            },
            Some(FilterListAction::NavigateUp) => {
                match self {
                    Self::CommandPalette { selected, .. }
                    | Self::SplitDirectionPicker { selected, .. }
                    | Self::CloseTargetPicker { selected, .. } => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                    Self::AgentPicker {
                        agents,
                        selected,
                        filter,
                        ..
                    } => {
                        let visible = picker_filtered_rows(agents, filter);
                        *selected = step_selectable(&visible, *selected, false);
                    }
                    Self::RenameTab { .. }
                    | Self::ExportFile { .. }
                    | Self::ContainerInfo { .. }
                    | Self::GitHubContext { .. }
                    | Self::Usage { .. }
                    | Self::SpawnFailure(_)
                    | Self::ConfirmAction { .. }
                    | Self::ExecPicker(_) => {}
                    Self::ExitDirty { selected, .. } => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                    Self::ExitInspect { selected, .. } => {
                        if *selected > 0 {
                            *selected -= 1;
                        }
                    }
                }
                DialogAction::Redraw
            }
            Some(FilterListAction::NavigateDown) => {
                match self {
                    Self::CommandPalette {
                        selected,
                        filter,
                        close_label,
                    } => {
                        let visible = palette_filtered_indices(filter, *close_label);
                        if *selected + 1 < visible.len() {
                            *selected += 1;
                        }
                    }
                    Self::SplitDirectionPicker { selected, filter } => {
                        let visible = split_direction_filtered_indices(filter);
                        if *selected + 1 < visible.len() {
                            *selected += 1;
                        }
                    }
                    Self::CloseTargetPicker { selected, filter } => {
                        let visible = close_target_filtered_indices(filter);
                        if *selected + 1 < visible.len() {
                            *selected += 1;
                        }
                    }
                    Self::AgentPicker {
                        agents,
                        selected,
                        filter,
                        ..
                    } => {
                        let visible = picker_filtered_rows(agents, filter);
                        *selected = step_selectable(&visible, *selected, true);
                    }
                    Self::RenameTab { .. }
                    | Self::ExportFile { .. }
                    | Self::ContainerInfo { .. }
                    | Self::GitHubContext { .. }
                    | Self::Usage { .. }
                    | Self::SpawnFailure(_)
                    | Self::ConfirmAction { .. }
                    | Self::ExecPicker(_) => {}
                    Self::ExitDirty { selected, .. } => {
                        if *selected + 1 < EXIT_DIRTY_ROWS.len() {
                            *selected += 1;
                        }
                    }
                    Self::ExitInspect { selected, lines } => {
                        if *selected + 1 < lines.len() {
                            *selected += 1;
                        }
                    }
                }
                DialogAction::Redraw
            }
            Some(FilterListAction::FilterBackspace) => {
                match self {
                    Self::CommandPalette {
                        filter, selected, ..
                    }
                    | Self::SplitDirectionPicker { filter, selected }
                    | Self::CloseTargetPicker { filter, selected } => {
                        filter.pop();
                        *selected = 0;
                    }
                    Self::AgentPicker {
                        agents,
                        filter,
                        selected,
                        ..
                    } => {
                        filter.pop();
                        let visible = picker_filtered_rows(agents, filter);
                        *selected = first_selectable_idx(&visible);
                    }
                    _ => {}
                }
                DialogAction::Redraw
            }
            Some(FilterListAction::Confirm) => match self {
                Self::CommandPalette {
                    selected,
                    filter,
                    close_label,
                } => {
                    let visible = palette_filtered_indices(filter, *close_label);
                    match visible.get(*selected) {
                        Some(idx) => DialogAction::Command(PALETTE_ITEMS[*idx].0.clone()),
                        None => DialogAction::Redraw,
                    }
                }
                Self::SplitDirectionPicker { selected, filter } => {
                    let visible = split_direction_filtered_indices(filter);
                    match visible.get(*selected) {
                        Some(idx) => DialogAction::SplitDirection(SPLIT_DIRECTION_ITEMS[*idx]),
                        None => DialogAction::Redraw,
                    }
                }
                Self::CloseTargetPicker { selected, filter } => {
                    let visible = close_target_filtered_indices(filter);
                    match visible.get(*selected) {
                        Some(idx) => DialogAction::PickedCloseTarget(CLOSE_TARGET_ITEMS[*idx].0),
                        None => DialogAction::Redraw,
                    }
                }
                Self::AgentPicker {
                    agents,
                    selected,
                    intent,
                    filter,
                } => {
                    let visible = picker_filtered_rows(agents, filter);
                    match visible.get(*selected) {
                        Some(PickerRow::Agent(idx)) => DialogAction::SpawnAgent {
                            agent: Some(agents[*idx].clone()),
                            intent: *intent,
                        },
                        Some(PickerRow::Shell) => DialogAction::SpawnAgent {
                            agent: None,
                            intent: *intent,
                        },
                        Some(PickerRow::Section(_)) | None => DialogAction::Redraw,
                    }
                }
                Self::ExitDirty { selected, .. } => match EXIT_DIRTY_ROWS.get(*selected) {
                    Some((row, _)) => DialogAction::ExitDirty(*row),
                    None => DialogAction::Redraw,
                },
                _ => DialogAction::Redraw,
            },
            None => {
                if let Some(c) = printable_filter_char(key) {
                    match self {
                        Self::CommandPalette {
                            filter, selected, ..
                        }
                        | Self::SplitDirectionPicker { filter, selected }
                        | Self::CloseTargetPicker { filter, selected } => {
                            filter.push(c);
                            *selected = 0;
                        }
                        Self::AgentPicker {
                            agents,
                            filter,
                            selected,
                            ..
                        } => {
                            filter.push(c);
                            let visible = picker_filtered_rows(agents, filter);
                            *selected = first_selectable_idx(&visible);
                        }
                        _ => {}
                    }
                }
                DialogAction::Redraw
            }
        }
    }
}

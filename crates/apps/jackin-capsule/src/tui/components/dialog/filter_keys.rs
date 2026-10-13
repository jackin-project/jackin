// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Type-to-filter list key handling for picker dialogs.

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
    /// Single-unit dispatch for the type-to-filter list dialogs.
    /// `key` is one byte (or one escape sequence) — see `handle_key`
    /// for the coalesced-chunk split.
    #[expect(
        clippy::too_many_lines,
        reason = "Filter-list key dispatcher with one arm per key binding. \
                  Each arm carries its focused state transition; extracting \
                  arms into sub-dispatchers would obscure per-binding readability."
    )]
    pub(crate) fn handle_filter_list_key(&mut self, key: &[u8]) -> DialogAction {
        // From here on, only the type-to-filter list dialogs reach this
        // code path. Dispatch through `FILTER_LIST_KEYMAP`: navigation,
        // confirm, filter-backspace, and dismiss are advertised keys;
        // printable `Char` input is not in the table and falls through
        // (the `None` arm) to `printable_filter_char` filter building.
        // The dismiss surface is narrower than the read-only dialogs
        // above (`q` / Delete are typing actions that build the filter,
        // not dismiss keys); only Esc / Ctrl+C / Ctrl+Q close.
        match raw_bytes_to_chord(key).and_then(|chord| FILTER_LIST_KEYMAP.dispatch(chord)) {
            Some(FilterListAction::Dismiss) => match self {
                // Esc / Ctrl+C on the dirty-exit modal = keep changes and exit
                // (never lose work). The read-only Inspect list and every other
                // dialog dismiss normally; Inspect pops back to the modal
                // underneath via the dialog stack.
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
                        // Section row or out-of-bounds index — no
                        // action. The render path keeps `selected`
                        // on a selectable row, but a stale value
                        // (e.g. from a filter pass that emptied the
                        // list) falls through to Redraw rather than
                        // panic.
                        Some(PickerRow::Section(_)) | None => DialogAction::Redraw,
                    }
                }
                // Enter on the dirty-exit modal emits the focused row's action.
                Self::ExitDirty { selected, .. } => match EXIT_DIRTY_ROWS.get(*selected) {
                    Some((row, _)) => DialogAction::ExitDirty(*row),
                    None => DialogAction::Redraw,
                },
                _ => DialogAction::Redraw,
            },
            // Printable ASCII single-byte chunks become filter input.
            // Multi-byte chunks that reach here hold ESC (CSI fragments
            // that did not match a known key, etc.) — `handle_key` splits
            // ESC-free chunks before dispatch. They stay no-op redraws:
            // the parser already classified them, and feeding escape
            // bytes into the filter would garble the visible typing state.
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

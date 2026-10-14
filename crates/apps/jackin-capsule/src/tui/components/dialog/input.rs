// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::keymap::raw_bytes_to_chord;

use super::{CLOSE_TARGET_ITEMS, DialogAction, SPLIT_DIRECTION_ITEMS};
use crate::tui::keymap::{RENAME_KEYMAP, RenameAction};

pub(super) fn rename_tab_handle_key(
    tab_idx: usize,
    input: &mut termrock::widgets::TextInputState,
    key: &[u8],
) -> DialogAction {
    text_input_handle_key(input, key, |value| DialogAction::RenameTab {
        tab_idx,
        label: value,
    })
}

pub(super) fn export_file_handle_key(
    input: &mut termrock::widgets::TextInputState,
    reveal_after_export: bool,
    open_after_export: bool,
    key: &[u8],
) -> DialogAction {
    text_input_handle_key(input, key, |path| DialogAction::ExportFile {
        path,
        reveal_after_export,
        open_after_export,
    })
}

fn text_input_handle_key(
    input: &mut termrock::widgets::TextInputState,
    key: &[u8],
    commit: impl FnOnce(String) -> DialogAction,
) -> DialogAction {
    match raw_bytes_to_chord(key).and_then(|chord| RENAME_KEYMAP.dispatch(chord)) {
        Some(RenameAction::Dismiss) => DialogAction::Dismiss,
        Some(RenameAction::Save) => commit(input.trimmed_value().to_owned()),
        Some(RenameAction::FieldBackspace) => {
            input.apply(termrock::widgets::EditAction::Backspace);
            DialogAction::Redraw
        }
        None => {
            let Ok(s) = std::str::from_utf8(key) else {
                return DialogAction::Redraw;
            };
            for ch in s.chars() {
                if (ch.is_control() && ch != '\t') || ch == '\0' {
                    continue;
                }
                input.apply(termrock::widgets::EditAction::Insert(ch));
            }
            DialogAction::Redraw
        }
    }
}

pub(super) fn exec_picker_handle_key(
    state: &mut crate::exec::ExecPickerState,
    key: &[u8],
) -> DialogAction {
    match key {
        b" " => {
            state.toggle_cursor();
            DialogAction::Redraw
        }
        b"\x1b[A" | b"\x10" => {
            state.cursor_up();
            DialogAction::Redraw
        }
        b"\x1b[B" | b"\x0e" => {
            state.cursor_down();
            DialogAction::Redraw
        }
        b"\r" | b"\n" => DialogAction::ExecConfirm {
            command: state.command.clone(),
            args: state.args.clone(),
            selected: state.selected_refs(),
        },
        b"\x1b" | b"\x03" => DialogAction::ExecCancel,
        _ => DialogAction::Consume,
    }
}

pub(super) fn printable_filter_char(key: &[u8]) -> Option<char> {
    if key.len() != 1 {
        return None;
    }
    let b = key[0];
    if (0x20..=0x7e).contains(&b) {
        Some(b as char)
    } else {
        None
    }
}

pub(super) fn close_target_filtered_indices(filter: &str) -> Vec<usize> {
    let needle = filter.to_ascii_lowercase();
    CLOSE_TARGET_ITEMS
        .iter()
        .enumerate()
        .filter(|(_, (_, label))| needle.is_empty() || label.to_ascii_lowercase().contains(&needle))
        .map(|(idx, _)| idx)
        .collect()
}

pub(super) fn split_direction_filtered_indices(filter: &str) -> Vec<usize> {
    let needle = filter.to_ascii_lowercase();
    SPLIT_DIRECTION_ITEMS
        .iter()
        .enumerate()
        .filter(|(_, dir)| needle.is_empty() || dir.label().to_ascii_lowercase().contains(&needle))
        .map(|(idx, _)| idx)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PickerRow {
    Section(&'static str),
    Agent(usize),
    Shell,
}

impl PickerRow {
    fn is_selectable(self) -> bool {
        !matches!(self, Self::Section(_))
    }
}

pub(super) fn picker_filtered_rows(agents: &[String], filter: &str) -> Vec<PickerRow> {
    let needle = filter.to_ascii_lowercase();
    let agent_matches: Vec<PickerRow> = agents
        .iter()
        .enumerate()
        .filter(|(_, slug)| {
            let label =
                crate::tui::components::agent_display_name(slug.as_str()).unwrap_or(slug.as_str());
            needle.is_empty() || label.to_ascii_lowercase().contains(&needle)
        })
        .map(|(idx, _)| PickerRow::Agent(idx))
        .collect();
    let shell_match = needle.is_empty() || "shell".contains(&needle);

    let mut out = Vec::with_capacity(agent_matches.len() + 3);
    if !agent_matches.is_empty() {
        out.push(PickerRow::Section("agents"));
        out.extend(agent_matches);
    }
    if shell_match {
        out.push(PickerRow::Section("shells"));
        out.push(PickerRow::Shell);
    }
    out
}

pub(super) fn first_selectable_idx(rows: &[PickerRow]) -> usize {
    rows.iter().position(|r| r.is_selectable()).unwrap_or(0)
}

pub(super) fn step_selectable(rows: &[PickerRow], from: usize, forward: bool) -> usize {
    if rows.is_empty() {
        return 0;
    }
    let last = rows.len() - 1;
    let mut idx = from.min(last);
    loop {
        let next = if forward {
            if idx >= last {
                break;
            }
            idx + 1
        } else if idx == 0 {
            break;
        } else {
            idx - 1
        };
        idx = next;
        if rows[idx].is_selectable() {
            return idx;
        }
    }
    if rows[idx].is_selectable() {
        idx
    } else if forward {
        (0..idx)
            .rev()
            .find(|&i| rows[i].is_selectable())
            .unwrap_or(idx)
    } else {
        (idx + 1..rows.len())
            .find(|&i| rows[i].is_selectable())
            .unwrap_or(idx)
    }
}

pub(super) fn dialog_list_row_clickable(row: u16, box_row: u16, visible_count: usize) -> bool {
    let first_item_row = box_row + 3;
    row >= first_item_row
        && row < first_item_row.saturating_add(u16::try_from(visible_count).unwrap_or(u16::MAX))
}

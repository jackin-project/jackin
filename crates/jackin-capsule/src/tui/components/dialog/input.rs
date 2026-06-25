//! Key, filter, and list-hit helpers for capsule dialogs.

use jackin_tui::components::raw_bytes_to_chord;

use super::{CLOSE_TARGET_ITEMS, DialogAction, SPLIT_DIRECTION_ITEMS};
use crate::tui::keymap::{RENAME_KEYMAP, RenameAction};

/// Edit a rename-tab input buffer in response to a raw key chunk.
/// Dispatches advertised keys through [`RENAME_KEYMAP`]: Enter commits,
/// Esc / Ctrl+C / Ctrl+Q cancel, Backspace removes the trailing char.
/// Any other printable chunk appends (the keymap `None` arm). Length cap
/// and printable filter live inside `jackin_tui::TextField` so this
/// handler only needs to dispatch key bytes — the buffer math is shared
/// with the console TUI surface.
pub(super) fn rename_tab_handle_key(
    tab_idx: usize,
    input: &mut jackin_tui::TextField,
    key: &[u8],
) -> DialogAction {
    match raw_bytes_to_chord(key).and_then(|chord| RENAME_KEYMAP.dispatch(chord)) {
        Some(RenameAction::Dismiss) => DialogAction::Dismiss,
        Some(RenameAction::Save) => DialogAction::RenameTab {
            tab_idx,
            label: input.trimmed_value(),
        },
        Some(RenameAction::FieldBackspace) => {
            input.backspace();
            DialogAction::Redraw
        }
        None => {
            // Accept any valid UTF-8 chunk one char at a time so CJK /
            // emoji / combining-mark labels reach `TextField` and match
            // the unicode-width measurement `lay_out_tabs` uses for
            // tab-strip rendering. C0 controls (other than the explicit
            // Esc / Enter / Backspace arms above) and invalid UTF-8
            // chunks fall through as a Redraw no-op.
            let Ok(s) = std::str::from_utf8(key) else {
                return DialogAction::Redraw;
            };
            for ch in s.chars() {
                if (ch.is_control() && ch != '\t') || ch == '\0' {
                    continue;
                }
                input.insert_char(ch);
            }
            DialogAction::Redraw
        }
    }
}

/// Drive the `jackin-exec` credential picker from a raw key chunk.
///
/// Space toggles the row under the cursor (multi-select); ↑/↓ move the cursor;
/// Enter confirms with the selected credentials; Esc / Ctrl+C cancel. Any other
/// key is consumed so it never reaches the focused pane behind the modal.
pub(super) fn exec_picker_handle_key(
    state: &mut crate::exec::ExecPickerState,
    key: &[u8],
) -> DialogAction {
    match key {
        b" " => {
            state.toggle_cursor();
            DialogAction::Redraw
        }
        // Up arrow / Ctrl+P.
        b"\x1b[A" | b"\x10" => {
            state.cursor_up();
            DialogAction::Redraw
        }
        // Down arrow / Ctrl+N.
        b"\x1b[B" | b"\x0e" => {
            state.cursor_down();
            DialogAction::Redraw
        }
        // Enter — confirm and resolve the selected credentials.
        b"\r" | b"\n" => DialogAction::ExecConfirm {
            command: state.command.clone(),
            args: state.args.clone(),
            selected: state.selected_refs(),
        },
        // Esc / Ctrl+C — cancel, run nothing.
        b"\x1b" | b"\x03" => DialogAction::ExecCancel,
        _ => DialogAction::Consume,
    }
}

/// Filterable dialogs accept printable ASCII (0x20..=0x7e) as filter
/// input. Multi-byte sequences fall through as no-op redraws — they
/// were already classified by the parser (or arrived unrecognised),
/// and feeding them into the filter would garble the visible typing
/// state. Operators who need non-ASCII filtering can fall back to
/// arrow navigation.
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

/// Indices into `CLOSE_TARGET_ITEMS` whose label contains `filter`
/// as a case-insensitive substring. Empty filter returns every item.
pub(super) fn close_target_filtered_indices(filter: &str) -> Vec<usize> {
    let needle = filter.to_ascii_lowercase();
    CLOSE_TARGET_ITEMS
        .iter()
        .enumerate()
        .filter(|(_, (_, label))| needle.is_empty() || label.to_ascii_lowercase().contains(&needle))
        .map(|(idx, _)| idx)
        .collect()
}

/// Indices into `SPLIT_DIRECTION_ITEMS` whose label contains `filter`
/// as a case-insensitive substring. Empty filter returns every item.
pub(super) fn split_direction_filtered_indices(filter: &str) -> Vec<usize> {
    let needle = filter.to_ascii_lowercase();
    SPLIT_DIRECTION_ITEMS
        .iter()
        .enumerate()
        .filter(|(_, dir)| needle.is_empty() || dir.label().to_ascii_lowercase().contains(&needle))
        .map(|(idx, _)| idx)
        .collect()
}

/// One renderable row inside an `AgentPicker` after filtering. The
/// `Section` variant carries a non-selectable label that groups the
/// selectable rows beneath it ("agents" before agent rows, "shells"
/// before the shell row) so the operator visually distinguishes the
/// two kinds of session jackin can spawn. Future shell variants
/// (zsh, bash, fish) will land under the same "shells" section.
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

/// Filtered + grouped row list for the current input. Two groups —
/// "agents" first, "shells" last — separated by section labels.
/// A group whose items have all been filtered out is dropped entirely
/// (label included) so the dialog never paints a "shells" header
/// with no items underneath it. Each item passes the filter when its
/// display label (via `jackin_tui::agent_display_name` for agents,
/// the literal `"Shell"` for the shell row) contains the filter as a
/// case-insensitive substring.
pub(super) fn picker_filtered_rows(agents: &[String], filter: &str) -> Vec<PickerRow> {
    let needle = filter.to_ascii_lowercase();
    let agent_matches: Vec<PickerRow> = agents
        .iter()
        .enumerate()
        .filter(|(_, slug)| {
            let label = jackin_tui::agent_display_name(slug.as_str()).unwrap_or(slug.as_str());
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

/// First selectable index in `rows`, or `0` when the list is empty
/// (the caller never indexes into an empty list because the render
/// path paints nothing in that state).
pub(super) fn first_selectable_idx(rows: &[PickerRow]) -> usize {
    rows.iter().position(|r| r.is_selectable()).unwrap_or(0)
}

/// Advance `from` to the next selectable index in `from..rows.len()`
/// when `forward = true`, or to the previous selectable in `0..from`
/// when `false`. Clamps at the bounds (no wrap). Section rows are
/// skipped transparently so an arrow keypress moves from one item
/// to the next without parking on a label.
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
    // Reached an edge while skipping sections. Fall back to whatever
    // the nearest selectable in the opposite direction is so the
    // selection never lands on a label.
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
    row >= first_item_row && row < first_item_row + visible_count as u16
}

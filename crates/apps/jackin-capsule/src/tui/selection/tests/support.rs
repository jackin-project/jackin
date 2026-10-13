// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn row(text: &str) -> RowSnapshot {
    RowSnapshot {
        cells: text
            .chars()
            .map(|ch| CellSnapshot {
                contents: ch.to_string(),
                width: u16::try_from(UnicodeWidthChar::width(ch).unwrap_or(1)).unwrap_or(1),
            })
            .collect(),
    }
}

pub(super) fn word_at(text: &str, probe: &str) -> Option<String> {
    let snapshot = row(text);
    let probe_start = text.find(probe).expect("probe in line");
    let probe_char_idx = text[..probe_start].chars().count() + probe.chars().count() / 2;
    let cells = snapshot.display_cells();
    let col = cells[probe_char_idx].start_col;
    let (start, end) = word_bounds_in_row(&snapshot, col)?;
    Some(snapshot.text_range(start, end))
}

pub(super) fn word_at_col(text: &str, col: u16) -> Option<String> {
    let snapshot = row(text);
    let (start, end) = word_bounds_in_row(&snapshot, col)?;
    Some(snapshot.text_range(start, end))
}

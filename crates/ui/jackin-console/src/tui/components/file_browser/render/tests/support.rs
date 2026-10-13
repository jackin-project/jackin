// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn make_state_at(path: PathBuf) -> FileBrowserState {
    FileBrowserState::from_listing(crate::services::file_browser::listing_at(
        path.clone(),
        path,
    ))
}

pub(super) fn row_string(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

pub(super) fn char_column(row: &str, needle: &str) -> usize {
    let byte = row
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} should appear in {row:?}"));
    row[..byte].chars().count()
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::components::modal_overlay::file_browser_overlay_rect;

pub(super) mod git_prompt;
pub(super) mod input;
pub mod listing;
pub(super) mod render;
pub(super) mod state;

pub use git_prompt::{GitPromptFocus, git_prompt_rect, git_prompt_url_row_rect};
pub use input::FileBrowserOutcome;
pub use listing::{FolderEntry, FolderListing};
pub use render::{listing_rect, render};
pub use state::FileBrowserState;

#[must_use]
pub fn page_rows_for_modal(term_size: ratatui::layout::Rect, state: &FileBrowserState) -> u16 {
    let modal_area = file_browser_overlay_rect(term_size);
    let listing_area = listing_rect(modal_area, state.rejected_reason.is_some());
    u16::try_from(termrock::scroll::viewport_height(listing_area)).unwrap_or(u16::MAX)
}

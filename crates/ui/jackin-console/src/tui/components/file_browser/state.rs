// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use super::git_prompt::GitPromptFocus;
use super::listing::{FolderEntry, FolderListing};
use super::render::selection_hit_at;
use ratatui::layout::Position;
use termrock::widgets::ListState;

#[derive(Debug)]
pub struct FileBrowserState {
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub entries: Vec<FolderEntry>,
    pub list_state: ListState<usize>,
    pub rejected_reason: Option<String>,
    pub show_hidden: bool,
    pub pending_git_prompt: Option<PathBuf>,
    pub pending_git_url: Option<String>,
    pub(super) pending_git_url_rx:
        Option<crate::tui::runtime::BlockingSubscription<Option<String>>>,
    pub pending_git_focus: GitPromptFocus,
}

impl FileBrowserState {
    pub fn footer_items(&self) -> Vec<termrock::widgets::HintSpan<'static>> {
        use termrock::widgets::HintSpan;
        if self.pending_git_prompt.is_some() {
            super::git_prompt::git_prompt_footer_items(self.pending_git_url.is_some())
        } else {
            use termrock::keymap::glyph;
            vec![
                HintSpan::Key("\u{2191}\u{2193}/j/k"),
                HintSpan::Text("navigate"),
                HintSpan::GroupSep,
                HintSpan::Key(glyph::PGUP_PGDN),
                HintSpan::Text("page"),
                HintSpan::GroupSep,
                HintSpan::Key("↵/l"),
                HintSpan::Text("open"),
                HintSpan::GroupSep,
                HintSpan::Key("H/h/\u{2190}"),
                HintSpan::Text("up"),
                HintSpan::GroupSep,
                HintSpan::Key("S"),
                HintSpan::Text("select"),
                HintSpan::GroupSep,
                HintSpan::Key("Esc"),
                HintSpan::Text("up/cancel"),
            ]
        }
    }

    pub fn from_listing(listing: FolderListing) -> Self {
        let FolderListing { root, cwd, entries } = listing;
        let list_state = ListState::for_count(entries.len());
        Self {
            root,
            cwd,
            entries,
            list_state,
            show_hidden: false,
            rejected_reason: None,
            pending_git_prompt: None,
            pending_git_url: None,
            pending_git_url_rx: None,
            pending_git_focus: GitPromptFocus::MountHere,
        }
    }

    pub fn apply_listing(&mut self, listing: FolderListing) {
        self.root = listing.root;
        self.cwd = listing.cwd;
        self.entries = listing.entries;
        self.list_state = ListState::for_count(self.entries.len());
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub(super) fn select_next(&mut self) {
        self.list_state.cycle_index(self.entries.len(), 1);
    }

    pub(super) fn select_prev(&mut self) {
        self.list_state.cycle_index(self.entries.len(), -1);
    }

    pub fn scroll_selection(&mut self, delta: i16) -> bool {
        self.list_state
            .move_index(self.entries.len(), isize::from(delta))
    }

    pub fn scroll_selection_at(
        &mut self,
        area: ratatui::layout::Rect,
        column: u16,
        row: u16,
        delta: i16,
    ) -> bool {
        if self.pending_git_prompt.is_some()
            || !selection_hit_at(area, self, Position::new(column, row))
        {
            return false;
        }
        let _changed = self.scroll_selection(delta);
        true
    }

    pub fn page_selection(&mut self, rows: u16, direction: i16) -> bool {
        let rows = i16::try_from(rows.max(1)).unwrap_or(i16::MAX);
        self.scroll_selection(rows.saturating_mul(direction.signum()))
    }

    pub(super) fn highlighted(&self) -> Option<&FolderEntry> {
        self.list_state.selected_item(&self.entries)
    }

    pub fn reject_commit(&mut self, reason: String) {
        self.rejected_reason = Some(reason);
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! File-browser terminal state.
//!
//! Houses `FileBrowserState` (the modal's working state). Directory scanning,
//! sandbox policy, and git-origin inspection live in `services::file_browser`.

use std::path::{Path, PathBuf};

use super::git_prompt::GitPromptFocus;
use super::listing::{FolderEntry, FolderListing};
use crate::tui::layout::point_in_rect;
use termrock::widgets::ListState;

#[derive(Debug)]
pub struct FileBrowserState {
    /// $HOME — the browser cannot navigate above this path.
    pub root: PathBuf,
    /// Currently-displayed directory.
    pub cwd: PathBuf,
    /// Entries loaded from `cwd`, after filtering + sorting.
    pub entries: Vec<FolderEntry>,
    /// Canonical `TermRock` selection state. Drives which row is highlighted.
    pub list_state: ListState<usize>,
    /// Set when the operator presses `s` but the selection is rejected
    /// (e.g. `$HOME` itself, `~/.jackin/...`). Cleared on the next key.
    pub rejected_reason: Option<String>,
    /// Show hidden (dot-prefixed) directories in the listing.
    ///
    /// Default `false` (mounts flow); `true` for the auth source-folder picker
    /// so dotfile credential dirs like `~/.claude` are reachable.
    pub show_hidden: bool,
    /// Active when the operator has pressed Enter on a git-repo row.
    pub pending_git_prompt: Option<PathBuf>,
    /// Origin URL (web form) for the repo referenced by
    /// `pending_git_prompt`. `None` for non-GitHub remotes or any repo
    /// whose origin can't be resolved — the overlay then omits the row.
    pub pending_git_url: Option<String>,
    pub(super) pending_git_url_rx:
        Option<crate::tui::runtime::BlockingSubscription<Option<String>>>,
    /// Which button is highlighted in the git-repo prompt.
    pub pending_git_focus: GitPromptFocus,
}

impl FileBrowserState {
    /// Footer-bar hints for the current state. The screen footer renders these
    /// (hints are footer-only — the browser draws no internal hint row); the
    /// git-repo confirm overlay swaps in its own confirm/cancel keys.
    pub fn footer_items(&self) -> Vec<termrock::widgets::HintSpan<'static>> {
        use termrock::widgets::HintSpan;
        if self.pending_git_prompt.is_some() {
            super::git_prompt::git_prompt_footer_items(self.pending_git_url.is_some())
        } else {
            use termrock::keymap::glyph;
            vec![
                // UNREGISTERABLE(multi-key-display-group): ↑↓/j/k combines arrow keys and vim aliases.
                HintSpan::Key("\u{2191}\u{2193}/j/k"),
                HintSpan::Text("navigate"),
                HintSpan::GroupSep,
                // UNREGISTERABLE(multi-key-display-group): PgUp/PgDn combined display.
                HintSpan::Key(glyph::PGUP_PGDN),
                HintSpan::Text("page"),
                HintSpan::GroupSep,
                // UNREGISTERABLE(multi-key-display-group): ↵/l combines Enter and vim right.
                HintSpan::Key("↵/l"),
                HintSpan::Text("open"),
                HintSpan::GroupSep,
                // UNREGISTERABLE(multi-key-display-group): H/h/← combines three up-directory bindings.
                HintSpan::Key("H/h/\u{2190}"),
                HintSpan::Text("up"),
                HintSpan::GroupSep,
                // UNREGISTERABLE(file-browser-no-keymap): S selects inline; no FILE_BROWSER_KEYMAP registered.
                HintSpan::Key("S"),
                HintSpan::Text("select"),
                HintSpan::GroupSep,
                // UNREGISTERABLE(file-browser-no-keymap): Esc handled inline.
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

    /// Current working directory. Exposed so the create-workspace wizard
    /// can breadcrumb this across a step-back.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Move selection down one, wrapping at the end.
    pub(super) fn select_next(&mut self) {
        self.list_state.cycle_index(self.entries.len(), 1);
    }

    /// Move selection up one, wrapping at the start.
    pub(super) fn select_prev(&mut self) {
        self.list_state.cycle_index(self.entries.len(), -1);
    }

    /// Move selection by wheel delta without wrapping.
    ///
    /// Keyboard navigation wraps to keep repeated arrow presses efficient.
    /// Wheel gestures should instead saturate at the listing edges, matching
    /// normal scroll behavior and preventing an edge scroll from jumping from
    /// the top to the bottom.
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
        if self.pending_git_prompt.is_some() || !point_in_rect(column, row, area) {
            return false;
        }
        let _changed = self.scroll_selection(delta);
        true
    }

    pub fn page_selection(&mut self, rows: u16, direction: i16) -> bool {
        let rows = i16::try_from(rows.max(1)).unwrap_or(i16::MAX);
        self.scroll_selection(rows.saturating_mul(direction.signum()))
    }

    /// The currently-highlighted entry, if any.
    pub(super) fn highlighted(&self) -> Option<&FolderEntry> {
        self.list_state.selected_item(&self.entries)
    }

    pub fn reject_commit(&mut self, reason: String) {
        self.rejected_reason = Some(reason);
    }
}

#[cfg(test)]
mod tests;

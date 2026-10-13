// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Keyboard + mouse event handling for the file browser.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;

use super::git_prompt::git_prompt_url_row_rect;
use super::state::FileBrowserState;

/// Semantic result from file-browser input. External work such as opening
/// URLs is requested here and executed by the owning console input layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBrowserOutcome<T> {
    Continue,
    Commit(T),
    Cancel,
    OpenGitUrl(String),
    ResolveGitUrl(PathBuf),
    NavigateTo(PathBuf),
    NavigateUp,
    RequestCommit(PathBuf),
}

impl FileBrowserState {
    pub fn handle_key(&mut self, key: KeyEvent) -> FileBrowserOutcome<PathBuf> {
        self.handle_key_with_page_rows(key, None)
    }

    pub fn handle_key_with_page_rows(
        &mut self,
        key: KeyEvent,
        page_rows: Option<u16>,
    ) -> FileBrowserOutcome<PathBuf> {
        // Git-repo prompt has its own key map; delegate before clearing
        // any state the main handler would otherwise reset.
        if self.pending_git_prompt.is_some() {
            return self.handle_git_prompt_key(key);
        }

        // Clear any stale rejection on the next keypress.
        self.rejected_reason = None;

        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                self.select_prev();
                FileBrowserOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                self.select_next();
                FileBrowserOutcome::Continue
            }
            KeyCode::PageUp => {
                if let Some(rows) = page_rows {
                    let _changed = self.page_selection(rows, -1);
                }
                FileBrowserOutcome::Continue
            }
            KeyCode::PageDown => {
                if let Some(rows) = page_rows {
                    let _changed = self.page_selection(rows, 1);
                }
                FileBrowserOutcome::Continue
            }
            KeyCode::Left | KeyCode::Char('h' | 'H') => FileBrowserOutcome::NavigateUp,
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l' | 'L') => self.handle_enter(),
            KeyCode::Char('s' | 'S') => {
                // Prefer the highlighted entry (so the operator can pick a
                // sibling without first Entering). Fall back to cwd when
                // there's no real selection (empty listing, `..` row).
                let target = self
                    .highlighted()
                    .filter(|e| !e.is_parent)
                    .map_or_else(|| self.cwd.clone(), |e| e.path.clone());
                Self::commit_or_reject(target)
            }
            KeyCode::Esc => {
                // Esc steps back one directory when the operator has
                // drilled below root — mirroring `h` / `←`. Only cancels
                // the modal when already at root. `rejected_reason` was
                // cleared above; preserve that in both branches.
                if self.cwd == self.root {
                    FileBrowserOutcome::Cancel
                } else {
                    FileBrowserOutcome::NavigateUp
                }
            }
            _ => FileBrowserOutcome::Continue,
        }
    }

    /// Shared path for Enter / l / → on a highlighted entry: navigate
    /// into dirs, open the git-repo prompt on repo rows, up on the `..` row.
    pub(super) fn handle_enter(&mut self) -> FileBrowserOutcome<PathBuf> {
        let Some(entry) = self.highlighted().cloned() else {
            return FileBrowserOutcome::Continue;
        };
        if entry.is_parent {
            return FileBrowserOutcome::NavigateUp;
        }
        if entry.is_git {
            let path = entry.path;
            self.open_git_prompt(path.clone());
            return FileBrowserOutcome::ResolveGitUrl(path);
        }
        FileBrowserOutcome::NavigateTo(entry.path)
    }

    /// Shared commit-or-reject logic used by `s` and the git-repo prompt's
    /// "Mount this repository" option. Enforces the same sandbox rules.
    pub(super) fn commit_or_reject(target: PathBuf) -> FileBrowserOutcome<PathBuf> {
        FileBrowserOutcome::RequestCommit(target)
    }

    /// Return the URL requested by a left-click at `(column, row)` in
    /// absolute terminal coordinates while `modal_area` hosts this browser.
    /// A matching click doesn't dismiss the prompt, matching the keyboard
    /// keypath.
    pub fn url_to_open_on_click(&self, modal_area: Rect, column: u16, row: u16) -> Option<String> {
        if !self.url_row_hit(modal_area, column, row) {
            return None;
        }
        self.pending_git_url.clone()
    }

    /// Side-effect-free hit-test: whether `(column, row)` lands on the
    /// git-prompt's clickable URL row (prompt active and a URL resolved).
    /// Separated from [`Self::url_to_open_on_click`] so the hover
    /// hand-pointer cue can test the same geometry without opening the URL.
    #[must_use]
    pub fn url_row_hit(&self, modal_area: Rect, column: u16, row: u16) -> bool {
        if self.pending_git_prompt.is_none() || self.pending_git_url.is_none() {
            return false;
        }
        let has_rejection = self.rejected_reason.is_some();
        let Some(url_rect) = git_prompt_url_row_rect(modal_area, has_rejection) else {
            return false;
        };
        column >= url_rect.x && column < url_rect.x + url_rect.width && row == url_rect.y
    }
}

#[cfg(test)]
mod tests;

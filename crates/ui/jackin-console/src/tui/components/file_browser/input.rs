// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;

use super::git_prompt::git_prompt_url_row_rect;
use super::state::FileBrowserState;

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
        if self.pending_git_prompt.is_some() {
            return self.handle_git_prompt_key(key);
        }

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
                let target = self
                    .highlighted()
                    .filter(|e| !e.is_parent)
                    .map_or_else(|| self.cwd.clone(), |e| e.path.clone());
                Self::commit_or_reject(target)
            }
            KeyCode::Esc => {
                if self.cwd == self.root {
                    FileBrowserOutcome::Cancel
                } else {
                    FileBrowserOutcome::NavigateUp
                }
            }
            _ => FileBrowserOutcome::Continue,
        }
    }

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

    pub(super) fn commit_or_reject(target: PathBuf) -> FileBrowserOutcome<PathBuf> {
        FileBrowserOutcome::RequestCommit(target)
    }

    pub fn url_to_open_on_click(&self, modal_area: Rect, column: u16, row: u16) -> Option<String> {
        if !self.url_row_hit(modal_area, column, row) {
            return None;
        }
        self.pending_git_url.clone()
    }

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

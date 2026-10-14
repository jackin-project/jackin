// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::tui::runtime::SubscriptionPoll;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::input::FileBrowserOutcome;
use super::state::FileBrowserState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitPromptFocus {
    MountHere,
    EnterIn,
    Cancel,
}

impl FileBrowserState {
    pub(super) fn dismiss_git_prompt(&mut self) {
        self.pending_git_prompt = None;
        self.pending_git_url = None;
        self.pending_git_url_rx = None;
    }

    pub(super) fn open_git_prompt(&mut self, path: PathBuf) {
        self.pending_git_url = None;
        self.pending_git_url_rx = None;
        self.pending_git_prompt = Some(path);
        self.pending_git_focus = GitPromptFocus::MountHere;
    }

    pub fn attach_git_url_resolution(
        &mut self,
        rx: crate::tui::runtime::BlockingSubscription<Option<String>>,
    ) {
        self.pending_git_url = None;
        self.pending_git_url_rx = Some(rx);
    }

    #[must_use]
    pub fn poll_git_url_resolution(&mut self) -> bool {
        let Some(rx) = self.pending_git_url_rx.as_mut() else {
            return false;
        };
        match rx.poll_next() {
            SubscriptionPoll::Ready(url) => {
                self.pending_git_url = url;
                self.pending_git_url_rx = None;
                true
            }
            SubscriptionPoll::Pending => false,
            SubscriptionPoll::Closed => {
                self.pending_git_url_rx = None;
                false
            }
        }
    }

    pub(super) fn handle_git_prompt_key(&mut self, key: KeyEvent) -> FileBrowserOutcome<PathBuf> {
        let Some(path) = self.pending_git_prompt.clone() else {
            return FileBrowserOutcome::Continue;
        };
        match key.code {
            KeyCode::Char('m' | 'M') => {
                self.dismiss_git_prompt();
                Self::commit_or_reject(path)
            }
            KeyCode::Char('p' | 'P') => {
                self.dismiss_git_prompt();
                FileBrowserOutcome::NavigateTo(path)
            }
            KeyCode::Char('o' | 'O') => self
                .pending_git_url
                .clone()
                .map_or(FileBrowserOutcome::Continue, FileBrowserOutcome::OpenGitUrl),
            KeyCode::Char('c' | 'C') | KeyCode::Esc => {
                self.dismiss_git_prompt();
                FileBrowserOutcome::Continue
            }
            KeyCode::Enter => {
                let focus = self.pending_git_focus;
                self.dismiss_git_prompt();
                match focus {
                    GitPromptFocus::MountHere => Self::commit_or_reject(path),
                    GitPromptFocus::EnterIn => FileBrowserOutcome::NavigateTo(path),
                    GitPromptFocus::Cancel => FileBrowserOutcome::Continue,
                }
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l' | 'L') => {
                self.pending_git_focus = match self.pending_git_focus {
                    GitPromptFocus::MountHere => GitPromptFocus::EnterIn,
                    GitPromptFocus::EnterIn => GitPromptFocus::Cancel,
                    GitPromptFocus::Cancel => GitPromptFocus::MountHere,
                };
                FileBrowserOutcome::Continue
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h' | 'H') => {
                self.pending_git_focus = match self.pending_git_focus {
                    GitPromptFocus::MountHere => GitPromptFocus::Cancel,
                    GitPromptFocus::EnterIn => GitPromptFocus::MountHere,
                    GitPromptFocus::Cancel => GitPromptFocus::EnterIn,
                };
                FileBrowserOutcome::Continue
            }
            _ => FileBrowserOutcome::Continue,
        }
    }
}

pub fn git_prompt_rect(listing: Rect, has_url: bool) -> Option<Rect> {
    let w = listing.width.saturating_sub(4).min(80);
    let base_h: u16 = if has_url { 8 } else { 7 };
    let h = base_h.min(listing.height);
    if w == 0 || h == 0 {
        return None;
    }
    let x = listing.x + listing.width.saturating_sub(w) / 2;
    let y = listing.y + listing.height.saturating_sub(h) / 2;
    Some(Rect {
        x,
        y,
        width: w,
        height: h,
    })
}

pub fn git_prompt_url_row_rect(modal_area: Rect, has_rejection: bool) -> Option<Rect> {
    let listing = super::render::listing_rect(modal_area, has_rejection);
    let overlay = git_prompt_rect(listing, true)?;
    let inner = Rect {
        x: overlay.x.saturating_add(1),
        y: overlay.y.saturating_add(1),
        width: overlay.width.saturating_sub(2),
        height: overlay.height.saturating_sub(2),
    };
    let (content, _) = crate::tui::dialog_layout::dialog_content_and_actions(inner);
    if content.height < 2 {
        return None;
    }
    Some(Rect {
        x: content.x,
        y: content.y.saturating_add(1),
        width: content.width,
        height: 1,
    })
}

pub(super) fn git_prompt_buttons(focus: GitPromptFocus) -> Line<'static> {
    let focused = Style::default()
        .bg(termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Text)
            .fg
            .unwrap_or_default())
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let unfocused = Style::default()
        .fg(termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Accent)
            .fg
            .unwrap_or_default())
        .add_modifier(Modifier::BOLD);
    let btn = |target: GitPromptFocus, label: &'static str| -> Span<'static> {
        let style = if focus == target { focused } else { unfocused };
        Span::styled(format!("  {label}  "), style)
    };
    Line::from(vec![
        btn(GitPromptFocus::MountHere, "Mount this repository"),
        Span::raw("    "),
        btn(GitPromptFocus::EnterIn, "Pick a subdirectory"),
        Span::raw("    "),
        btn(GitPromptFocus::Cancel, "Cancel"),
    ])
}

pub(super) fn git_prompt_footer_items(has_url: bool) -> Vec<termrock::widgets::HintSpan<'static>> {
    use termrock::widgets::HintSpan;
    let mut spans = vec![
        HintSpan::Key("M"),
        HintSpan::Text("mount"),
        HintSpan::Sep,
        HintSpan::Key("P"),
        HintSpan::Text("pick"),
    ];
    if has_url {
        spans.extend([HintSpan::Sep, HintSpan::Key("O"), HintSpan::Text("open")]);
    }
    spans.extend([
        HintSpan::GroupSep,
        HintSpan::Key("\u{21e5}/\u{2192}"),
        HintSpan::Text("next"),
        HintSpan::Sep,
        HintSpan::Key("\u{21e4}/\u{2190}"),
        HintSpan::Text("prev"),
        HintSpan::Sep,
        HintSpan::Key("\u{21b5}"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        HintSpan::Key("C/Esc"),
        HintSpan::Text("cancel"),
    ]);
    spans
}

pub(super) fn render_git_prompt(frame: &mut Frame<'_>, parent: Rect, state: &FileBrowserState) {
    use ratatui::layout::Alignment;

    let has_url = state.pending_git_url.is_some();
    let Some(area) = git_prompt_rect(parent, has_url) else {
        return;
    };

    let inner = termrock::layout::render_dialog_shell(
        frame,
        area,
        Some("Git repository detected"),
        termrock::widgets::PanelChrome::Focused,
        &termrock::style::DesignSystem::default(),
    );

    let (content, action_row) = crate::tui::dialog_layout::dialog_content_and_actions(inner);
    let prompt_row = Rect {
        height: 1,
        ..content
    };

    frame.render_widget(
        Paragraph::new(Span::styled(
            "What would you like to do?",
            termrock::style::DesignSystem::default().style(termrock::style::Role::TextStrong),
        ))
        .alignment(Alignment::Center),
        prompt_row,
    );

    if has_url {
        let url = state.pending_git_url.as_deref().unwrap_or_default();
        let url_row = Rect {
            y: content.y.saturating_add(1),
            height: 1,
            ..content
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                url.to_owned(),
                Style::default()
                    .fg(termrock::style::DesignSystem::default()
                        .style(termrock::style::Role::TextMuted)
                        .fg
                        .unwrap_or_default())
                    .add_modifier(Modifier::ITALIC),
            ))
            .alignment(Alignment::Center),
            url_row,
        );
    }

    frame.render_widget(
        Paragraph::new(git_prompt_buttons(state.pending_git_focus)).alignment(Alignment::Center),
        action_row,
    );
}

#[cfg(test)]
mod tests;

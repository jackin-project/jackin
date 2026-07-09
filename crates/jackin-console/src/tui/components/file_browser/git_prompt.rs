//! Git-repo-detected prompt: state machine + geometry + render.
//!
//! When the operator hits Enter on a row whose path contains a `.git`,
//! we pause navigation and show a small modal asking what to do
//! (mount / pick-subdir / cancel / open-in-browser). This module owns
//! the focus enum, the per-prompt key handler, and the overlay
//! rendering. Git-origin inspection lives in `services::file_browser`;
//! browser launching is requested as an input outcome for the owning
//! console input layer to execute.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use jackin_tui::runtime::{Subscription, SubscriptionPoll};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::input::FileBrowserOutcome;
use super::state::FileBrowserState;
use super::{PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

/// Focus target for the in-browser "git-repo row, what now?" prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitPromptFocus {
    /// Commit the git-repo path as the selected path (same effect as `s`).
    MountHere,
    /// Navigate into the repo directory (today's Enter behavior).
    EnterIn,
    /// Dismiss the prompt and return to the listing.
    Cancel,
}

impl FileBrowserState {
    /// Clear the git-repo prompt state in one shot — both the pending
    /// path and the resolved URL.
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
        rx: jackin_tui::runtime::BlockingSubscription<Option<String>>,
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

    /// Key handler used while the git-repo prompt is active.
    pub(super) fn handle_git_prompt_key(&mut self, key: KeyEvent) -> FileBrowserOutcome<PathBuf> {
        let Some(path) = self.pending_git_prompt.clone() else {
            return FileBrowserOutcome::Continue;
        };
        match key.code {
            KeyCode::Char('m' | 'M') => {
                self.dismiss_git_prompt();
                self.commit_or_reject(path)
            }
            // `p` for "pick a subdirectory" — matches the button label
            // (renamed from `Enter` to `Pick` in batch 16).
            KeyCode::Char('p' | 'P') => {
                self.dismiss_git_prompt();
                FileBrowserOutcome::NavigateTo(path)
            }
            // `o` for "open the repo's web URL in the browser" — best-effort.
            // No-op when `pending_git_url` is `None` (non-GitHub origin or
            // unresolvable remote); launcher failures are logged on the
            // `--debug` channel since `FileBrowserState` has no error surface.
            // The overlay drops the `· O open` hint segment in the None case
            // so the keystroke is only advertised when it actually does something.
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
                    GitPromptFocus::MountHere => self.commit_or_reject(path),
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

/// Rect of the git-repo prompt overlay, mirroring the geometry in
/// `render_git_prompt`. Returns `None` when the overlay would exceed the
/// listing area.
pub fn git_prompt_rect(listing: Rect, has_url: bool) -> Option<Rect> {
    // Structural exception: git prompt is a child overlay constrained by the File Browser listing rect, not a top-level modal.
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

/// Rect of the URL row inside the git-prompt overlay, in absolute
/// screen coordinates. Returns `None` when `has_url` is false — the
/// URL row isn't rendered then and a click there shouldn't open anything.
///
/// Row order inside the overlay follows the canonical five-slot dialog layout:
/// `[leading spacer][prompt + url][spacer][buttons][trailing spacer]`.
/// The URL row sits at content row index 1 when the URL is present.
pub fn git_prompt_url_row_rect(modal_area: Rect, has_rejection: bool) -> Option<Rect> {
    // Structural exception: URL hit-testing is derived from the child overlay rect used by the File Browser git prompt.
    let listing = super::render::listing_rect(modal_area, has_rejection);
    let overlay = git_prompt_rect(listing, true)?;
    let inner = Rect {
        x: overlay.x.saturating_add(1),
        y: overlay.y.saturating_add(1),
        width: overlay.width.saturating_sub(2),
        height: overlay.height.saturating_sub(2),
    };
    let chunks = jackin_tui::components::dialog_inner_chunks(inner, Some(2));
    if chunks[1].height < 2 {
        return None;
    }
    Some(Rect {
        x: chunks[1].x,
        y: chunks[1].y.saturating_add(1),
        width: chunks[1].width,
        height: 1,
    })
}

/// Build the three focus-styled button spans for the git-repo prompt.
/// Focused choice highlights on white; unfocused stays flush with the
/// modal background so only the focused choice pops (canonical template).
pub(super) fn git_prompt_buttons(focus: GitPromptFocus) -> Line<'static> {
    let focused = Style::default()
        .bg(WHITE)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let unfocused = Style::default()
        .fg(PHOSPHOR_GREEN)
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

/// Build the canonical footer hints for the git-repo prompt.
///
/// When `has_url` is true:
/// `M mount · P pick · O open · C/Esc cancel`.
/// When `has_url` is false, the `· O open` segment is dropped so the
/// hint doesn't advertise a disabled action:
/// `M mount · P pick · C/Esc cancel`.
pub(super) fn git_prompt_footer_items(has_url: bool) -> Vec<jackin_tui::HintSpan<'static>> {
    use jackin_tui::HintSpan;
    let mut spans = vec![
        // UNREGISTERABLE(git-prompt-no-keymap): M handled inline; no GIT_PROMPT_KEYMAP registered.
        HintSpan::Key("M"),
        HintSpan::Text("mount"),
        HintSpan::Sep,
        // UNREGISTERABLE(git-prompt-no-keymap): P handled inline; no GIT_PROMPT_KEYMAP registered.
        HintSpan::Key("P"),
        HintSpan::Text("pick"),
    ];
    if has_url {
        // UNREGISTERABLE(git-prompt-no-keymap): O handled inline; no GIT_PROMPT_KEYMAP registered.
        spans.extend([HintSpan::Sep, HintSpan::Key("O"), HintSpan::Text("open")]);
    }
    spans.extend([
        HintSpan::GroupSep,
        // UNREGISTERABLE(multi-key-display-group): ⇥/→ combines Tab and Right arrow.
        HintSpan::Key("\u{21e5}/\u{2192}"),
        HintSpan::Text("next"),
        HintSpan::Sep,
        // UNREGISTERABLE(multi-key-display-group): ⇤/← combines BackTab and Left arrow.
        HintSpan::Key("\u{21e4}/\u{2190}"),
        HintSpan::Text("prev"),
        HintSpan::Sep,
        // UNREGISTERABLE(git-prompt-no-keymap): ↵ confirms inline.
        HintSpan::Key("\u{21b5}"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(multi-key-display-group): combined C/Esc cancel display.
        HintSpan::Key("C/Esc"),
        HintSpan::Text("cancel"),
    ]);
    spans
}

/// Overlay renderer for the in-browser "Git repository detected" prompt.
pub(super) fn render_git_prompt(frame: &mut Frame<'_>, parent: Rect, state: &FileBrowserState) {
    use ratatui::layout::Alignment;

    // Add a row when we have an origin URL to show under the title.
    let has_url = state.pending_git_url.is_some();
    let Some(area) = git_prompt_rect(parent, has_url) else {
        return;
    };

    let inner = jackin_tui::components::render_dialog_shell(
        frame,
        area,
        Some("Git repository detected"),
        jackin_tui::components::DialogBorder::Default,
    );

    let content_rows = if has_url { 2 } else { 1 };
    let chunks = jackin_tui::components::dialog_inner_chunks(inner, Some(content_rows));
    let prompt_row = Rect {
        height: 1,
        ..chunks[1]
    };

    frame.render_widget(
        Paragraph::new(Span::styled(
            "What would you like to do?",
            jackin_tui::theme::BOLD_WHITE,
        ))
        .alignment(Alignment::Center),
        prompt_row,
    );

    if has_url {
        let url = state.pending_git_url.as_deref().unwrap_or_default();
        let url_row = Rect {
            y: chunks[1].y.saturating_add(1),
            height: 1,
            ..chunks[1]
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                url.to_owned(),
                Style::default()
                    .fg(PHOSPHOR_DIM)
                    .add_modifier(Modifier::ITALIC),
            ))
            .alignment(Alignment::Center),
            url_row,
        );
    }

    frame.render_widget(
        Paragraph::new(git_prompt_buttons(state.pending_git_focus)).alignment(Alignment::Center),
        chunks[3],
    );
}

#[cfg(test)]
mod tests;

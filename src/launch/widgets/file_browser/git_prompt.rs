//! Git-repo-detected prompt: state machine + geometry + render.
//!
//! When the operator hits Enter on a row whose path contains a `.git`,
//! we pause navigation and show a small modal asking what to do
//! (mount / pick-subdir / cancel / open-in-browser). This module owns
//! the focus enum, the per-prompt key handler, and the overlay
//! rendering. `resolve_git_url` also lives here because it's only
//! consumed by the prompt flow.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::state::FileBrowserState;
use super::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};
use crate::launch::widgets::ModalOutcome;

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

/// Resolve the origin web URL for a git-repo path via `mount_info::inspect`.
/// Returns `Some` only for GitHub remotes that expose a resolvable web URL.
pub(super) fn resolve_git_url(path: &Path) -> Option<String> {
    match crate::launch::manager::mount_info::inspect(&path.display().to_string()) {
        crate::launch::manager::mount_info::MountKind::Git { web_url, .. } => web_url,
        _ => None,
    }
}

impl FileBrowserState {
    /// Clear the git-repo prompt state in one shot — both the pending
    /// path and the resolved URL.
    pub(super) fn dismiss_git_prompt(&mut self) {
        self.pending_git_prompt = None;
        self.pending_git_url = None;
    }

    /// Key handler used while the git-repo prompt is active.
    pub(super) fn handle_git_prompt_key(&mut self, key: KeyEvent) -> ModalOutcome<PathBuf> {
        let Some(path) = self.pending_git_prompt.clone() else {
            return ModalOutcome::Continue;
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
                self.set_cwd(&path);
                ModalOutcome::Continue
            }
            // `o` for "open the repo's web URL in the browser" — best-effort;
            // silent no-op when `pending_git_url` is `None` (non-GitHub origin
            // or unresolvable remote) or when the launcher fails. The overlay
            // drops the `· O open` hint segment in the None case so the
            // keystroke is only advertised when it actually does something.
            KeyCode::Char('o' | 'O') => {
                if let Some(url) = self.pending_git_url.as_deref() {
                    let _ = open::that_detached(url);
                }
                ModalOutcome::Continue
            }
            KeyCode::Char('c' | 'C') | KeyCode::Esc => {
                self.dismiss_git_prompt();
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                let focus = self.pending_git_focus;
                self.dismiss_git_prompt();
                match focus {
                    GitPromptFocus::MountHere => self.commit_or_reject(path),
                    GitPromptFocus::EnterIn => {
                        self.set_cwd(&path);
                        ModalOutcome::Continue
                    }
                    GitPromptFocus::Cancel => ModalOutcome::Continue,
                }
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l' | 'L') => {
                self.pending_git_focus = match self.pending_git_focus {
                    GitPromptFocus::MountHere => GitPromptFocus::EnterIn,
                    GitPromptFocus::EnterIn => GitPromptFocus::Cancel,
                    GitPromptFocus::Cancel => GitPromptFocus::MountHere,
                };
                ModalOutcome::Continue
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h' | 'H') => {
                self.pending_git_focus = match self.pending_git_focus {
                    GitPromptFocus::MountHere => GitPromptFocus::Cancel,
                    GitPromptFocus::EnterIn => GitPromptFocus::MountHere,
                    GitPromptFocus::Cancel => GitPromptFocus::EnterIn,
                };
                ModalOutcome::Continue
            }
            _ => ModalOutcome::Continue,
        }
    }
}

/// Rect of the git-repo prompt overlay, mirroring the geometry in
/// `render_git_prompt`. Returns `None` when the overlay would exceed the
/// listing area.
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

/// Rect of the URL row inside the git-prompt overlay, in absolute
/// screen coordinates. Returns `None` when `has_url` is false — the
/// URL row isn't rendered then and a click there shouldn't open anything.
///
/// Row order inside the overlay's inner (borders stripped) body is
/// `[prompt][url?][spacer][buttons][spacer][hint]`, all Length(1). So the
/// URL row sits at `inner.y + 1 = overlay.y + 1 (top border) + 1 = overlay.y + 2`.
pub fn git_prompt_url_row_rect(modal_area: Rect, has_rejection: bool) -> Option<Rect> {
    let listing = super::render::listing_rect(modal_area, has_rejection);
    let overlay = git_prompt_rect(listing, true)?;
    // Need at least borders + prompt + url rows — otherwise the URL row
    // got clipped by the parent's height.
    if overlay.height < 3 {
        return None;
    }
    // Inside the block: strip the borders, then take row index 1.
    let inner_x = overlay.x + 1;
    let inner_width = overlay.width.saturating_sub(2);
    let url_y = overlay.y + 2;
    Some(Rect {
        x: inner_x,
        y: url_y,
        width: inner_width,
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

/// Build the canonical hint footer line for the git-repo prompt.
///
/// When `has_url` is true:
/// `M mount · P pick · O open · C/Esc cancel`.
/// When `has_url` is false, the `· O open` segment is dropped so the
/// hint doesn't advertise a disabled action:
/// `M mount · P pick · C/Esc cancel`.
pub(super) fn git_prompt_hint(has_url: bool) -> Line<'static> {
    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let mut spans: Vec<Span<'static>> = vec![
        Span::styled("M", key_style),
        Span::styled(" mount", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("P", key_style),
        Span::styled(" pick", text_style),
    ];
    if has_url {
        spans.extend([
            Span::styled(" \u{b7} ", sep_style),
            Span::styled("O", key_style),
            Span::styled(" open", text_style),
        ]);
    }
    spans.extend([
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("C/Esc", key_style),
        Span::styled(" cancel", text_style),
    ]);
    Line::from(spans)
}

/// Overlay renderer for the in-browser "Git repository detected" prompt.
pub(super) fn render_git_prompt(frame: &mut Frame, parent: Rect, state: &FileBrowserState) {
    use ratatui::layout::{Alignment, Constraint, Direction, Layout};

    // Add a row when we have an origin URL to show under the title.
    let has_url = state.pending_git_url.is_some();
    // Overlay widens to 80 cols so the three-button row and the canonical
    // hint line both fit on one line without wrapping.
    let w = parent.width.saturating_sub(4).min(80);
    let base_h: u16 = if has_url { 8 } else { 7 };
    let h = base_h.min(parent.height);
    let x = parent.x + parent.width.saturating_sub(w) / 2;
    let y = parent.y + parent.height.saturating_sub(h) / 2;
    let area = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " Git repository detected ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    // Row constraints: [prompt][url?][spacer][buttons][spacer][hint].
    let row_count = if has_url { 6 } else { 5 };
    let constraints: Vec<Constraint> = (0..row_count).map(|_| Constraint::Length(1)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "What would you like to do?",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        rows[0],
    );

    let (buttons_idx, hint_idx) = if has_url {
        let url = state.pending_git_url.as_deref().unwrap_or_default();
        frame.render_widget(
            Paragraph::new(Span::styled(
                url.to_string(),
                Style::default()
                    .fg(PHOSPHOR_DIM)
                    .add_modifier(Modifier::ITALIC),
            ))
            .alignment(Alignment::Center),
            rows[1],
        );
        (3, 5)
    } else {
        (2, 4)
    };

    frame.render_widget(
        Paragraph::new(git_prompt_buttons(state.pending_git_focus)).alignment(Alignment::Center),
        rows[buttons_idx],
    );
    frame.render_widget(
        Paragraph::new(git_prompt_hint(has_url)).alignment(Alignment::Center),
        rows[hint_idx],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use tempfile::tempdir;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn state_rooted_at(root: PathBuf, cwd: PathBuf) -> FileBrowserState {
        FileBrowserState::new_at(root, cwd)
    }

    // ── Git-repo prompt ───────────────────────────────────────────────

    #[test]
    fn enter_on_git_repo_opens_prompt() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        // Index 0 is `..`; advance to `repo`.
        state.handle_key(key(KeyCode::Down));
        let outcome = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert!(state.pending_git_prompt.is_some());
        assert_eq!(state.pending_git_focus, GitPromptFocus::MountHere);
    }

    /// Write a minimal `.git/HEAD` + `.git/config` with the given origin.
    fn seed_git_repo_with_origin(repo: &Path, remote: &str) {
        let git = repo.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            git.join("config"),
            format!("[remote \"origin\"]\n\turl = {remote}\n"),
        )
        .unwrap();
    }

    #[test]
    fn enter_on_git_repo_with_origin_sets_url() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        seed_git_repo_with_origin(&repo, "git@github.com:jackin-project/jackin.git");

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        assert!(state.pending_git_prompt.is_some());
        let url = state
            .pending_git_url
            .as_deref()
            .expect("GitHub origin must resolve");
        assert!(url.contains("github.com/jackin-project/jackin"));
    }

    #[test]
    fn enter_on_git_repo_without_origin_leaves_url_none() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        assert!(state.pending_git_prompt.is_some());
        assert!(state.pending_git_url.is_none());
    }

    #[test]
    fn mount_here_commits_git_path() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        assert_eq!(state.pending_git_focus, GitPromptFocus::MountHere);
        let outcome = state.handle_key(key(KeyCode::Enter));
        match outcome {
            ModalOutcome::Commit(p) => {
                assert_eq!(p.canonicalize().unwrap(), repo.canonicalize().unwrap(),);
            }
            other => panic!("expected Commit, got {other:?}"),
        }
        assert!(state.pending_git_prompt.is_none());
    }

    #[test]
    fn enter_in_navigates_into_subdir() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir(repo.join("sub")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter)); // open prompt
        state.handle_key(key(KeyCode::Tab)); // MountHere -> EnterIn
        assert_eq!(state.pending_git_focus, GitPromptFocus::EnterIn);

        let outcome = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert!(state.pending_git_prompt.is_none());
        assert_eq!(
            state.cwd.canonicalize().unwrap(),
            repo.canonicalize().unwrap(),
        );
    }

    #[test]
    fn cancel_dismisses_prompt_via_focus() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent.clone());
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        state.handle_key(key(KeyCode::Tab));
        state.handle_key(key(KeyCode::Tab));
        assert_eq!(state.pending_git_focus, GitPromptFocus::Cancel);

        let outcome = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert!(state.pending_git_prompt.is_none());
        assert_eq!(
            state.cwd.canonicalize().unwrap(),
            parent.canonicalize().unwrap(),
        );
    }

    #[test]
    fn esc_dismisses_prompt_without_cancelling_browser() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        assert!(state.pending_git_prompt.is_some());
        let outcome = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert!(state.pending_git_prompt.is_none());
    }

    #[test]
    fn m_shortcut_commits_repo_from_prompt() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        state.handle_key(key(KeyCode::Tab));
        let outcome = state.handle_key(key(KeyCode::Char('m')));
        match outcome {
            ModalOutcome::Commit(p) => {
                assert_eq!(p.canonicalize().unwrap(), repo.canonicalize().unwrap(),);
            }
            other => panic!("expected Commit, got {other:?}"),
        }
    }

    // ── O hotkey (open URL in browser) ────────────────────────────────

    /// With `pending_git_url == None`, `O` must be a silent no-op: the
    /// prompt stays open, focus is unchanged, and no commit/cancel fires.
    /// We can't assert that `open::that_detached` *didn't* run (it doesn't
    /// run when the URL is None — that's the code path we're testing),
    /// but we can pin the observable state.
    #[test]
    fn o_shortcut_without_url_is_silent_noop() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        assert!(state.pending_git_prompt.is_some());
        assert!(state.pending_git_url.is_none());
        let focus_before = state.pending_git_focus;

        let outcome = state.handle_key(key(KeyCode::Char('o')));
        assert!(matches!(outcome, ModalOutcome::Continue));
        // Prompt still open, focus unchanged.
        assert!(state.pending_git_prompt.is_some());
        assert_eq!(state.pending_git_focus, focus_before);
    }

    /// With `pending_git_url == Some(url)`, `O` still returns Continue
    /// and keeps the prompt open (open-in-browser is best-effort and
    /// doesn't dismiss). Hidden file:// URL so `open::that_detached` is
    /// a silent no-op in CI even when a GUI isn't available.
    #[test]
    fn o_shortcut_with_url_returns_continue_and_keeps_prompt_open() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        // Force a URL into state for the test; the real handler would have
        // populated this via `resolve_git_url` when origin is a GitHub URL.
        state.pending_git_url = Some("file:///tmp/definitely-not-real".to_string());

        let outcome = state.handle_key(key(KeyCode::Char('O')));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert!(state.pending_git_prompt.is_some());
        // URL stays on state — O doesn't dismiss the prompt.
        assert_eq!(
            state.pending_git_url.as_deref(),
            Some("file:///tmp/definitely-not-real"),
        );
    }

    // ── Conditional hint segment ──────────────────────────────────────

    /// The `O open` hint segment is only rendered when a URL is resolved.
    /// With `has_url == false` the hint must not advertise `O open`.
    #[test]
    fn git_prompt_hint_omits_open_segment_when_url_is_none() {
        let line = git_prompt_hint(false);
        let rendered: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            !rendered.contains('O'),
            "hint should not mention O when no URL: {rendered:?}"
        );
        assert!(
            !rendered.contains("open"),
            "hint should not mention 'open' when no URL: {rendered:?}"
        );
        assert!(rendered.contains('M'));
        assert!(rendered.contains('P'));
        assert!(rendered.contains("C/Esc"));
    }

    #[test]
    fn git_prompt_hint_includes_open_segment_when_url_is_present() {
        let line = git_prompt_hint(true);
        let rendered: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            rendered.contains('O'),
            "hint should mention O when URL resolved: {rendered:?}"
        );
        assert!(
            rendered.contains("open"),
            "hint should mention 'open' when URL resolved: {rendered:?}"
        );
        // Still preserves the other segments + trailing cancel.
        assert!(rendered.contains('M'));
        assert!(rendered.contains('P'));
        assert!(rendered.contains("C/Esc"));
    }
}

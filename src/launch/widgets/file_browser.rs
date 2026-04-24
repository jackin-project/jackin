//! Host folder picker — wraps ratatui-explorer, shows folders only,
//! adds `s` as "select current folder".
//!
//! Restrictions:
//! - Starts at $HOME.
//! - Refuses navigation above $HOME (clamps cwd back after `handle()`).
//! - Excludes noisy top-level directories from the listing.
//! - Rejects $HOME itself and ~/.jackin/* as workspace sources.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use directories::BaseDirs;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, FrameExt as _},
};
use ratatui_explorer::{FileExplorer, FileExplorerBuilder, Theme};

/// Phosphor green — matches jackin's primary colour.
const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
/// Dimmed phosphor — used for non-selected text.
const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);

use super::ModalOutcome;

/// Directories excluded from the listing when browsing $HOME.
const EXCLUDED: &[&str] = &[
    "Library",
    "Applications",
    "Movies",
    "Music",
    "OrbStack",
    "Pictures",
];

/// Marker prepended to directory names that are git repos. U+2387
/// (alternative key symbol) reads as a branch and puts the marker where
/// the eye lands first. Renders as a small icon in monospace terminals
/// without needing a nerd-font.
///
/// Note: ratatui-explorer 0.3's Theme exposes a single `dir_style` shared
/// by every directory entry, so per-entry colouring (green for repos) is
/// not possible here. A future PR that drops ratatui-explorer for a
/// custom list renderer could add colour; for now the textual prefix is
/// the affordance.
const GIT_REPO_MARKER: &str = "\u{2387} ";

/// Does `path` contain a `.git` child? Dir (regular clone) OR file
/// (submodule worktree, `.git` is a file pointing at the real gitdir).
/// Single `metadata` call per directory listing — no filesystem walk.
fn has_git_dir(path: &Path) -> bool {
    let dotgit = path.join(".git");
    dotgit.is_dir() || dotgit.is_file()
}

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

pub struct FileBrowserState {
    pub explorer: FileExplorer,
    /// $HOME — the browser cannot navigate above this path.
    pub root: PathBuf,
    /// Set when the user presses `s` but the selection is rejected.
    /// Cleared on the next keypress.
    pub rejected_reason: Option<String>,
    /// Active when the operator has pressed Enter on a git-repo row.
    /// Carries the repo path so approving "mount this repo" commits to it
    /// without re-walking the listing.
    pub pending_git_prompt: Option<PathBuf>,
    /// Which choice is highlighted in the git-repo prompt. Cycled by Tab.
    /// Ignored when `pending_git_prompt` is `None`.
    pub pending_git_focus: GitPromptFocus,
}

impl std::fmt::Debug for FileBrowserState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileBrowserState")
            .field("root", &self.root)
            .field("rejected_reason", &self.rejected_reason)
            .field("pending_git_prompt", &self.pending_git_prompt)
            .field("pending_git_focus", &self.pending_git_focus)
            .finish_non_exhaustive()
    }
}

/// Shared `filter_map` body: apply sandbox/exclusion rules and append the
/// git-repo marker to directory names that contain a `.git` child. Extracted
/// into a free function so unit tests can drive it without constructing a
/// full `FileExplorer`.
fn annotate_file(mut file: ratatui_explorer::File, root: &Path) -> Option<ratatui_explorer::File> {
    // Keep only directories.
    if !file.is_dir {
        return None;
    }
    // ratatui-explorer appends a trailing `/` to directory entries at runtime
    // (so "Library" renders as "Library/" and the synthetic parent-link as
    // "../"). Strip the slash for comparisons against bare names like `..` or
    // the `EXCLUDED` list — otherwise the filter silently misses every entry.
    let bare = file.name.trim_end_matches('/');
    // Hide `..` when navigating up would leave the root subtree. `file.path`
    // is the parent directory that `..` leads to; if it is not inside root,
    // the entry would escape the sandbox — hide it.
    if bare == ".." {
        if !file.path.starts_with(root) {
            return None;
        }
        return Some(file);
    }
    // Strip excluded top-level names.
    if EXCLUDED.contains(&bare) {
        return None;
    }
    // Prepend the git-repo marker when the directory contains a `.git` child.
    // Works for both plain clones (`.git` is a dir) and submodules (`.git` is
    // a file containing `gitdir: <path>`). Prefix puts the marker where the
    // eye lands first — "⎇ scentbird-root/" vs the trailing marker that was
    // easy to miss.
    if has_git_dir(&file.path) {
        file.name.insert_str(0, GIT_REPO_MARKER);
    }
    Some(file)
}

impl FileBrowserState {
    /// Build a new browser starting at $HOME, filtered to directories only,
    /// excluding well-known noisy top-level folders.
    pub fn new_from_home() -> anyhow::Result<Self> {
        let home = BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("could not resolve $HOME"))?;

        // Build a phosphor-palette theme that matches jackin's TUI style.
        let theme = Theme::default()
            // Block with ALL borders styled phosphor dark would be overridden by
            // the block in the default theme; replace with a jackin-coloured block.
            .with_block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(PHOSPHOR_DIM)),
            )
            // Base list text: white (dir entries are the only thing shown).
            .with_style(Style::default().fg(Color::Rgb(255, 255, 255)))
            // Directory entries: white (same — all entries are dirs).
            .with_dir_style(Style::default().fg(Color::Rgb(255, 255, 255)))
            // Non-directory items: dim (shouldn't appear, but keep safe).
            .with_item_style(Style::default().fg(PHOSPHOR_DIM))
            // Highlighted directory: bright phosphor bg + black fg.
            .with_highlight_dir_style(
                Style::default()
                    .bg(PHOSPHOR_GREEN)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            // Highlighted non-dir item.
            .with_highlight_item_style(
                Style::default()
                    .bg(PHOSPHOR_GREEN)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            // Use ▸ as selection indicator.
            .with_highlight_symbol("▸ ")
            // Dynamic title: shortened CWD, styled bold-white.
            .with_title_top(|fe| {
                let cwd = crate::tui::shorten_home(&fe.cwd().display().to_string());
                Line::styled(
                    cwd,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
            });
        let root_for_filter = home.clone();
        let explorer = FileExplorerBuilder::default()
            .working_dir(&home)
            .theme(theme)
            .filter_map(move |file| annotate_file(file, &root_for_filter))
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build file explorer: {e}"))?;

        Ok(Self {
            explorer,
            root: home,
            rejected_reason: None,
            pending_git_prompt: None,
            pending_git_focus: GitPromptFocus::MountHere,
        })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<PathBuf> {
        // Git-repo prompt has its own key map; delegate before clearing any
        // state the main handler would otherwise reset.
        if self.pending_git_prompt.is_some() {
            return self.handle_git_prompt_key(key);
        }

        // Clear any stale rejection message on the next keypress.
        self.rejected_reason = None;

        match key.code {
            KeyCode::Char('s') => {
                // Prefer the currently-highlighted entry (the "would navigate
                // into" target) so the operator can pick a sibling folder
                // without pressing Enter first. Fall back to the cwd when the
                // highlight is `../` or the listing is empty.
                let cwd = self.explorer.cwd().clone();
                let target = {
                    let files = self.explorer.files();
                    if files.is_empty() {
                        // `current()` panics on an empty listing — guard it.
                        cwd
                    } else {
                        let highlighted = self.explorer.current();
                        // An entry named `"../"` is ratatui-explorer's synthetic
                        // parent-link row; treat it as "no real selection".
                        let is_parent_link = highlighted.name == "../";
                        if highlighted.is_dir && !is_parent_link {
                            highlighted.path.clone()
                        } else {
                            cwd
                        }
                    }
                };

                self.commit_or_reject(target)
            }
            KeyCode::Enter => {
                // If the highlighted entry is a git repo (and not the
                // synthetic `../` parent link), open the choice prompt
                // instead of navigating in. Guard `current()` against an
                // empty listing (filter may have removed every entry).
                if !self.explorer.files().is_empty() {
                    let highlighted = self.explorer.current();
                    let is_parent_link = highlighted.name == "../";
                    let path = highlighted.path.clone();
                    let is_dir = highlighted.is_dir;
                    if is_dir && !is_parent_link && has_git_dir(&path) {
                        self.pending_git_prompt = Some(path);
                        self.pending_git_focus = GitPromptFocus::MountHere;
                        return ModalOutcome::Continue;
                    }
                }
                // Fall through to the explorer for non-git folders.
                let event = crossterm::event::Event::Key(key);
                let _ = self.explorer.handle(&event);
                let cwd = self.explorer.cwd().clone();
                if !cwd.starts_with(&self.root) {
                    let _ = self.explorer.set_cwd(&self.root);
                }
                ModalOutcome::Continue
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => {
                // Guard: ratatui-explorer panics (div-by-zero) on nav keys
                // when the listing is empty. Skip dispatch in that case.
                if !self.explorer.files().is_empty() {
                    let event = crossterm::event::Event::Key(key);
                    let _ = self.explorer.handle(&event);
                }

                // Clamp cwd back to root if the user navigated above $HOME.
                // set_cwd() exists in ratatui-explorer 0.3.x.
                let cwd = self.explorer.cwd().clone();
                if !cwd.starts_with(&self.root) {
                    let _ = self.explorer.set_cwd(&self.root);
                }
                ModalOutcome::Continue
            }
        }
    }

    /// Shared commit-or-reject logic used by `s` and the git-repo prompt's
    /// "Mount this repository" option. Enforces the same sandbox rules.
    fn commit_or_reject(&mut self, target: PathBuf) -> ModalOutcome<PathBuf> {
        // Reject root itself — user must navigate into a subfolder.
        if target == self.root {
            self.rejected_reason =
                Some("Cannot use $HOME itself — navigate into a subfolder.".into());
            return ModalOutcome::Continue;
        }
        // Reject jackin's own data directory.
        let jackin_data = self.root.join(".jackin");
        if target.starts_with(&jackin_data) {
            self.rejected_reason =
                Some("Cannot use ~/.jackin/* — those paths are reserved.".into());
            return ModalOutcome::Continue;
        }
        ModalOutcome::Commit(target)
    }

    /// Key handler used while the git-repo prompt is active.
    /// - Tab / `BackTab` / ←→ / h/l cycle focus.
    /// - Enter commits the focused option.
    /// - M / E / C are direct shortcuts.
    /// - Esc dismisses the prompt (but does NOT cancel the browser).
    fn handle_git_prompt_key(&mut self, key: KeyEvent) -> ModalOutcome<PathBuf> {
        let Some(path) = self.pending_git_prompt.clone() else {
            return ModalOutcome::Continue;
        };
        match key.code {
            KeyCode::Char('m' | 'M') => {
                self.pending_git_prompt = None;
                self.commit_or_reject(path)
            }
            KeyCode::Char('e' | 'E') => {
                // "Pick a subdirectory" — navigate into the repo dir and
                // clear the prompt. Uses `set_cwd` to avoid re-posting the
                // Enter event (which would re-open the prompt).
                self.pending_git_prompt = None;
                let _ = self.explorer.set_cwd(&path);
                ModalOutcome::Continue
            }
            KeyCode::Char('c' | 'C') | KeyCode::Esc => {
                self.pending_git_prompt = None;
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                let focus = self.pending_git_focus;
                self.pending_git_prompt = None;
                match focus {
                    GitPromptFocus::MountHere => self.commit_or_reject(path),
                    GitPromptFocus::EnterIn => {
                        let _ = self.explorer.set_cwd(&path);
                        ModalOutcome::Continue
                    }
                    GitPromptFocus::Cancel => ModalOutcome::Continue,
                }
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                self.pending_git_focus = match self.pending_git_focus {
                    GitPromptFocus::MountHere => GitPromptFocus::EnterIn,
                    GitPromptFocus::EnterIn => GitPromptFocus::Cancel,
                    GitPromptFocus::Cancel => GitPromptFocus::MountHere,
                };
                ModalOutcome::Continue
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
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

pub fn render(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout},
        style::{Color, Modifier, Style},
        text::Span,
        widgets::Paragraph,
    };

    frame.render_widget(ratatui::widgets::Clear, area);

    // Layout:
    //   - When a rejection is active: [reason banner][explorer][nav hint]
    //   - Otherwise:                   [explorer][nav hint]
    //
    // The "press [S] to use this folder" banner was removed: the footer
    // `S select` hint already tells the operator the same thing and no
    // other modal uses a top affordance banner. The `rejected_reason`
    // banner stays — it is functional error feedback, not an affordance.
    let has_rejection = state.rejected_reason.is_some();
    let constraints: Vec<Constraint> = if has_rejection {
        vec![
            Constraint::Length(1), // rejection banner
            Constraint::Min(3),    // explorer
            Constraint::Length(1), // nav hint
        ]
    } else {
        vec![
            Constraint::Min(3),    // explorer
            Constraint::Length(1), // nav hint
        ]
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let explorer_idx = if has_rejection {
        let reason = state.rejected_reason.as_ref().unwrap();
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("\u{2717} {reason}"),
                Style::default()
                    .fg(Color::Rgb(255, 94, 122))
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
            chunks[0],
        );
        1
    } else {
        0
    };

    frame.render_widget_ref(state.explorer.widget(), chunks[explorer_idx]);

    render_footer_legend(frame, chunks[chunks.len() - 1], state);

    // Git-repo prompt overlay — centred inside the explorer area so the
    // listing stays visible as context behind the modal.
    if state.pending_git_prompt.is_some() {
        render_git_prompt(frame, chunks[explorer_idx], state);
    }
}

/// Render the bottom footer legend. Swaps the usual nav+`s` legend for a
/// prompt-focused legend when the git-repo confirm overlay is active.
fn render_footer_legend(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    use ratatui::{
        layout::Alignment,
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::Paragraph,
    };
    let key = Style::default()
        .fg(Color::Rgb(255, 255, 255))
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(Color::Rgb(0, 255, 65));
    let sep = Style::default().fg(Color::Rgb(0, 80, 18));
    let line = if state.pending_git_prompt.is_some() {
        Line::from(vec![
            Span::styled("Tab", key),
            Span::styled(" cycle", text),
            Span::styled(" \u{b7} ", sep),
            Span::styled("Enter", key),
            Span::styled(" confirm", text),
            Span::styled(" \u{b7} ", sep),
            Span::styled("Esc", key),
            Span::styled(" cancel", text),
        ])
    } else {
        Line::from(vec![
            Span::styled("\u{2191}\u{2193}", key),
            Span::styled(" navigate", text),
            Span::styled(" \u{b7} ", sep),
            Span::styled("Enter", key),
            Span::styled(" open", text),
            Span::styled(" \u{b7} ", sep),
            Span::styled("h/\u{2190}", key),
            Span::styled(" up", text),
            Span::raw("   "),
            Span::styled("s", key),
            Span::styled(" select", text),
            Span::raw("   "),
            Span::styled("Esc", key),
            Span::styled(" cancel", text),
        ])
    };
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

/// Overlay renderer for the in-browser "Git repository detected" prompt.
/// Mirrors the phosphor palette + focus styling used by `confirm.rs`
/// (bright white bg on focused button, phosphor green on unfocused) so
/// it feels native next to the other modals.
fn render_git_prompt(frame: &mut ratatui::Frame, parent: Rect, state: &FileBrowserState) {
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Borders, Paragraph},
    };

    // Centre a fixed-size overlay inside the explorer area.
    let w = parent.width.saturating_sub(4).min(60);
    let h = 7u16.min(parent.height);
    let x = parent.x + parent.width.saturating_sub(w) / 2;
    let y = parent.y + parent.height.saturating_sub(h) / 2;
    let area = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    let phosphor = Color::Rgb(0, 255, 65);
    let white = Color::Rgb(255, 255, 255);
    let phosphor_dark = Color::Rgb(0, 80, 18);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(phosphor))
        .title(Span::styled(
            " Git repository detected ",
            Style::default().fg(white).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // prompt
            Constraint::Length(1), // spacer
            Constraint::Length(1), // buttons
            Constraint::Length(1), // spacer
            Constraint::Length(1), // hint
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Span::styled(
            "What would you like to do?",
            Style::default().fg(white).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        rows[0],
    );

    let focused = Style::default()
        .bg(white)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let unfocused = Style::default()
        .bg(phosphor)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let btn = |focus: GitPromptFocus, label: &'static str| -> Span<'static> {
        let style = if state.pending_git_focus == focus {
            focused
        } else {
            unfocused
        };
        Span::styled(format!(" {label} "), style)
    };
    let buttons = Line::from(vec![
        btn(GitPromptFocus::MountHere, "Mount this repository"),
        Span::raw("  "),
        btn(GitPromptFocus::EnterIn, "Pick a subdirectory"),
        Span::raw("  "),
        btn(GitPromptFocus::Cancel, "Cancel"),
    ]);
    frame.render_widget(
        Paragraph::new(buttons).alignment(Alignment::Center),
        rows[2],
    );

    let key_style = Style::default().fg(white).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(phosphor);
    let sep_style = Style::default().fg(phosphor_dark);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("M", key_style),
            Span::styled(" mount", text_style),
            Span::styled(" \u{b7} ", sep_style),
            Span::styled("E", key_style),
            Span::styled(" enter", text_style),
            Span::styled(" \u{b7} ", sep_style),
            Span::styled("C/Esc", key_style),
            Span::styled(" cancel", text_style),
        ]))
        .alignment(Alignment::Center),
        rows[4],
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

    fn make_state_at(path: std::path::PathBuf) -> FileBrowserState {
        let theme = Theme::default().add_default_title();
        let root_for_filter = path.clone();
        let explorer = FileExplorerBuilder::default()
            .working_dir(&path)
            .theme(theme)
            .filter_map(move |file| annotate_file(file, &root_for_filter))
            .build()
            .unwrap();
        FileBrowserState {
            explorer,
            root: path,
            rejected_reason: None,
            pending_git_prompt: None,
            pending_git_focus: GitPromptFocus::MountHere,
        }
    }

    #[test]
    fn filter_excludes_files() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("folder")).unwrap();
        std::fs::write(tmp.path().join("file.txt"), b"x").unwrap();

        let state = make_state_at(tmp.path().to_path_buf());
        let files: Vec<_> = state
            .explorer
            .files()
            .iter()
            .map(|f| f.name.clone())
            .collect();
        assert!(
            files.iter().any(|n| n == "folder/"),
            "folder missing: {files:?}"
        );
        assert!(
            !files.iter().any(|n| n == "file.txt"),
            "file should be filtered out: {files:?}"
        );
    }

    #[test]
    fn s_commits_highlighted_entry() {
        // Inside a folder with a nested directory, `s` should commit the
        // highlighted child entry — not the parent cwd.
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).unwrap();

        let theme = Theme::default().add_default_title();
        let explorer = FileExplorerBuilder::default()
            .working_dir(&parent)
            .theme(theme)
            .filter_map(|file| if file.is_dir { Some(file) } else { None })
            .build()
            .unwrap();
        let mut state = FileBrowserState {
            explorer,
            // root = tmp so that `parent` is a subfolder of the sandbox and
            // neither parent nor child are rejected by the $HOME guard.
            root: tmp.path().to_path_buf(),
            rejected_reason: None,
            pending_git_prompt: None,
            pending_git_focus: GitPromptFocus::MountHere,
        };

        // Ratatui-explorer puts the synthetic `../` entry at index 0, so
        // advance the selection once to land on `child/`.
        state.handle_key(key(KeyCode::Down));

        let outcome = state.handle_key(key(KeyCode::Char('s')));
        if let ModalOutcome::Commit(path) = outcome {
            assert_eq!(
                path.canonicalize().unwrap(),
                child.canonicalize().unwrap(),
                "s should commit the highlighted child, not the parent cwd"
            );
        } else {
            panic!("expected Commit, got {:?}", outcome);
        }
    }

    #[test]
    fn s_falls_back_to_cwd_when_directory_is_empty() {
        // Inside an empty folder there is no highlighted entry to commit;
        // `s` should fall back to committing the cwd itself.
        let tmp = tempdir().unwrap();
        let empty = tmp.path().join("empty");
        std::fs::create_dir(&empty).unwrap();

        let theme = Theme::default().add_default_title();
        let explorer = FileExplorerBuilder::default()
            .working_dir(&empty)
            .theme(theme)
            .filter_map(|file| if file.is_dir { Some(file) } else { None })
            .build()
            .unwrap();
        let mut state = FileBrowserState {
            explorer,
            // root = tmp so that `empty` is not $HOME itself and s is not rejected.
            root: tmp.path().to_path_buf(),
            rejected_reason: None,
            pending_git_prompt: None,
            pending_git_focus: GitPromptFocus::MountHere,
        };

        let outcome = state.handle_key(key(KeyCode::Char('s')));
        if let ModalOutcome::Commit(path) = outcome {
            assert_eq!(
                path.canonicalize().unwrap(),
                empty.canonicalize().unwrap(),
                "s should commit cwd when no child is highlighted"
            );
        } else {
            panic!("expected Commit, got {:?}", outcome);
        }
    }

    #[test]
    fn s_rejects_root_itself() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        // cwd == root, so pressing s should reject.
        let outcome = state.handle_key(key(KeyCode::Char('s')));
        assert!(
            matches!(outcome, ModalOutcome::Continue),
            "expected Continue (rejection), got {:?}",
            outcome
        );
        assert!(
            state.rejected_reason.is_some(),
            "rejected_reason should be set"
        );
    }

    #[test]
    fn s_rejects_jackin_data_dir() {
        let tmp = tempdir().unwrap();
        let jackin = tmp.path().join(".jackin").join("workspaces");
        std::fs::create_dir_all(&jackin).unwrap();

        let theme = Theme::default().add_default_title();
        let explorer = FileExplorerBuilder::default()
            .working_dir(&jackin)
            .theme(theme)
            .filter_map(|file| if file.is_dir { Some(file) } else { None })
            .build()
            .unwrap();
        let mut state = FileBrowserState {
            explorer,
            root: tmp.path().to_path_buf(),
            rejected_reason: None,
            pending_git_prompt: None,
            pending_git_focus: GitPromptFocus::MountHere,
        };

        let outcome = state.handle_key(key(KeyCode::Char('s')));
        assert!(
            matches!(outcome, ModalOutcome::Continue),
            "expected Continue (rejection), got {:?}",
            outcome
        );
        assert!(
            state.rejected_reason.is_some(),
            "rejected_reason should be set for .jackin path"
        );
    }

    #[test]
    fn rejection_cleared_on_next_keypress() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        // First press: rejected.
        state.handle_key(key(KeyCode::Char('s')));
        assert!(state.rejected_reason.is_some());
        // Any subsequent key clears it.
        state.handle_key(key(KeyCode::Char('j'))); // navigate down
        assert!(
            state.rejected_reason.is_none(),
            "rejection should be cleared after next key"
        );
    }

    #[test]
    fn esc_cancels() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        assert!(matches!(
            state.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    // ── Item 7: git-repo marker + Enter-on-git-repo prompt ────────────

    /// Helper: build a File that looks like a ratatui-explorer-provided
    /// directory entry so we can exercise `annotate_file` directly.
    fn dir_file(name: &str, path: std::path::PathBuf) -> ratatui_explorer::File {
        ratatui_explorer::File {
            name: format!("{name}/"),
            path,
            is_dir: true,
            is_hidden: false,
            file_type: None,
        }
    }

    #[test]
    fn git_repo_in_filter_map_gets_marker() {
        // Create a directory with a `.git` subdir — the marker should be
        // appended to the entry's display name by `annotate_file`.
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let file = dir_file("repo", repo.clone());
        let out = annotate_file(file, tmp.path()).expect("directory should pass filter");
        assert!(
            out.name.starts_with(GIT_REPO_MARKER),
            "git-repo directory must have the marker prepended; got {:?}",
            out.name
        );
    }

    #[test]
    fn git_repo_via_submodule_gitfile_also_gets_marker() {
        // Submodules have `.git` as a FILE pointing at the real gitdir.
        // `has_git_dir` must treat those as repos too.
        let tmp = tempdir().unwrap();
        let sub = tmp.path().join("submodule");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join(".git"), "gitdir: ../.git/modules/submodule\n").unwrap();

        let file = dir_file("submodule", sub.clone());
        let out = annotate_file(file, tmp.path()).expect("directory should pass filter");
        assert!(
            out.name.starts_with(GIT_REPO_MARKER),
            "submodule directory must have the marker prepended; got {:?}",
            out.name
        );
    }

    #[test]
    fn excluded_names_with_trailing_slash_are_filtered() {
        // ratatui-explorer appends a trailing `/` to directory names at
        // runtime. Our filter must strip that before matching against the
        // bare EXCLUDED entries — otherwise `Library/` etc. slip through.
        let tmp = tempdir().unwrap();
        for name in EXCLUDED {
            let p = tmp.path().join(name);
            std::fs::create_dir(&p).unwrap();
            let file = dir_file(name, p);
            assert!(
                annotate_file(file, tmp.path()).is_none(),
                "excluded directory `{name}` must be filtered even with trailing slash"
            );
        }
    }

    #[test]
    fn parent_link_with_trailing_slash_passes_when_inside_root() {
        // The synthetic parent-link from ratatui-explorer has name "../" —
        // ensure the trailing slash doesn't defeat the `..` detection.
        let tmp = tempdir().unwrap();
        let child = tmp.path().join("child");
        std::fs::create_dir(&child).unwrap();
        // `file.path` is the parent that `..` leads to — use tmp (root) itself.
        let parent_link = ratatui_explorer::File {
            name: "../".to_string(),
            path: tmp.path().to_path_buf(),
            is_dir: true,
            is_hidden: false,
            file_type: None,
        };
        assert!(annotate_file(parent_link, tmp.path()).is_some());
    }

    #[test]
    fn parent_link_with_trailing_slash_hidden_when_would_escape_root() {
        // If `..` leads out of `root`, the filter must drop it — otherwise
        // the user can navigate above $HOME.
        let tmp = tempdir().unwrap();
        let root = tmp.path().join("sandbox");
        std::fs::create_dir(&root).unwrap();
        // `..` from `root` leads back to `tmp`, which is NOT inside `root`.
        let parent_link = ratatui_explorer::File {
            name: "../".to_string(),
            path: tmp.path().to_path_buf(),
            is_dir: true,
            is_hidden: false,
            file_type: None,
        };
        assert!(annotate_file(parent_link, &root).is_none());
    }

    #[test]
    fn non_git_folder_in_filter_map_unchanged() {
        // A plain folder with no `.git` child must NOT receive the marker.
        let tmp = tempdir().unwrap();
        let plain = tmp.path().join("plain");
        std::fs::create_dir(&plain).unwrap();

        let file = dir_file("plain", plain.clone());
        let out = annotate_file(file, tmp.path()).expect("directory should pass filter");
        assert!(
            !out.name.contains(GIT_REPO_MARKER),
            "non-git directory must not have the marker; got {:?}",
            out.name
        );
        // Name should be the bare `plain/` (what ratatui-explorer produced).
        assert_eq!(out.name, "plain/");
    }

    /// Construct a state rooted at `root` whose explorer cwd is `cwd`.
    /// The explorer uses the real `annotate_file` so git markers show up.
    fn state_rooted_at(root: std::path::PathBuf, cwd: std::path::PathBuf) -> FileBrowserState {
        let theme = Theme::default().add_default_title();
        let root_for_filter = root.clone();
        let explorer = FileExplorerBuilder::default()
            .working_dir(&cwd)
            .theme(theme)
            .filter_map(move |file| annotate_file(file, &root_for_filter))
            .build()
            .unwrap();
        FileBrowserState {
            explorer,
            root,
            rejected_reason: None,
            pending_git_prompt: None,
            pending_git_focus: GitPromptFocus::MountHere,
        }
    }

    #[test]
    fn enter_on_git_repo_opens_prompt() {
        // Parent contains a git-repo subfolder. Navigating past `../` to
        // land on the repo and pressing Enter should open the prompt
        // (not navigate into it).
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        // Skip past `../` onto `repo`.
        state.handle_key(key(KeyCode::Down));
        let outcome = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert!(
            state.pending_git_prompt.is_some(),
            "Enter on a git-repo row must open the prompt"
        );
        assert_eq!(state.pending_git_focus, GitPromptFocus::MountHere);
    }

    #[test]
    fn mount_here_commits_git_path() {
        // Prompt open on a repo path → focus MountHere → Enter commits
        // that path via the same sandbox rules as `s`.
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter)); // open prompt
        assert_eq!(state.pending_git_focus, GitPromptFocus::MountHere);
        let outcome = state.handle_key(key(KeyCode::Enter));
        match outcome {
            ModalOutcome::Commit(p) => {
                assert_eq!(
                    p.canonicalize().unwrap(),
                    repo.canonicalize().unwrap(),
                    "MountHere must commit the highlighted repo path"
                );
            }
            other => panic!("expected Commit, got {other:?}"),
        }
        assert!(state.pending_git_prompt.is_none(), "prompt should clear");
    }

    #[test]
    fn enter_in_navigates_into_subdir() {
        // Prompt open → focus EnterIn → Enter navigates into the repo
        // and clears the prompt. No Commit is emitted.
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir(repo.join("sub")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent.clone());
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter)); // open prompt
        // Cycle focus once: MountHere -> EnterIn.
        state.handle_key(key(KeyCode::Tab));
        assert_eq!(state.pending_git_focus, GitPromptFocus::EnterIn);

        let outcome = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert!(state.pending_git_prompt.is_none());
        assert_eq!(
            state.explorer.cwd().canonicalize().unwrap(),
            repo.canonicalize().unwrap(),
            "EnterIn must navigate into the repo directory"
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
        // Tab twice → Cancel focus.
        state.handle_key(key(KeyCode::Tab));
        state.handle_key(key(KeyCode::Tab));
        assert_eq!(state.pending_git_focus, GitPromptFocus::Cancel);

        let outcome = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert!(state.pending_git_prompt.is_none());
        // cwd unchanged.
        assert_eq!(
            state.explorer.cwd().canonicalize().unwrap(),
            parent.canonicalize().unwrap(),
        );
    }

    #[test]
    fn esc_dismisses_prompt_without_cancelling_browser() {
        // Esc while the prompt is active clears the prompt but keeps the
        // browser open (no Cancel outcome).
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        assert!(state.pending_git_prompt.is_some());
        let outcome = state.handle_key(key(KeyCode::Esc));
        assert!(
            matches!(outcome, ModalOutcome::Continue),
            "Esc in the prompt must not cancel the browser; got {outcome:?}"
        );
        assert!(state.pending_git_prompt.is_none());
    }

    #[test]
    fn enter_on_plain_folder_still_navigates() {
        // Regression guard: non-git directories keep their usual Enter =
        // navigate-in behavior.
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let plain = parent.join("plain");
        std::fs::create_dir_all(&plain).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        assert!(
            state.pending_git_prompt.is_none(),
            "plain folder must not open the prompt"
        );
        assert_eq!(
            state.explorer.cwd().canonicalize().unwrap(),
            plain.canonicalize().unwrap(),
            "Enter on a plain folder must navigate into it"
        );
    }

    #[test]
    fn m_shortcut_commits_repo_from_prompt() {
        // The `M` shortcut should short-circuit to MountHere regardless
        // of the current focus.
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let repo = parent.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter)); // open
        // Cycle focus away from MountHere so we know the shortcut works
        // regardless of what's focused.
        state.handle_key(key(KeyCode::Tab));
        let outcome = state.handle_key(key(KeyCode::Char('m')));
        match outcome {
            ModalOutcome::Commit(p) => {
                assert_eq!(p.canonicalize().unwrap(), repo.canonicalize().unwrap(),);
            }
            other => panic!("expected Commit from M shortcut, got {other:?}"),
        }
    }
}

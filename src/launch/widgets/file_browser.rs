//! Host folder picker — wraps ratatui-explorer, shows folders only,
//! adds `s` as "select current folder".
//!
//! Restrictions:
//! - Starts at $HOME.
//! - Refuses navigation above $HOME (clamps cwd back after `handle()`).
//! - Excludes noisy top-level directories from the listing.
//! - Rejects $HOME itself and ~/.jackin/* as workspace sources.

use std::path::PathBuf;

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

pub struct FileBrowserState {
    pub explorer: FileExplorer,
    /// $HOME — the browser cannot navigate above this path.
    pub root: PathBuf,
    /// Set when the user presses `s` but the selection is rejected.
    /// Cleared on the next keypress.
    pub rejected_reason: Option<String>,
}

impl std::fmt::Debug for FileBrowserState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileBrowserState")
            .field("root", &self.root)
            .field("rejected_reason", &self.rejected_reason)
            .finish_non_exhaustive()
    }
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
            .filter_map(move |file| {
                // Keep only directories.
                if !file.is_dir {
                    return None;
                }
                // Hide `..` when navigating up would leave the $HOME subtree.
                // `file.path` is the parent directory that `..` leads to; if it
                // is not inside root, the entry would escape the sandbox — hide it.
                if file.name == ".." {
                    if !file.path.starts_with(&root_for_filter) {
                        return None;
                    }
                    return Some(file);
                }
                // Strip excluded top-level names.
                if EXCLUDED.iter().any(|x| *x == file.name) {
                    return None;
                }
                Some(file)
            })
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build file explorer: {e}"))?;

        Ok(Self {
            explorer,
            root: home,
            rejected_reason: None,
        })
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<PathBuf> {
        // Clear any stale rejection message on the next keypress.
        self.rejected_reason = None;

        match key.code {
            KeyCode::Char('s') => {
                // Prefer the currently-highlighted entry (the "would navigate
                // into" target) so the operator can pick a sibling folder
                // without pressing Enter first. Fall back to the cwd when the
                // highlight is `../` or the listing is empty (in that case
                // `current()` is the parent entry or the files list is empty).
                let cwd = self.explorer.cwd().clone();
                let target = {
                    let files = self.explorer.files();
                    let highlighted = self.explorer.current();
                    // An entry named `"../"` is ratatui-explorer's synthetic
                    // parent-link row; treat it as "no real selection".
                    let is_parent_link = highlighted.name == "../";
                    if !files.is_empty() && highlighted.is_dir && !is_parent_link {
                        highlighted.path.clone()
                    } else {
                        cwd
                    }
                };

                // Reject $HOME itself — user must navigate into a subfolder.
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
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => {
                let event = crossterm::event::Event::Key(key);
                let _ = self.explorer.handle(&event);

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
}

pub fn render(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout},
        style::{Color, Modifier, Style},
        text::Span,
        widgets::Paragraph,
    };

    frame.render_widget(ratatui::widgets::Clear, area);

    let has_rejection = state.rejected_reason.is_some();
    let constraints: Vec<Constraint> = if has_rejection {
        vec![
            Constraint::Length(1), // affordance
            Constraint::Length(1), // rejection banner
            Constraint::Min(3),    // explorer
            Constraint::Length(1), // nav hint
        ]
    } else {
        vec![
            Constraint::Length(1), // affordance
            Constraint::Min(3),    // explorer
            Constraint::Length(1), // nav hint
        ]
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // Affordance — match the footer key/label scheme:
    //   Key ("[S]") = WHITE + BOLD; surrounding prose = PHOSPHOR_GREEN.
    frame.render_widget(
        Paragraph::new(ratatui::text::Line::from(vec![
            Span::styled("press ", Style::default().fg(Color::Rgb(0, 255, 65))),
            Span::styled(
                "[S]",
                Style::default()
                    .fg(Color::Rgb(255, 255, 255))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " to use this folder",
                Style::default().fg(Color::Rgb(0, 255, 65)),
            ),
        ]))
        .alignment(Alignment::Center),
        chunks[0],
    );

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
            chunks[1],
        );
        2
    } else {
        1
    };

    frame.render_widget_ref(state.explorer.widget(), chunks[explorer_idx]);

    // Footer legend — same scheme as the main TUI footer.
    let key = Style::default()
        .fg(Color::Rgb(255, 255, 255))
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(Color::Rgb(0, 255, 65));
    let sep = Style::default().fg(Color::Rgb(0, 80, 18));
    frame.render_widget(
        Paragraph::new(ratatui::text::Line::from(vec![
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
        ]))
        .alignment(Alignment::Center),
        chunks[chunks.len() - 1],
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
        let explorer = FileExplorerBuilder::default()
            .working_dir(&path)
            .theme(theme)
            .filter_map(|file| if file.is_dir { Some(file) } else { None })
            .build()
            .unwrap();
        FileBrowserState {
            explorer,
            root: path,
            rejected_reason: None,
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
}

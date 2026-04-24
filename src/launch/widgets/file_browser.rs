//! Host folder picker — custom directory browser scoped to $HOME.
//!
//! Behavior:
//! - Starts at $HOME.
//! - Refuses navigation above $HOME (clamps cwd back to root).
//! - Excludes noisy top-level directories at the $HOME level.
//! - Rejects $HOME itself and ~/.jackin/* as workspace sources.
//! - Tags git-repo rows with a trailing ` (git)` suffix in a distinct
//!   colour so the operator can scan for repos at a glance. Enter on a
//!   repo row opens a prompt (mount / pick-subdir / cancel) before
//!   committing or navigating in.
//!
//! The browser was originally built on `ratatui-explorer`, but that
//! crate's `Theme` exposes a single `dir_style` shared by every row —
//! meaning "colour git repos differently" is impossible. Rewriting in-
//! house costs ~400 lines and unlocks per-entry styling plus a simpler
//! keymap (`h/l` / arrows / `s` / `Esc` handled directly instead of
//! round-tripping through the explorer's event handler).

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use directories::BaseDirs;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use tui_widget_list::ListState;

/// Phosphor green — matches jackin's primary colour.
const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
/// Dimmed phosphor — used for the ` (git)` suffix and italic metadata.
const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
/// Bright white — used for cwd titles + focus highlights.
const WHITE: Color = Color::Rgb(255, 255, 255);
/// Sandbox-rejection / error red.
const DANGER_RED: Color = Color::Rgb(255, 94, 122);
/// Dark phosphor — block borders, separator glyphs.
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);

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

/// Does `path` contain a `.git` child? Dir (regular clone) OR file
/// (submodule worktree, `.git` is a file pointing at the real gitdir).
/// Single `metadata` call per directory entry — no filesystem walk.
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

/// One row in the folder listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    /// Display name, no trailing slash. `".."` for the synthetic parent link.
    pub name: String,
    /// Absolute path the row resolves to. For `..` this is the parent dir.
    pub path: PathBuf,
    /// True for the synthetic `..` parent-link row.
    pub is_parent: bool,
    /// True iff `path` contains a `.git` child (dir or submodule file).
    pub is_git: bool,
}

#[derive(Debug)]
pub struct FileBrowserState {
    /// $HOME — the browser cannot navigate above this path.
    pub root: PathBuf,
    /// Currently-displayed directory.
    pub cwd: PathBuf,
    /// Entries loaded from `cwd`, after filtering + sorting.
    pub entries: Vec<FolderEntry>,
    /// tui-widget-list selection state. Drives which row is highlighted.
    pub list_state: ListState,
    /// Set when the operator presses `s` but the selection is rejected
    /// (e.g. `$HOME` itself, `~/.jackin/...`). Cleared on the next key.
    pub rejected_reason: Option<String>,
    /// Active when the operator has pressed Enter on a git-repo row.
    pub pending_git_prompt: Option<PathBuf>,
    /// Origin URL (web form) for the repo referenced by
    /// `pending_git_prompt`. `None` for non-GitHub remotes or any repo
    /// whose origin can't be resolved — the overlay then omits the row.
    pub pending_git_url: Option<String>,
    /// Which button is highlighted in the git-repo prompt.
    pub pending_git_focus: GitPromptFocus,
}

/// Resolve the origin web URL for a git-repo path via `mount_info::inspect`.
/// Returns `Some` only for GitHub remotes that expose a resolvable web URL.
fn resolve_git_url(path: &Path) -> Option<String> {
    match crate::launch::manager::mount_info::inspect(&path.display().to_string()) {
        crate::launch::manager::mount_info::MountKind::Git { web_url, .. } => web_url,
        _ => None,
    }
}

/// Is `name` one of the top-level noise directories we hide at `$HOME`?
fn is_excluded(name: &str) -> bool {
    EXCLUDED.contains(&name)
}

/// Read directories under `cwd` and build the entry list. Hidden files
/// (leading `.`) are excluded; the `..` synthetic parent-link is prepended
/// iff `cwd != root`; at the root level the `EXCLUDED` list filters out
/// the macOS-style noise folders. Unreadable directories yield an empty
/// list — the caller surfaces that as an empty listing, not an error.
fn load_entries(cwd: &Path, root: &Path) -> Vec<FolderEntry> {
    let mut out: Vec<FolderEntry> = Vec::new();

    // Synthetic parent link — only shown when there's somewhere to go.
    if cwd != root
        && let Some(parent) = cwd.parent()
        && parent.starts_with(root)
    {
        out.push(FolderEntry {
            name: "..".to_string(),
            path: parent.to_path_buf(),
            is_parent: true,
            is_git: false,
        });
    }

    let Ok(read) = std::fs::read_dir(cwd) else {
        return out;
    };

    let at_root = cwd == root;
    let mut dirs: Vec<FolderEntry> = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden files (leading dot).
        if name.starts_with('.') {
            continue;
        }
        // Skip the $HOME-level noise folders.
        if at_root && is_excluded(&name) {
            continue;
        }
        // Only keep directories.
        let Ok(ty) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        // Follow symlinks to detect dir-symlinks; stat failures skip the entry.
        let is_dir = ty.is_dir() || (ty.is_symlink() && path.is_dir());
        if !is_dir {
            continue;
        }
        let is_git = has_git_dir(&path);
        dirs.push(FolderEntry {
            name,
            path,
            is_parent: false,
            is_git,
        });
    }
    // Alphabetical, case-insensitive. `..` (if present) was pushed first
    // and stays at index 0.
    dirs.sort_by_key(|e| e.name.to_lowercase());
    out.extend(dirs);
    out
}

impl FileBrowserState {
    /// Build a new browser starting at $HOME, filtered to directories only,
    /// excluding well-known noisy top-level folders.
    pub fn new_from_home() -> anyhow::Result<Self> {
        let home = BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("could not resolve $HOME"))?;
        Ok(Self::new_at(home.clone(), home))
    }

    /// Build a browser with `root` as the sandbox boundary and `cwd` as
    /// the initial directory. Primarily for tests — production callers
    /// should use `new_from_home`.
    pub fn new_at(root: PathBuf, cwd: PathBuf) -> Self {
        let entries = load_entries(&cwd, &root);
        let mut list_state = ListState::default();
        if !entries.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            root,
            cwd,
            entries,
            list_state,
            rejected_reason: None,
            pending_git_prompt: None,
            pending_git_url: None,
            pending_git_focus: GitPromptFocus::MountHere,
        }
    }

    /// Current working directory. Exposed so the create-workspace wizard
    /// can breadcrumb this across a step-back.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Re-point the browser at `cwd`, clamped to the sandbox root.
    /// Silently falls back to the root when `cwd` is outside the sandbox
    /// or not a readable directory.
    pub fn set_cwd(&mut self, cwd: &Path) {
        let target = if cwd.starts_with(&self.root) && cwd.is_dir() {
            cwd.to_path_buf()
        } else {
            self.root.clone()
        };
        self.cwd = target;
        self.reload();
    }

    /// Re-read entries from disk and reset the selection to index 0.
    pub fn reload(&mut self) {
        self.entries = load_entries(&self.cwd, &self.root);
        let sel = if self.entries.is_empty() {
            None
        } else {
            Some(0)
        };
        self.list_state.select(sel);
    }

    /// Move selection down one, wrapping at the end.
    fn select_next(&mut self) {
        let n = self.entries.len();
        if n == 0 {
            return;
        }
        let next = self
            .list_state
            .selected
            .map_or(0, |i| if i + 1 >= n { 0 } else { i + 1 });
        self.list_state.select(Some(next));
    }

    /// Move selection up one, wrapping at the start.
    fn select_prev(&mut self) {
        let n = self.entries.len();
        if n == 0 {
            return;
        }
        let prev = self
            .list_state
            .selected
            .map_or(0, |i| if i == 0 { n - 1 } else { i - 1 });
        self.list_state.select(Some(prev));
    }

    /// The currently-highlighted entry, if any.
    fn highlighted(&self) -> Option<&FolderEntry> {
        self.list_state.selected.and_then(|i| self.entries.get(i))
    }

    /// Navigate up one level (`cwd` → `cwd.parent()`), clamped to `root`.
    fn navigate_up(&mut self) {
        if self.cwd == self.root {
            return;
        }
        let Some(parent) = self.cwd.parent() else {
            return;
        };
        if !parent.starts_with(&self.root) {
            return;
        }
        self.cwd = parent.to_path_buf();
        self.reload();
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<PathBuf> {
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
                ModalOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                self.select_next();
                ModalOutcome::Continue
            }
            KeyCode::Left | KeyCode::Char('h' | 'H') => {
                self.navigate_up();
                ModalOutcome::Continue
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l' | 'L') => self.handle_enter(),
            KeyCode::Char('s' | 'S') => {
                // Prefer the highlighted entry (so the operator can pick a
                // sibling without first Entering). Fall back to cwd when
                // there's no real selection (empty listing, `..` row).
                let target = self
                    .highlighted()
                    .filter(|e| !e.is_parent)
                    .map_or_else(|| self.cwd.clone(), |e| e.path.clone());
                self.commit_or_reject(target)
            }
            KeyCode::Esc => {
                // Esc steps back one directory when the operator has
                // drilled below root — mirroring `h` / `←`. Only cancels
                // the modal when already at root. `rejected_reason` was
                // cleared above; preserve that in both branches.
                if self.cwd == self.root {
                    ModalOutcome::Cancel
                } else {
                    self.navigate_up();
                    ModalOutcome::Continue
                }
            }
            _ => ModalOutcome::Continue,
        }
    }

    /// Shared path for Enter / l / → on a highlighted entry: navigate
    /// into dirs, open the git-repo prompt on repo rows, up on the `..` row.
    fn handle_enter(&mut self) -> ModalOutcome<PathBuf> {
        let Some(entry) = self.highlighted().cloned() else {
            return ModalOutcome::Continue;
        };
        if entry.is_parent {
            self.navigate_up();
            return ModalOutcome::Continue;
        }
        if entry.is_git {
            self.pending_git_url = resolve_git_url(&entry.path);
            self.pending_git_prompt = Some(entry.path);
            self.pending_git_focus = GitPromptFocus::MountHere;
            return ModalOutcome::Continue;
        }
        // Plain folder — navigate in.
        if entry.path.starts_with(&self.root) && entry.path.is_dir() {
            self.cwd = entry.path;
            self.reload();
        }
        ModalOutcome::Continue
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

    /// Clear the git-repo prompt state in one shot — both the pending
    /// path and the resolved URL.
    fn dismiss_git_prompt(&mut self) {
        self.pending_git_prompt = None;
        self.pending_git_url = None;
    }

    /// Key handler used while the git-repo prompt is active.
    fn handle_git_prompt_key(&mut self, key: KeyEvent) -> ModalOutcome<PathBuf> {
        let Some(path) = self.pending_git_prompt.clone() else {
            return ModalOutcome::Continue;
        };
        match key.code {
            KeyCode::Char('m' | 'M') => {
                self.dismiss_git_prompt();
                self.commit_or_reject(path)
            }
            KeyCode::Char('e' | 'E') => {
                self.dismiss_git_prompt();
                self.set_cwd(&path);
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

pub fn render(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    use ratatui::layout::{Alignment, Constraint, Direction, Layout};

    frame.render_widget(ratatui::widgets::Clear, area);

    // Layout: [optional rejection banner][listing][nav hint].
    let has_rejection = state.rejected_reason.is_some();
    let constraints: Vec<Constraint> = if has_rejection {
        vec![
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ]
    } else {
        vec![Constraint::Min(3), Constraint::Length(1)]
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let listing_idx = if has_rejection {
        let reason = state.rejected_reason.as_ref().unwrap();
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("\u{2717} {reason}"),
                Style::default().fg(DANGER_RED).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
            chunks[0],
        );
        1
    } else {
        0
    };

    render_listing(frame, chunks[listing_idx], state);
    render_footer_legend(frame, chunks[chunks.len() - 1], state);

    // Git-repo prompt overlay — centred inside the listing area so the
    // listing stays visible as context behind the modal.
    if state.pending_git_prompt.is_some() {
        render_git_prompt(frame, chunks[listing_idx], state);
    }
}

/// Render the folder listing inside `area` with a phosphor-framed block
/// and a bold-white cwd title.
fn render_listing(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let title = Span::styled(
        format!(
            " {} ",
            crate::tui::shorten_home(&state.cwd.display().to_string())
        ),
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(title);

    let selected = state.list_state.selected;
    let highlight_style = Style::default()
        .bg(PHOSPHOR_GREEN)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let base_style = Style::default().fg(PHOSPHOR_GREEN);
    let git_suffix_style = Style::default()
        .fg(PHOSPHOR_DIM)
        .add_modifier(Modifier::BOLD);

    let lines: Vec<Line> = state
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let is_sel = Some(i) == selected;
            let name_slash = if e.is_parent {
                "../".to_string()
            } else {
                format!("{}/", e.name)
            };
            if is_sel {
                // Highlight row: single span covering name + optional git suffix.
                let mut text = format!("  {name_slash}");
                if e.is_git {
                    text.push_str(" (git)");
                }
                Line::from(Span::styled(text, highlight_style))
            } else if e.is_git {
                Line::from(vec![
                    Span::styled(format!("  {name_slash}"), base_style),
                    Span::styled(" (git)", git_suffix_style),
                ])
            } else {
                Line::from(Span::styled(format!("  {name_slash}"), base_style))
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

/// Render the bottom footer legend. Swaps the usual nav+`s` legend for a
/// prompt-focused legend when the git-repo confirm overlay is active.
fn render_footer_legend(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    use ratatui::layout::Alignment;
    let key = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text = Style::default().fg(PHOSPHOR_GREEN);
    let sep = Style::default().fg(PHOSPHOR_DARK);
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
            Span::styled("H/\u{2190}", key),
            Span::styled(" up", text),
            Span::raw("   "),
            Span::styled("S", key),
            Span::styled(" select", text),
            Span::raw("   "),
            Span::styled("Esc", key),
            Span::styled(" up/cancel", text),
        ])
    };
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

/// Build the three focus-styled button spans for the git-repo prompt.
fn git_prompt_buttons(focus: GitPromptFocus) -> Line<'static> {
    let focused = Style::default()
        .bg(WHITE)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let unfocused = Style::default()
        .bg(PHOSPHOR_GREEN)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let btn = |target: GitPromptFocus, label: &'static str| -> Span<'static> {
        let style = if focus == target { focused } else { unfocused };
        Span::styled(format!(" {label} "), style)
    };
    Line::from(vec![
        btn(GitPromptFocus::MountHere, "Mount this repository"),
        Span::raw("  "),
        btn(GitPromptFocus::EnterIn, "Pick a subdirectory"),
        Span::raw("  "),
        btn(GitPromptFocus::Cancel, "Cancel"),
    ])
}

/// Build the M/E/C hint footer line for the git-repo prompt.
fn git_prompt_hint() -> Line<'static> {
    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    Line::from(vec![
        Span::styled("M", key_style),
        Span::styled(" mount", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("E", key_style),
        Span::styled(" enter", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("C/Esc", key_style),
        Span::styled(" cancel", text_style),
    ])
}

/// Overlay renderer for the in-browser "Git repository detected" prompt.
fn render_git_prompt(frame: &mut Frame, parent: Rect, state: &FileBrowserState) {
    use ratatui::layout::{Alignment, Constraint, Direction, Layout};

    // Add a row when we have an origin URL to show under the title.
    let has_url = state.pending_git_url.is_some();
    let w = parent.width.saturating_sub(4).min(60);
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
        .border_style(Style::default().fg(PHOSPHOR_GREEN))
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
        Paragraph::new(git_prompt_hint()).alignment(Alignment::Center),
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

    fn make_state_at(path: PathBuf) -> FileBrowserState {
        FileBrowserState::new_at(path.clone(), path)
    }

    fn state_rooted_at(root: PathBuf, cwd: PathBuf) -> FileBrowserState {
        FileBrowserState::new_at(root, cwd)
    }

    // ── Filtering + directory-only listing ────────────────────────────

    #[test]
    fn filter_excludes_files() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("folder")).unwrap();
        std::fs::write(tmp.path().join("file.txt"), b"x").unwrap();

        let state = make_state_at(tmp.path().to_path_buf());
        let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"folder"), "folder missing: {names:?}");
        assert!(
            !names.contains(&"file.txt"),
            "file should be filtered out: {names:?}"
        );
    }

    #[test]
    fn hidden_files_are_excluded() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("visible")).unwrap();
        std::fs::create_dir(tmp.path().join(".hidden")).unwrap();

        let state = make_state_at(tmp.path().to_path_buf());
        let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"visible"));
        assert!(!names.contains(&".hidden"));
    }

    #[test]
    fn excluded_names_filtered_at_root() {
        let tmp = tempdir().unwrap();
        for name in EXCLUDED {
            std::fs::create_dir(tmp.path().join(name)).unwrap();
        }
        std::fs::create_dir(tmp.path().join("Projects")).unwrap();

        let state = make_state_at(tmp.path().to_path_buf());
        let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
        for name in EXCLUDED {
            assert!(!names.contains(name), "excluded `{name}` slipped through");
        }
        assert!(names.contains(&"Projects"));
    }

    #[test]
    fn excluded_names_visible_below_root() {
        // EXCLUDED only applies at the sandbox root; a folder named
        // "Library" one level below should still be visible.
        let tmp = tempdir().unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir_all(sub.join("Library")).unwrap();

        let state = state_rooted_at(tmp.path().to_path_buf(), sub);
        let names: Vec<&str> = state.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Library"));
    }

    #[test]
    fn parent_link_absent_at_root() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("a")).unwrap();
        let state = make_state_at(tmp.path().to_path_buf());
        assert!(
            state.entries.iter().all(|e| !e.is_parent),
            "`..` must not appear at root: {:?}",
            state.entries
        );
    }

    #[test]
    fn parent_link_present_below_root() {
        let tmp = tempdir().unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let state = state_rooted_at(tmp.path().to_path_buf(), sub);
        assert!(state.entries.first().is_some_and(|e| e.is_parent));
    }

    // ── Git-repo detection ────────────────────────────────────────────

    #[test]
    fn git_repo_entries_have_is_git_true() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let state = make_state_at(tmp.path().to_path_buf());
        let entry = state
            .entries
            .iter()
            .find(|e| e.name == "repo")
            .expect("repo row must exist");
        assert!(entry.is_git, "repo row must be flagged as git");
    }

    #[test]
    fn non_git_folders_have_is_git_false() {
        let tmp = tempdir().unwrap();
        let plain = tmp.path().join("plain");
        std::fs::create_dir(&plain).unwrap();

        let state = make_state_at(tmp.path().to_path_buf());
        let entry = state
            .entries
            .iter()
            .find(|e| e.name == "plain")
            .expect("plain row must exist");
        assert!(!entry.is_git);
    }

    #[test]
    fn submodule_gitfile_counts_as_git() {
        let tmp = tempdir().unwrap();
        let sub = tmp.path().join("submodule");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join(".git"), "gitdir: ../.git/modules/submodule\n").unwrap();

        let state = make_state_at(tmp.path().to_path_buf());
        let entry = state
            .entries
            .iter()
            .find(|e| e.name == "submodule")
            .expect("submodule row must exist");
        assert!(entry.is_git);
    }

    // ── Render: ensure the ` (git)` suffix actually appears ───────────

    #[test]
    fn git_entries_render_with_git_suffix() {
        use ratatui::{Terminal, backend::TestBackend};

        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir(tmp.path().join("plain")).unwrap();

        // Use a state where the selection is NOT on the git row, so the
        // suffix renders as a separate span rather than getting absorbed
        // into the highlight style.
        let mut state = make_state_at(tmp.path().to_path_buf());
        // Sort order is alphabetical lowercase: plain < repo. Select plain
        // (index 0) so repo's ` (git)` suffix renders unhighlighted.
        state.list_state.select(Some(0));

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let dump = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(dump.contains("repo/"), "repo row should render: {dump:?}");
        assert!(
            dump.contains("(git)"),
            "git suffix should render on the repo row: {dump:?}"
        );
        assert!(dump.contains("plain/"));
    }

    // ── `s` behaviour ─────────────────────────────────────────────────

    #[test]
    fn s_commits_highlighted_entry() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let child = parent.join("child");
        std::fs::create_dir_all(&child).unwrap();

        // root = tmp so that neither parent nor child trip the $HOME guard.
        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent.clone());
        // Highlighted entry at index 0 is `..`; advance to `child`.
        state.handle_key(key(KeyCode::Down));

        let outcome = state.handle_key(key(KeyCode::Char('s')));
        match outcome {
            ModalOutcome::Commit(path) => {
                assert_eq!(path.canonicalize().unwrap(), child.canonicalize().unwrap(),);
            }
            other => panic!("expected Commit, got {other:?}"),
        }
    }

    #[test]
    fn s_falls_back_to_cwd_when_directory_is_empty() {
        let tmp = tempdir().unwrap();
        let empty = tmp.path().join("empty");
        std::fs::create_dir(&empty).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), empty.clone());
        // Empty except for `..` — `s` should commit cwd, not `..`.
        let outcome = state.handle_key(key(KeyCode::Char('s')));
        match outcome {
            ModalOutcome::Commit(path) => {
                assert_eq!(path.canonicalize().unwrap(), empty.canonicalize().unwrap(),);
            }
            other => panic!("expected Commit, got {other:?}"),
        }
    }

    #[test]
    fn s_rejects_root_itself() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        let outcome = state.handle_key(key(KeyCode::Char('s')));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert!(state.rejected_reason.is_some());
    }

    #[test]
    fn s_rejects_jackin_data_dir() {
        let tmp = tempdir().unwrap();
        let jackin = tmp.path().join(".jackin").join("workspaces");
        std::fs::create_dir_all(&jackin).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), jackin);
        let outcome = state.handle_key(key(KeyCode::Char('s')));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert!(state.rejected_reason.is_some());
    }

    #[test]
    fn rejection_cleared_on_next_keypress() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        state.handle_key(key(KeyCode::Char('s')));
        assert!(state.rejected_reason.is_some());
        state.handle_key(key(KeyCode::Char('j')));
        assert!(state.rejected_reason.is_none());
    }

    // ── Esc step-back navigation ──────────────────────────────────────

    #[test]
    fn esc_at_root_cancels_modal() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        let outcome = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(outcome, ModalOutcome::Cancel));
    }

    #[test]
    fn esc_inside_subfolder_navigates_up() {
        let tmp = tempdir().unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), sub.clone());
        let outcome = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(outcome, ModalOutcome::Continue));
        assert_eq!(
            state.cwd.canonicalize().unwrap(),
            tmp.path().canonicalize().unwrap(),
        );
    }

    #[test]
    fn esc_deep_navigates_up_one_level() {
        let tmp = tempdir().unwrap();
        let l1 = tmp.path().join("a");
        let l2 = l1.join("b");
        let l3 = l2.join("c");
        std::fs::create_dir_all(&l3).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), l3);
        state.handle_key(key(KeyCode::Esc));
        assert_eq!(
            state.cwd.canonicalize().unwrap(),
            l2.canonicalize().unwrap(),
        );
    }

    #[test]
    fn esc_clears_rejected_reason() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        state.rejected_reason = Some("stale reason".into());
        let outcome = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(outcome, ModalOutcome::Cancel));
        assert!(state.rejected_reason.is_none());
    }

    // ── h / l navigation ──────────────────────────────────────────────

    #[test]
    fn h_navigates_up() {
        let tmp = tempdir().unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let mut state = state_rooted_at(tmp.path().to_path_buf(), sub);
        state.handle_key(key(KeyCode::Char('h')));
        assert_eq!(
            state.cwd.canonicalize().unwrap(),
            tmp.path().canonicalize().unwrap(),
        );
    }

    #[test]
    fn h_at_root_is_noop() {
        let tmp = tempdir().unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        state.handle_key(key(KeyCode::Char('h')));
        assert_eq!(state.cwd, tmp.path());
    }

    #[test]
    fn l_navigates_into_highlighted_dir() {
        let tmp = tempdir().unwrap();
        let child = tmp.path().join("child");
        std::fs::create_dir(&child).unwrap();
        let mut state = make_state_at(tmp.path().to_path_buf());
        // No `..` at root — index 0 is `child`.
        state.handle_key(key(KeyCode::Char('l')));
        assert_eq!(
            state.cwd.canonicalize().unwrap(),
            child.canonicalize().unwrap(),
        );
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
    fn enter_on_plain_folder_still_navigates() {
        let tmp = tempdir().unwrap();
        let parent = tmp.path().join("parent");
        let plain = parent.join("plain");
        std::fs::create_dir_all(&plain).unwrap();

        let mut state = state_rooted_at(tmp.path().to_path_buf(), parent);
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        assert!(state.pending_git_prompt.is_none());
        assert_eq!(
            state.cwd.canonicalize().unwrap(),
            plain.canonicalize().unwrap(),
        );
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
}

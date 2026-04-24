//! Host folder picker — wraps ratatui-explorer, shows folders only,
//! adds `s` as "select current folder".

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use directories::BaseDirs;
use ratatui::{Frame, layout::Rect, widgets::FrameExt as _};
use ratatui_explorer::{FileExplorer, FileExplorerBuilder, Theme};

use super::ModalOutcome;

pub struct FileBrowserState {
    pub explorer: FileExplorer,
    pub root_hint: PathBuf,
}

impl std::fmt::Debug for FileBrowserState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileBrowserState")
            .field("root_hint", &self.root_hint)
            .finish_non_exhaustive()
    }
}

impl FileBrowserState {
    /// Build a new browser rooted at the given start path. Filters out
    /// non-directories so only folders are pickable.
    pub fn new(start: PathBuf) -> anyhow::Result<Self> {
        let theme = Theme::default().add_default_title();
        let explorer = FileExplorerBuilder::default()
            .working_dir(&start)
            .theme(theme)
            .filter_map(|entry| if entry.is_dir { Some(entry) } else { None })
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build file explorer: {e}"))?;
        Ok(Self {
            explorer,
            root_hint: start,
        })
    }

    pub fn new_from_home() -> anyhow::Result<Self> {
        let home = BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("could not resolve $HOME"))?;
        Self::new(home)
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<PathBuf> {
        match key.code {
            KeyCode::Char('s') => ModalOutcome::Commit(self.explorer.current().path.clone()),
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => {
                let event = crossterm::event::Event::Key(key);
                let _ = self.explorer.handle(&event);
                ModalOutcome::Continue
            }
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    use ratatui::{
        layout::{Constraint, Direction, Layout},
        style::{Color, Modifier, Style},
        text::Span,
        widgets::Paragraph,
    };

    frame.render_widget(ratatui::widgets::Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    frame.render_widget_ref(state.explorer.widget(), chunks[0]);

    let hint = Span::styled(
        "↑↓ navigate · Enter open · h/← up · s select · Esc cancel",
        Style::default()
            .fg(Color::Rgb(0, 140, 30))
            .add_modifier(Modifier::ITALIC),
    );
    frame.render_widget(Paragraph::new(hint), chunks[1]);
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

    #[test]
    fn new_seeds_cwd_to_given_start() {
        let tmp = tempdir().unwrap();
        let state = FileBrowserState::new(tmp.path().to_path_buf()).unwrap();
        assert_eq!(state.root_hint, tmp.path());
    }

    #[test]
    fn filter_excludes_files() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("folder")).unwrap();
        std::fs::write(tmp.path().join("file.txt"), b"x").unwrap();

        let state = FileBrowserState::new(tmp.path().to_path_buf()).unwrap();
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
    fn s_commits_currently_selected_path() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("folder")).unwrap();
        let mut state = FileBrowserState::new(tmp.path().to_path_buf()).unwrap();
        let outcome = state.handle_key(key(KeyCode::Char('s')));
        assert!(matches!(outcome, ModalOutcome::Commit(_)));
    }

    #[test]
    fn esc_cancels() {
        let tmp = tempdir().unwrap();
        let mut state = FileBrowserState::new(tmp.path().to_path_buf()).unwrap();
        assert!(matches!(
            state.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }
}

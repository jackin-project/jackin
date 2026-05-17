//! Two-button confirmation modal shown before committing an editor save.
//!
//! Replaces the old inline "bare S → save immediately" flow with a
//! preview dialog: operator reviews a list of changes (pre-built by the
//! caller in [`ConfirmSaveState`]), then picks `Save` or `Cancel`.
//! Mount-collapse warnings fold into the same dialog as an extra
//! section so the operator sees ONE confirm for the full plan.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::ModalOutcome;

use super::scrollable::{apply_scroll_delta, render_lines_with_offset_in_area};
use super::{PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveChoice {
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmSaveFocus {
    Save,
    Cancel,
}

/// State for the `ConfirmSave` modal. The caller pre-builds the content
/// `lines` from the editor state so the widget itself stays dumb.
///
/// `effective_removals` and `final_mounts` carry the planner's output
/// through the confirm step — `input.rs::save_editor` consumes them when
/// the operator commits, and no second `plan_edit`/`plan_create` call is
/// needed after confirmation.
#[derive(Debug, Clone)]
pub struct ConfirmSaveState {
    pub lines: Vec<Line<'static>>,
    pub focus: ConfirmSaveFocus,
    /// Vertical scroll offset — how many lines are hidden above the visible window.
    pub scroll_offset: u16,
    /// `plan_edit`'s `effective_removals`, forwarded into
    /// `edit_workspace`. Empty for Create flows.
    pub effective_removals: Vec<String>,
    /// `plan_create`'s collapsed mount set. Empty (meaning "no override
    /// needed") for Edit flows.
    pub final_mounts: Option<Vec<crate::workspace::MountConfig>>,
    /// `true` when the plan carries mount-collapses.
    pub has_collapses: bool,
}

impl ConfirmSaveState {
    /// Build a new `ConfirmSave` modal. Default focus = Save so the
    /// operator can confirm with a single Enter after reviewing the diff.
    #[must_use]
    pub const fn new(lines: Vec<Line<'static>>) -> Self {
        Self {
            lines,
            focus: ConfirmSaveFocus::Save,
            scroll_offset: 0,
            effective_removals: Vec::new(),
            final_mounts: None,
            has_collapses: false,
        }
    }

    pub const fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<SaveChoice> {
        match key.code {
            KeyCode::Char('s' | 'S') => ModalOutcome::Commit(SaveChoice::Save),
            KeyCode::Char('c' | 'C') | KeyCode::Esc => ModalOutcome::Cancel,
            // Up/Down/j/k scroll the content preview.
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                apply_scroll_delta(&mut self.scroll_offset, -1);
                ModalOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                apply_scroll_delta(&mut self.scroll_offset, 1);
                ModalOutcome::Continue
            }
            // Tab / BackTab / Right / Left — only two buttons,
            // so every "move focus" key just toggles between them.
            KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Right
            | KeyCode::Left
            | KeyCode::Char('l' | 'L' | 'h' | 'H') => {
                self.focus = match self.focus {
                    ConfirmSaveFocus::Save => ConfirmSaveFocus::Cancel,
                    ConfirmSaveFocus::Cancel => ConfirmSaveFocus::Save,
                };
                ModalOutcome::Continue
            }
            KeyCode::Enter => match self.focus {
                ConfirmSaveFocus::Save => ModalOutcome::Commit(SaveChoice::Save),
                ConfirmSaveFocus::Cancel => ModalOutcome::Cancel,
            },
            _ => ModalOutcome::Continue,
        }
    }
}

/// Total rows the `ConfirmSave` modal wants given its current line count.
/// Layout: top border + blank + N content lines + blank + buttons + bottom border = N + 5.
#[must_use]
pub fn required_height(state: &ConfirmSaveState) -> u16 {
    let lines = u16::try_from(state.lines.len()).unwrap_or(u16::MAX);
    lines.saturating_add(5)
}

pub fn render(frame: &mut Frame, area: Rect, state: &ConfirmSaveState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " Confirm changes ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    // Compute how many content rows we can afford, then apply the scroll
    // offset so the operator can page through long diffs.
    let content_rows = inner.height.saturating_sub(3); // blank, blank, buttons
    let content_rows = content_rows.saturating_sub(1); // bottom-of-content blank
    let content_area_height = content_rows;

    // Content indented by SUBPANEL_CONTENT_INDENT (2). The caller is
    // responsible for any deeper indentation; we just add a uniform
    // left gutter so lines don't butt up against the border.
    let indented: Vec<Line> = state
        .lines
        .iter()
        .cloned()
        .map(|l| {
            let mut spans = vec![Span::raw("  ")];
            spans.extend(l.spans);
            Line::from(spans)
        })
        .collect();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top blank
            Constraint::Length(content_area_height),
            Constraint::Length(1), // blank
            Constraint::Length(1), // buttons
        ])
        .split(inner);

    render_lines_with_offset_in_area(frame, chunks[1], indented, state.scroll_offset);

    // Buttons — focused choice highlights on white.
    let focused_style = Style::default()
        .bg(WHITE)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let unfocused_style = Style::default()
        .fg(PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);

    let save_style = if matches!(state.focus, ConfirmSaveFocus::Save) {
        focused_style
    } else {
        unfocused_style
    };
    let cancel_style = if matches!(state.focus, ConfirmSaveFocus::Cancel) {
        focused_style
    } else {
        unfocused_style
    };

    let button_line = Line::from(vec![
        Span::styled("  Save  ", save_style),
        Span::raw("    "),
        Span::styled("  Cancel  ", cancel_style),
    ]);
    frame.render_widget(
        Paragraph::new(button_line).alignment(Alignment::Center),
        chunks[3],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn sample_state() -> ConfirmSaveState {
        ConfirmSaveState::new(vec![Line::from("Create workspace: demo")])
    }

    #[test]
    fn confirm_save_defaults_to_save_focus() {
        let s = sample_state();
        assert_eq!(s.focus, ConfirmSaveFocus::Save);
    }

    #[test]
    fn confirm_save_tab_cycles_save_cancel() {
        let mut s = sample_state();
        assert_eq!(s.focus, ConfirmSaveFocus::Save);
        assert!(matches!(
            s.handle_key(key(KeyCode::Tab)),
            ModalOutcome::Continue
        ));
        assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
        s.handle_key(key(KeyCode::Tab));
        assert_eq!(s.focus, ConfirmSaveFocus::Save);
    }

    #[test]
    fn confirm_save_left_cycles_reverse() {
        let mut s = sample_state();
        s.handle_key(key(KeyCode::Left));
        assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
        s.handle_key(key(KeyCode::Left));
        assert_eq!(s.focus, ConfirmSaveFocus::Save);
    }

    #[test]
    fn confirm_save_enter_on_save_commits_save_choice() {
        let mut s = sample_state();
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Commit(SaveChoice::Save)
        ));
    }

    #[test]
    fn confirm_save_enter_on_cancel_returns_cancel() {
        let mut s = sample_state();
        s.handle_key(key(KeyCode::Tab)); // Save -> Cancel
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn confirm_save_s_shortcut_commits_save() {
        let mut s = sample_state();
        // Rotate focus first to prove the shortcut is focus-independent.
        s.handle_key(key(KeyCode::Tab)); // -> Cancel
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('s'))),
            ModalOutcome::Commit(SaveChoice::Save)
        ));

        let mut s = sample_state();
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('S'))),
            ModalOutcome::Commit(SaveChoice::Save)
        ));
    }

    #[test]
    fn confirm_save_c_shortcut_cancels() {
        let mut s = sample_state();
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('c'))),
            ModalOutcome::Cancel
        ));

        let mut s = sample_state();
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('C'))),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn confirm_save_esc_cancels() {
        let mut s = sample_state();
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn required_height_accounts_for_chrome() {
        let s = ConfirmSaveState::new(vec![
            Line::from("one"),
            Line::from("two"),
            Line::from("three"),
        ]);
        // 3 content lines + 5 chrome rows (2 borders + top blank + after-content blank + buttons)
        assert_eq!(required_height(&s), 8);
    }
}

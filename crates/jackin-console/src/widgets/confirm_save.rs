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
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders},
};

use super::ModalOutcome;

use jackin_tui::components::scrollable_panel::{
    apply_scroll_delta, clamp_scroll_offset, render_lines_with_offset_in_area,
};
use jackin_tui::components::{Panel, PanelFocus};

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
pub struct ConfirmSaveState<M: Clone = ()> {
    pub lines: Vec<Line<'static>>,
    pub focus: ConfirmSaveFocus,
    /// Vertical scroll offset — how many lines are hidden above the visible window.
    pub scroll_offset: u16,
    preview_rows: u16,
    /// `plan_edit`'s `effective_removals`, forwarded into
    /// `edit_workspace`. Empty for Create flows.
    pub effective_removals: Vec<String>,
    /// `plan_create`'s collapsed mount set. Empty (meaning "no override
    /// needed") for Edit flows.
    pub final_mounts: Option<Vec<M>>,
    /// `true` when the plan carries mount-collapses.
    pub has_collapses: bool,
}

impl<M: Clone> ConfirmSaveState<M> {
    /// Build a new `ConfirmSave` modal. Default focus = Cancel so that Enter
    /// on a freshly-opened confirm never fires the destructive arm (RULE 7).
    #[must_use]
    pub const fn new(lines: Vec<Line<'static>>) -> Self {
        Self {
            lines,
            focus: ConfirmSaveFocus::Cancel,
            scroll_offset: 0,
            preview_rows: 0,
            effective_removals: Vec::new(),
            final_mounts: None,
            has_collapses: false,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<SaveChoice> {
        match key.code {
            KeyCode::Char('s' | 'S') => ModalOutcome::Commit(SaveChoice::Save),
            KeyCode::Char('c' | 'C') | KeyCode::Esc => ModalOutcome::Cancel,
            // Up/Down/j/k scroll the content preview.
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                self.scroll_preview_by(-1);
                ModalOutcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                self.scroll_preview_by(1);
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

    fn scroll_preview_by(&mut self, delta: isize) {
        apply_scroll_delta(
            &mut self.scroll_offset,
            delta as i16,
            usize::from(self.preview_rows),
            self.lines.len(),
        );
    }
}

/// Total rows the `ConfirmSave` modal wants given its current line count.
/// Layout: top border + blank + N content lines + blank + buttons + bottom border = N + 5.
#[must_use]
pub fn required_height<M: Clone>(state: &ConfirmSaveState<M>) -> u16 {
    let lines = u16::try_from(state.lines.len()).unwrap_or(u16::MAX);
    lines.saturating_add(5)
}

pub fn prepare_for_render<M: Clone>(area: Rect, state: &mut ConfirmSaveState<M>) {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let content_rows = inner.height.saturating_sub(3); // blank, blank, buttons
    let content_rows = content_rows.saturating_sub(1); // bottom-of-content blank
    state.preview_rows = content_rows;
    clamp_scroll_offset(
        state.lines.len(),
        usize::from(state.preview_rows),
        &mut state.scroll_offset,
    );
}

pub fn render<M: Clone>(frame: &mut Frame, area: Rect, state: &ConfirmSaveState<M>) {
    let block = Panel::new()
        .title(" Confirm changes ")
        .focus(PanelFocus::Focused)
        .block();
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

    let items = [
        jackin_tui::components::ButtonStripItem::new("Save"),
        jackin_tui::components::ButtonStripItem::new("Cancel"),
    ];
    let focused = match state.focus {
        ConfirmSaveFocus::Save => 0,
        ConfirmSaveFocus::Cancel => 1,
    };
    jackin_tui::components::ButtonStrip::new(&items)
        .focused(focused)
        .render(frame, chunks[3]);
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
    fn confirm_save_defaults_to_cancel_focus() {
        // Default = Cancel so Enter on a freshly-opened dialog never fires
        // the save arm (TUI design decisions: confirmation dialog rule).
        let s = sample_state();
        assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
    }

    #[test]
    fn confirm_save_tab_cycles_cancel_save() {
        let mut s = sample_state();
        assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
        assert!(matches!(
            s.handle_key(key(KeyCode::Tab)),
            ModalOutcome::Continue
        ));
        assert_eq!(s.focus, ConfirmSaveFocus::Save);
        s.handle_key(key(KeyCode::Tab));
        assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
    }

    #[test]
    fn confirm_save_left_cycles_reverse() {
        let mut s = sample_state();
        // Starts at Cancel; Left toggles to Save.
        s.handle_key(key(KeyCode::Left));
        assert_eq!(s.focus, ConfirmSaveFocus::Save);
        s.handle_key(key(KeyCode::Left));
        assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
    }

    #[test]
    fn confirm_save_enter_on_cancel_returns_cancel() {
        // Default focus = Cancel, so Enter fires Cancel immediately.
        let mut s = sample_state();
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn confirm_save_enter_on_save_commits_save_choice() {
        // Tab once (Cancel -> Save) then Enter commits Save.
        let mut s = sample_state();
        s.handle_key(key(KeyCode::Tab)); // Cancel -> Save
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Commit(SaveChoice::Save)
        ));
    }

    #[test]
    fn confirm_save_s_shortcut_commits_save() {
        let mut s = sample_state();
        // Rotate focus first to prove the shortcut is focus-independent.
        s.handle_key(key(KeyCode::Tab)); // Cancel -> Save
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
        let s = ConfirmSaveState::<()>::new(vec![
            Line::from("one"),
            Line::from("two"),
            Line::from("three"),
        ]);
        // 3 content lines + 5 chrome rows (2 borders + top blank + after-content blank + buttons)
        assert_eq!(required_height(&s), 8);
    }

    #[test]
    fn confirm_save_scroll_keys_start_from_clamped_offset() {
        let mut s = ConfirmSaveState::<()>::new(vec![
            Line::from("one"),
            Line::from("two"),
            Line::from("three"),
            Line::from("four"),
        ]);
        s.preview_rows = 2;
        s.scroll_offset = 99;

        s.handle_key(key(KeyCode::Down));
        assert_eq!(s.scroll_offset, 2);

        s.handle_key(key(KeyCode::Up));
        assert_eq!(s.scroll_offset, 1);
    }
}

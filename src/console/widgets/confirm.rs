//! Y/N confirmation modal with keyboard focus.
//!
//! Y / N / Esc return distinct outcomes; case-insensitive.
//! Tab / ←→ / h/l cycle focus between Yes and No.
//! Enter commits the focused button.

use crossterm::event::{KeyCode, KeyEvent};

use super::ModalOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmFocus {
    Yes,
    No,
}

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub focus: ConfirmFocus,
    pub title: String,
    pub kind: ConfirmKind,
}

/// Discriminated payload for the Confirm modal.
///
/// `Default` carries a free-form prompt string; `RoleTrust` carries the
/// role key and repository URL as separate fields so the renderer can lay
/// them out without parsing them back out of a `\n`-delimited blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmKind {
    Default { prompt: String },
    RoleTrust { role: String, repository: String },
}

impl ConfirmState {
    /// Build a new Confirm modal. Default focus = No (safer for
    /// destructive actions — Enter won't accidentally commit Yes).
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            focus: ConfirmFocus::No,
            title: "Confirm".into(),
            kind: ConfirmKind::Default {
                prompt: prompt.into(),
            },
        }
    }

    pub fn role_trust(role: impl Into<String>, repository: impl Into<String>) -> Self {
        Self {
            focus: ConfirmFocus::No,
            title: "Trust role source".into(),
            kind: ConfirmKind::RoleTrust {
                role: role.into(),
                repository: repository.into(),
            },
        }
    }

    pub const fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<bool> {
        match key.code {
            // Direct shortcuts (case-insensitive).
            KeyCode::Char('y' | 'Y') => ModalOutcome::Commit(true),
            KeyCode::Char('n' | 'N') => ModalOutcome::Commit(false),
            // Focus-based interaction — Tab/←→/h/l all toggle focus.
            KeyCode::Tab | KeyCode::Right | KeyCode::Left | KeyCode::Char('l' | 'h') => {
                self.focus = match self.focus {
                    ConfirmFocus::Yes => ConfirmFocus::No,
                    ConfirmFocus::No => ConfirmFocus::Yes,
                };
                ModalOutcome::Continue
            }
            KeyCode::Enter => ModalOutcome::Commit(matches!(self.focus, ConfirmFocus::Yes)),
            KeyCode::Esc => ModalOutcome::Cancel,
            _ => ModalOutcome::Continue,
        }
    }
}

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
const WHITE: Color = Color::Rgb(255, 255, 255);
const WARNING_YELLOW: Color = Color::Rgb(255, 216, 94);

/// Height (rows) this Confirm modal wants, given its current contents.
///
/// `Default` kind: N prompt lines + 6 chrome rows (top/bottom border = 2,
/// spacer, buttons, spacer, hint). `RoleTrust` uses a fixed 12-row layout
/// matching the structured renderer in `render_role_trust`.
#[must_use]
pub fn required_height(state: &ConfirmState) -> u16 {
    match &state.kind {
        ConfirmKind::RoleTrust { .. } => 12,
        ConfirmKind::Default { prompt } => {
            let prompt_lines = prompt.lines().count().max(1) as u16;
            prompt_lines + 6
        }
    }
}

#[must_use]
pub const fn width_pct(state: &ConfirmState) -> u16 {
    match &state.kind {
        ConfirmKind::Default { .. } => 60,
        ConfirmKind::RoleTrust { .. } => 70,
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &ConfirmState) {
    // Outer block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            format!(" {} ", state.title),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    let prompt = match &state.kind {
        ConfirmKind::RoleTrust { role, repository } => {
            render_role_trust(frame, inner, state, role, repository);
            return;
        }
        ConfirmKind::Default { prompt } => prompt.as_str(),
    };

    // Vertical layout inside the inner rect. The prompt area grows with the
    // number of lines in `prompt` so multi-line confirmations (e.g.
    // the mount-collapse prompt) render without clipping.
    let prompt_lines = prompt.lines().count().max(1) as u16;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(prompt_lines), // prompt (may span multiple lines)
            Constraint::Length(1),            // spacer
            Constraint::Length(1),            // button row
            Constraint::Length(1),            // spacer between buttons and hint
            Constraint::Length(1),            // footer hint
        ])
        .split(inner);

    // Prompt — render each line in turn so centering applies per-line.
    let prompt_lines_vec: Vec<Line> = prompt
        .lines()
        .map(|l| {
            Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ))
        })
        .collect();
    let prompt = Paragraph::new(prompt_lines_vec).alignment(Alignment::Center);
    frame.render_widget(prompt, chunks[0]);

    // Button row — focused choice highlights on white; unfocused stays
    // flush with the modal background so only the focused choice pops.
    let yes_focused = matches!(state.focus, ConfirmFocus::Yes);
    let no_focused = matches!(state.focus, ConfirmFocus::No);

    let focused_style = Style::default()
        .bg(WHITE)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let unfocused_style = Style::default()
        .fg(PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);

    let yes_btn_style = if yes_focused {
        focused_style
    } else {
        unfocused_style
    };
    let no_btn_style = if no_focused {
        focused_style
    } else {
        unfocused_style
    };

    let button_line = Line::from(vec![
        Span::styled("  Yes  ", yes_btn_style),
        Span::raw("    "),
        Span::styled("  No  ", no_btn_style),
    ]);
    frame.render_widget(
        Paragraph::new(button_line).alignment(Alignment::Center),
        chunks[2],
    );

    // Footer hint — same key/text/sep scheme as the main TUI footer.
    let key = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text = Style::default().fg(PHOSPHOR_GREEN);
    let sep = Style::default().fg(PHOSPHOR_DARK);
    let hint = Paragraph::new(ratatui::text::Line::from(vec![
        Span::styled("Y", key),
        Span::styled(" yes", text),
        Span::styled(" \u{b7} ", sep),
        Span::styled("N", key),
        Span::styled(" no", text),
        Span::styled(" \u{b7} ", sep),
        Span::styled("Esc", key),
        Span::styled(" cancel", text),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(hint, chunks[4]);
}

fn render_role_trust(
    frame: &mut Frame,
    inner: Rect,
    state: &ConfirmState,
    role: &str,
    repository: &str,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let key = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let value = Style::default()
        .fg(PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);
    let note = Style::default().fg(PHOSPHOR_DIM);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Trust this role source?",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Left),
        inset(rows[0], 3),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Role: ", key),
            Span::styled(role, value),
        ])),
        inset(rows[2], 3),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Repository: ", key),
            Span::styled(repository, value),
        ])),
        inset(rows[3], 3),
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    "!",
                    Style::default()
                        .fg(WARNING_YELLOW)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled("Dockerfile can run during image builds.", note),
            ]),
            Line::from(vec![
                Span::styled(
                    "!",
                    Style::default()
                        .fg(WARNING_YELLOW)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled("The role can access mounted workspace files.", note),
            ]),
        ]),
        inset(rows[5], 3),
    );

    render_buttons(frame, rows[6], state);
    render_hint(frame, rows[7]);
}

const fn inset(area: Rect, x: u16) -> Rect {
    Rect {
        x: area.x.saturating_add(x),
        y: area.y,
        width: area.width.saturating_sub(x.saturating_mul(2)),
        height: area.height,
    }
}

fn render_buttons(frame: &mut Frame, area: Rect, state: &ConfirmState) {
    let yes_focused = matches!(state.focus, ConfirmFocus::Yes);
    let no_focused = matches!(state.focus, ConfirmFocus::No);

    let focused_style = Style::default()
        .bg(WHITE)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let unfocused_style = Style::default()
        .fg(PHOSPHOR_GREEN)
        .add_modifier(Modifier::BOLD);

    let yes_btn_style = if yes_focused {
        focused_style
    } else {
        unfocused_style
    };
    let no_btn_style = if no_focused {
        focused_style
    } else {
        unfocused_style
    };

    let button_line = Line::from(vec![
        Span::styled("  Yes  ", yes_btn_style),
        Span::raw("    "),
        Span::styled("  No  ", no_btn_style),
    ]);
    frame.render_widget(
        Paragraph::new(button_line).alignment(Alignment::Center),
        area,
    );
}

fn render_hint(frame: &mut Frame, area: Rect) {
    let key = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text = Style::default().fg(PHOSPHOR_GREEN);
    let sep = Style::default().fg(PHOSPHOR_DARK);
    let hint = Paragraph::new(ratatui::text::Line::from(vec![
        Span::styled("Y", key),
        Span::styled(" yes", text),
        Span::styled(" \u{b7} ", sep),
        Span::styled("N", key),
        Span::styled(" no", text),
        Span::styled(" \u{b7} ", sep),
        Span::styled("Esc", key),
        Span::styled(" cancel", text),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(hint, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn y_commits_true() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('y'))),
            ModalOutcome::Commit(true)
        ));
    }

    #[test]
    fn uppercase_y_commits_true() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('Y'))),
            ModalOutcome::Commit(true)
        ));
    }

    #[test]
    fn n_commits_false() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('n'))),
            ModalOutcome::Commit(false)
        ));
    }

    #[test]
    fn esc_cancels() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    #[test]
    fn arrow_is_noop() {
        let mut s = ConfirmState::new("Delete?");
        assert!(matches!(
            s.handle_key(key(KeyCode::Down)),
            ModalOutcome::Continue
        ));
    }

    #[test]
    fn default_focus_is_no() {
        let s = ConfirmState::new("Delete?");
        assert_eq!(s.focus, ConfirmFocus::No);
    }

    #[test]
    fn tab_cycles_focus() {
        let mut s = ConfirmState::new("Delete?");
        assert_eq!(s.focus, ConfirmFocus::No);
        s.handle_key(key(KeyCode::Tab));
        assert_eq!(s.focus, ConfirmFocus::Yes);
        s.handle_key(key(KeyCode::Tab));
        assert_eq!(s.focus, ConfirmFocus::No);
    }

    #[test]
    fn enter_commits_focused_option() {
        let mut s = ConfirmState::new("Delete?");
        // Default focus is No, Enter commits No.
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Commit(false)
        ));

        let mut s = ConfirmState::new("Delete?");
        s.handle_key(key(KeyCode::Tab)); // focus Yes
        assert!(matches!(
            s.handle_key(key(KeyCode::Enter)),
            ModalOutcome::Commit(true)
        ));
    }

    #[test]
    fn y_still_works_regardless_of_focus() {
        let mut s = ConfirmState::new("Delete?");
        // Focus is No by default; Y should still commit true directly.
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('y'))),
            ModalOutcome::Commit(true)
        ));
    }

    #[test]
    fn role_trust_prompt_renders_readable_source_details() {
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};

        let s = ConfirmState::role_trust(
            "scentbird/agent-jones",
            "https://github.com/scentbird/jackin-agent-jones.git",
        );
        let area = Rect::new(0, 0, 100, required_height(&s));
        let backend = TestBackend::new(area.width, area.height);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, area, &s)).unwrap();

        let buf = term.backend().buffer();
        let mut rendered = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                rendered.push_str(buf[(x, y)].symbol());
            }
            rendered.push('\n');
        }

        assert!(rendered.contains("Trust role source"));
        assert!(rendered.contains("Role: scentbird/agent-jones"));
        assert!(
            rendered.contains("Repository: https://github.com/scentbird/jackin-agent-jones.git")
        );
        assert!(rendered.contains("Dockerfile can run during image builds."));
        assert!(rendered.contains("The role can access mounted workspace files."));
    }
}

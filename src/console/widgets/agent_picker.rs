//! Modal picker for agent disambiguation when launching a workspace
//! that has more than one eligible agent.
//!
//! Mirrors `github_picker`'s shape — one `Vec`-driven list +
//! `tui_widget_list::ListState` — so the manager can dispatch it with
//! the same Up/Down/Enter pattern. Adds a filter-as-you-type field so a
//! large agent roster can be narrowed in place.

use crossterm::event::{KeyCode, KeyEvent};
use tui_widget_list::ListState;

use super::ModalOutcome;
use crate::selector::ClassSelector;

#[derive(Debug)]
pub struct AgentPickerState {
    /// Eligibility-filtered set captured at open time; never mutated
    /// while the picker is up. Filter applies on top of this set.
    pub agents: Vec<ClassSelector>,
    pub list_state: ListState,
    pub filter: String,
    /// Subset of `agents` whose `key()` contains `filter` (case-insensitive).
    /// Recomputed on every filter mutation.
    pub filtered: Vec<ClassSelector>,
    /// Verb that follows `Enter` in the footer hint. Defaults to
    /// `select`. Constructed contexts override:
    ///
    /// - launch disambiguation (manager list → `Enter` on a workspace
    ///   with ≥2 eligible agents) → `launch`,
    /// - editor override-scope picking (Secrets tab `Specific agent`
    ///   branch) → `select`.
    ///
    /// The widget itself is identical in both cases — only the verb
    /// reads naturally for the operator's intent at the call site.
    pub confirm_label: String,
}

impl AgentPickerState {
    #[must_use]
    pub fn new(agents: Vec<ClassSelector>) -> Self {
        Self::with_confirm_label(agents, "select")
    }

    /// Same as [`AgentPickerState::new`] but with a caller-supplied verb
    /// for the `Enter` footer hint. Pass `"launch"` for the launch-
    /// disambiguation path; `"select"` (the default) for any "pick a
    /// row, then keep filling out a form" path.
    #[must_use]
    pub fn with_confirm_label(agents: Vec<ClassSelector>, confirm_label: &str) -> Self {
        let filtered = agents.clone();
        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            agents,
            list_state,
            filter: String::new(),
            filtered,
            confirm_label: confirm_label.to_string(),
        }
    }

    /// Recompute `filtered` from `agents` and the current `filter`. Anchors
    /// the selection at index 0 if the previous selection vanished.
    fn recompute_filtered(&mut self) {
        let needle = self.filter.to_ascii_lowercase();
        self.filtered = self
            .agents
            .iter()
            .filter(|agent| needle.is_empty() || agent.key().to_ascii_lowercase().contains(&needle))
            .cloned()
            .collect();
        if self.filtered.is_empty() {
            self.list_state.select(None);
        } else {
            // Always reset to the top after a filter change so the
            // operator never lands on a stale row index.
            self.list_state.select(Some(0));
        }
    }

    fn move_up(&mut self) {
        let n = self.filtered.len();
        if n > 0 {
            let next = self
                .list_state
                .selected
                .map_or(0, |i| if i == 0 { n - 1 } else { i - 1 });
            self.list_state.select(Some(next));
        }
    }

    fn move_down(&mut self) {
        let n = self.filtered.len();
        if n > 0 {
            let next = self
                .list_state
                .selected
                .map_or(0, |i| if i + 1 >= n { 0 } else { i + 1 });
            self.list_state.select(Some(next));
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<ClassSelector> {
        match key.code {
            KeyCode::Up => {
                self.move_up();
                ModalOutcome::Continue
            }
            KeyCode::Down => {
                self.move_down();
                ModalOutcome::Continue
            }
            KeyCode::Backspace => {
                if self.filter.pop().is_some() {
                    self.recompute_filtered();
                }
                ModalOutcome::Continue
            }
            KeyCode::Enter => {
                if let Some(i) = self.list_state.selected
                    && let Some(agent) = self.filtered.get(i)
                {
                    return ModalOutcome::Commit(agent.clone());
                }
                ModalOutcome::Continue
            }
            KeyCode::Esc => ModalOutcome::Cancel,
            KeyCode::Char(ch) => {
                // Every printable char appends to the filter — including
                // `j`/`k`, which would otherwise be ambiguous between
                // "type that letter" and "navigate the list" once the
                // filter is non-empty. Operators use the arrow keys for
                // navigation; the filter is the dominant interaction.
                self.filter.push(ch);
                self.recompute_filtered();
                ModalOutcome::Continue
            }
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
const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);

pub fn render(frame: &mut Frame, area: Rect, state: &AgentPickerState) {
    // Title style matches the rest of the launch TUI (WHITE + BOLD)
    // so the modal feels native next to OpPicker / GithubPicker.
    // Per the canonical list-modal layout (RULES.md "TUI List Modals"),
    // the filter buffer is NEVER part of the title — it lives on its own
    // dedicated row below the title bar.
    let title = Span::styled(
        " Select Agent ",
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(block, area);

    // Inner layout mirrors `OpPicker`'s pane stack:
    // filter row / spacer / list / spacer / footer.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // filter row
            Constraint::Length(1), // spacer
            Constraint::Min(1),    // list
            Constraint::Length(1), // spacer
            Constraint::Length(1), // footer
        ])
        .split(inner);

    // Filter row: `Filter: <buf>█` — placeholder dotted underline when
    // empty, cursor block at the end when populated. Same styling as
    // `OpPicker` so the two pickers feel like the same widget.
    let filter_line = if state.filter.is_empty() {
        Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(PHOSPHOR_DIM)),
            Span::styled("\u{2591}".repeat(20), Style::default().fg(PHOSPHOR_DARK)),
        ])
    } else {
        Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(PHOSPHOR_DIM)),
            Span::styled(state.filter.clone(), Style::default().fg(WHITE)),
            Span::styled(
                "\u{2588}",
                Style::default()
                    .fg(WHITE)
                    .add_modifier(Modifier::SLOW_BLINK),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(filter_line), rows[0]);

    // List body. When the filter narrows the visible set to nothing,
    // render no rows — the blank space below the filter row IS the
    // empty state. No `(no agents match)` placeholder per the canonical
    // list-modal layout.
    let lines: Vec<Line> = state
        .filtered
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let is_selected = Some(i) == state.list_state.selected;
            let prefix = if is_selected { "\u{25b8} " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(WHITE)
            };
            Line::from(vec![Span::styled(
                format!("{prefix}{}", agent.key()),
                style,
            )])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), rows[2]);

    // Footer hint — canonical key/text/sep styling, same separator
    // glyph and ordering as `OpPicker`. The Enter verb is supplied by
    // the caller (`launch` for launch disambiguation, `select` for the
    // override-scope path).
    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let confirm_label = format!(" {}", state.confirm_label);
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("\u{2191}\u{2193}", key_style),
        Span::styled(" navigate", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("type", key_style),
        Span::styled(" filter", text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("Enter", key_style),
        Span::styled(confirm_label, text_style),
        Span::styled(" \u{b7} ", sep_style),
        Span::styled("Esc", key_style),
        Span::styled(" cancel", text_style),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(hint, rows[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn agents(keys: &[&str]) -> Vec<ClassSelector> {
        keys.iter()
            .map(|k| ClassSelector::parse(k).expect("valid selector"))
            .collect()
    }

    #[test]
    fn new_selects_first_when_non_empty() {
        let s = AgentPickerState::new(agents(&["chainargos/agent-smith", "agent-brown"]));
        assert_eq!(s.list_state.selected, Some(0));
        assert_eq!(s.filtered.len(), 2);
    }

    #[test]
    fn new_selects_nothing_when_empty() {
        let s = AgentPickerState::new(vec![]);
        assert_eq!(s.list_state.selected, None);
    }

    #[test]
    fn enter_commits_selected_agent() {
        let mut s = AgentPickerState::new(agents(&[
            "chainargos/agent-smith",
            "chainargos/agent-brown",
        ]));
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome,
            ModalOutcome::Commit(a) if a.key() == "chainargos/agent-smith"));
    }

    #[test]
    fn esc_cancels() {
        let mut s = AgentPickerState::new(agents(&["agent-smith"]));
        assert!(matches!(
            s.handle_key(key(KeyCode::Esc)),
            ModalOutcome::Cancel
        ));
    }

    /// Typing into the filter narrows the visible set; agents whose key
    /// does not contain the substring are dropped.
    #[test]
    fn filter_narrows_agent_list() {
        let mut s = AgentPickerState::new(agents(&[
            "chainargos/agent-smith",
            "chainargos/agent-brown",
            "agent-architect",
        ]));
        for ch in "smith".chars() {
            s.handle_key(key(KeyCode::Char(ch)));
        }
        assert_eq!(s.filter, "smith");
        assert_eq!(s.filtered.len(), 1);
        assert_eq!(s.filtered[0].key(), "chainargos/agent-smith");
        // Selection re-anchors at index 0 of the filtered set.
        assert_eq!(s.list_state.selected, Some(0));
    }

    /// An empty filter shows every agent — equivalent to the initial
    /// state. Round-trip via Backspace must re-populate the list.
    #[test]
    fn filter_empty_shows_all() {
        let mut s = AgentPickerState::new(agents(&["agent-smith", "agent-brown"]));
        s.handle_key(key(KeyCode::Char('s')));
        // Only "agent-smith" contains 's'.
        assert_eq!(s.filtered.len(), 1);
        s.handle_key(key(KeyCode::Backspace));
        assert!(s.filter.is_empty());
        assert_eq!(s.filtered.len(), 2);
        assert_eq!(s.list_state.selected, Some(0));
    }

    /// Pressing Enter when the filter has narrowed the list to nothing
    /// is a no-op (no Commit, no Cancel) — the operator can keep typing
    /// or backspace out.
    #[test]
    fn enter_on_empty_filtered_list_is_noop() {
        let mut s = AgentPickerState::new(agents(&["agent-smith"]));
        for ch in "zzzz".chars() {
            s.handle_key(key(KeyCode::Char(ch)));
        }
        assert!(s.filtered.is_empty());
        let outcome = s.handle_key(key(KeyCode::Enter));
        assert!(matches!(outcome, ModalOutcome::Continue));
    }

    /// Down/Up wrap around the filtered list.
    #[test]
    fn down_wraps_at_end() {
        let mut s = AgentPickerState::new(agents(&["agent-a", "agent-b"]));
        s.handle_key(key(KeyCode::Down));
        s.handle_key(key(KeyCode::Down));
        assert_eq!(s.list_state.selected, Some(0));
    }

    #[test]
    fn up_wraps_at_start() {
        let mut s = AgentPickerState::new(agents(&["agent-a", "agent-b"]));
        s.handle_key(key(KeyCode::Up));
        assert_eq!(s.list_state.selected, Some(1));
    }

    /// Printable chars always append to the filter — including `j`/`k`,
    /// which are ambiguous between navigation and filter input. Pin
    /// that the filter wins so agents with those letters in their key
    /// can be typed naturally.
    #[test]
    fn j_and_k_append_to_filter_not_navigate() {
        let mut s = AgentPickerState::new(agents(&["agent-jenkins", "agent-kafka"]));
        s.handle_key(key(KeyCode::Char('j')));
        assert_eq!(s.filter, "j");
        assert_eq!(s.filtered.len(), 1);
        assert_eq!(s.filtered[0].key(), "agent-jenkins");
    }

    // ── Render-buffer smoke tests ─────────────────────────────────────
    //
    // These pin the canonical list-modal layout (`RULES.md` "TUI List
    // Modals"): persistent `Filter:` row, no `(no items match)`
    // placeholder, configurable Enter-verb in the footer.

    fn dump(state: &AgentPickerState, w: u16, h: u16) -> String {
        use ratatui::{Terminal, backend::TestBackend, layout::Rect};
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            let area = Rect::new(0, 0, w, h);
            super::render(f, area, state);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    /// Empty filter → row reads `Filter:` followed by placeholder dots
    /// (`░`). No filter text inlined in the title — the title bar is the
    /// bare `Select Agent` label.
    #[test]
    fn agent_picker_renders_filter_row_with_placeholder_dots_when_empty() {
        let s = AgentPickerState::new(agents(&["chainargos/agent-smith"]));
        let frame = dump(&s, 60, 12);
        assert!(
            frame.contains("Filter:"),
            "filter row label missing; frame:\n{frame}"
        );
        assert!(
            frame.contains('\u{2591}'),
            "filter row missing placeholder dots `░`; frame:\n{frame}"
        );
        // Title bar is the top border; pull just that row to check the
        // filter is NOT inlined into the title.
        let top: String = frame.lines().next().unwrap().to_string();
        assert!(
            top.contains("Select Agent"),
            "title bar must read `Select Agent`; top row:\n{top}"
        );
        assert!(
            !top.contains("filter:"),
            "filter must NOT be inlined into the title; top row:\n{top}"
        );
    }

    /// Typing a filter character → row shows the live characters in
    /// place of the placeholder dots, with a trailing cursor block.
    #[test]
    fn agent_picker_renders_filter_row_with_live_chars_when_typing() {
        let mut s = AgentPickerState::new(agents(&[
            "chainargos/agent-smith",
            "chainargos/agent-brown",
        ]));
        for ch in "smi".chars() {
            s.handle_key(key(KeyCode::Char(ch)));
        }
        let frame = dump(&s, 60, 12);
        assert!(
            frame.contains("Filter: smi"),
            "filter row must show live characters; frame:\n{frame}"
        );
        // The title bar still has no filter context.
        let top: String = frame.lines().next().unwrap().to_string();
        assert!(
            !top.contains("smi"),
            "live filter must NOT bleed into the title; top row:\n{top}"
        );
    }

    /// Footer's Enter-verb is the configured `confirm_label`. Two
    /// constructions, two verbs.
    #[test]
    fn agent_picker_footer_uses_configured_confirm_label() {
        let s_launch =
            AgentPickerState::with_confirm_label(agents(&["chainargos/agent-smith"]), "launch");
        let frame = dump(&s_launch, 60, 12);
        assert!(
            frame.contains("Enter") && frame.contains("launch"),
            "launch-context footer must read `Enter launch`; frame:\n{frame}"
        );
        assert!(
            !frame.contains(" select"),
            "launch-context footer must not say `select`; frame:\n{frame}"
        );

        let s_select =
            AgentPickerState::with_confirm_label(agents(&["chainargos/agent-smith"]), "select");
        let frame = dump(&s_select, 60, 12);
        assert!(
            frame.contains("Enter") && frame.contains("select"),
            "select-context footer must read `Enter select`; frame:\n{frame}"
        );
        assert!(
            !frame.contains(" launch"),
            "select-context footer must not say `launch`; frame:\n{frame}"
        );
    }

    /// Filter narrows visible set to nothing → blank space below the
    /// filter row, no `(no agents match)` placeholder.
    #[test]
    fn agent_picker_renders_no_empty_state_placeholder_when_filter_excludes_all() {
        let mut s = AgentPickerState::new(agents(&["agent-smith", "agent-brown"]));
        for ch in "zzzz".chars() {
            s.handle_key(key(KeyCode::Char(ch)));
        }
        assert!(s.filtered.is_empty());
        let frame = dump(&s, 60, 12);
        assert!(
            !frame.contains("(no agents match"),
            "must not render an empty-state placeholder; frame:\n{frame}"
        );
        assert!(
            !frame.contains("(no items match"),
            "must not render an empty-state placeholder; frame:\n{frame}"
        );
        // Sanity: the filter row still renders.
        assert!(frame.contains("Filter: zzzz"));
    }
}

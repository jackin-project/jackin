//! Listing + footer rendering for the file browser modal.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::git_prompt::render_git_prompt;
use super::state::FileBrowserState;
use super::{DANGER_RED, PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};

/// Vertical-layout constraints used by `render` and by the geometry-only
/// helpers consumed by the mouse-click hit-tester. Keep these in sync.
fn render_constraints(has_rejection: bool) -> Vec<ratatui::layout::Constraint> {
    use ratatui::layout::Constraint;
    if has_rejection {
        vec![
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ]
    } else {
        vec![Constraint::Min(3), Constraint::Length(1)]
    }
}

/// Rect of the listing area inside the modal.
///
/// This is the same chunk that `render` passes to `render_listing` and
/// anchors `render_git_prompt` on. Exposed so a mouse-handler can
/// recompute the git-prompt overlay geometry without needing `&mut`
/// access at render time.
pub fn listing_rect(modal_area: Rect, has_rejection: bool) -> Rect {
    use ratatui::layout::{Direction, Layout};
    let constraints = render_constraints(has_rejection);
    let listing_idx = usize::from(has_rejection);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(modal_area);
    chunks[listing_idx]
}

pub fn render(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    use ratatui::layout::{Alignment, Direction, Layout};

    frame.render_widget(ratatui::widgets::Clear, area);

    // Layout: [optional rejection banner][listing][nav hint].
    let has_rejection = state.rejected_reason.is_some();
    let constraints = render_constraints(has_rejection);
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
    let base_style = Style::default().fg(WHITE);
    let git_suffix_style = Style::default()
        .fg(PHOSPHOR_GREEN)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn make_state_at(path: PathBuf) -> FileBrowserState {
        FileBrowserState::new_at(path.clone(), path)
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

    // ── Entry name colour (WHITE) ─────────────────────────────────────

    /// Plain (non-git) directory entries render their name in WHITE so
    /// the listing stays legible against phosphor-green accents.
    #[test]
    fn non_git_entry_renders_in_white() {
        use ratatui::{Terminal, backend::TestBackend};

        let tmp = tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("plain")).unwrap();

        let state = make_state_at(tmp.path().to_path_buf());
        // Make sure nothing is selected so the highlight style doesn't
        // mask the base WHITE colour we want to assert on.
        let mut state = state;
        state.list_state.select(None);

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        // Locate the first cell of the name "plain" — rows start at y=0
        // with the block's top border, so the first entry sits at y=1
        // and the name begins at x = 1 (border) + 2 (indent) = 3.
        let cell = &buffer[(3, 1)];
        assert_eq!(
            cell.symbol(),
            "p",
            "expected 'p' at the entry's first char, got {:?}",
            cell.symbol()
        );
        assert_eq!(
            cell.fg,
            Color::Rgb(255, 255, 255),
            "non-git entry name should render in WHITE, got {:?}",
            cell.fg
        );
    }

    /// Git-repo entries render the name in WHITE and the ` (git)` suffix
    /// in PHOSPHOR_GREEN so the marker pops against the otherwise-white row.
    #[test]
    fn git_entry_name_is_white_and_suffix_is_phosphor_green() {
        use ratatui::{Terminal, backend::TestBackend};

        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();

        let mut state = make_state_at(tmp.path().to_path_buf());
        // Clear selection so the highlight style doesn't mask the spans.
        state.list_state.select(None);

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render(frame, frame.area(), &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        // First entry row is at y=1 (below the block's top border).
        // Name starts at x = 1 (border) + 2 (indent) = 3.
        let name_cell = &buffer[(3, 1)];
        assert_eq!(
            name_cell.symbol(),
            "r",
            "expected 'r' at name's first char, got {:?}",
            name_cell.symbol()
        );
        assert_eq!(
            name_cell.fg,
            Color::Rgb(255, 255, 255),
            "git entry name should render in WHITE, got {:?}",
            name_cell.fg
        );

        // Suffix: "  repo/ (git)" — the '(' of "(git)" sits at
        // x = 3 (name start) + 5 (len of "repo/") + 1 (space) = 9.
        let paren_cell = &buffer[(9, 1)];
        assert_eq!(
            paren_cell.symbol(),
            "(",
            "expected '(' at the suffix's first char, got {:?}",
            paren_cell.symbol()
        );
        assert_eq!(
            paren_cell.fg,
            Color::Rgb(0, 255, 65),
            "git suffix should render in PHOSPHOR_GREEN, got {:?}",
            paren_cell.fg
        );
    }
}

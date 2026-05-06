//! Render functions for the workspace manager TUI.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{ManagerListRow, ManagerStage, ManagerState};
use crate::config::AppConfig;

pub mod editor;
pub(super) mod list;
pub(super) mod modal;

// Re-export the shared modal geometry helper so `manager::input::mouse` can
// reach it via `super::super::render::modal_outer_rect`.
pub(super) use modal::modal_outer_rect;
// Re-export the editor entry point so input handlers can redraw the editor
// while a modal is being dismissed (see `input::mod`).
pub use editor::render_editor;

pub(super) const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
pub(super) const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
pub(super) const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
pub(super) const WHITE: Color = Color::Rgb(255, 255, 255);

// ── Footer item model ──────────────────────────────────────────────
//
// Structured footer items render with a consistent per-stage styling:
//   - Key(k):    WHITE + BOLD   — the literal hotkey glyph(s)
//   - Text(t):   PHOSPHOR_GREEN — the action label after a key
//   - Dyn(t):    PHOSPHOR_DIM   — free-form dynamic text (e.g. "3 changes")
//   - Sep:       PHOSPHOR_DARK  — single-dot separator between key+label pairs
//   - GroupSep:  (three spaces) — wider gap between logical groups
//
// Call sites build `Vec<FooterItem>` directly so the grouping is explicit,
// then hand it to `render_footer`. A convenience `footer_from_str` parser
// exists for legacy call sites that still own their string literal.

#[derive(Debug, Clone)]
pub(super) enum FooterItem {
    Key(&'static str),
    Text(&'static str),
    Dyn(String),
    Sep,
    GroupSep,
}

pub(super) fn footer_spans(items: &[FooterItem]) -> Vec<Span<'static>> {
    let key_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(PHOSPHOR_GREEN);
    let sep_style = Style::default().fg(PHOSPHOR_DARK);
    let dyn_style = Style::default().fg(PHOSPHOR_DIM);

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(items.len() * 2);
    for item in items {
        match item {
            FooterItem::Key(k) => {
                spans.push(Span::styled((*k).to_string(), key_style));
            }
            FooterItem::Text(t) => {
                spans.push(Span::styled(format!(" {t}"), text_style));
            }
            FooterItem::Dyn(t) => {
                spans.push(Span::styled(format!(" {t}"), dyn_style));
            }
            FooterItem::Sep => {
                spans.push(Span::styled(" \u{b7} ".to_string(), sep_style));
            }
            FooterItem::GroupSep => {
                spans.push(Span::raw("   "));
            }
        }
    }
    spans
}

pub(super) fn render_footer(frame: &mut Frame, area: Rect, items: &[FooterItem]) {
    let line = Line::from(footer_spans(items));
    let p = Paragraph::new(line).alignment(Alignment::Center);
    frame.render_widget(p, area);
}

#[allow(clippy::too_many_lines)]
pub fn render(
    frame: &mut Frame,
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    // Phase 1: render the base stage (Editor full-screen OR List chrome).
    if let ManagerStage::Editor(editor) = &state.stage {
        editor::render_editor(frame, editor, config, state.op_available);
    } else {
        // List / CreatePrelude / ConfirmDelete share the list-like chrome.
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Min(10),   // body
                Constraint::Length(2), // footer
            ])
            .split(area);

        render_header(frame, chunks[0], "workspaces");

        if matches!(&state.stage, ManagerStage::List) {
            list::render_list_body(frame, chunks[1], state, config, cwd);
        }

        let footer_items: Vec<FooterItem> = match &state.stage {
            ManagerStage::List => {
                // Surface "o open in GitHub" on rows whose workspace has at
                // least one GitHub-hosted mount with a resolvable web URL.
                // See `ManagerListRow` docs for row layout — current-dir and
                // the "+ New workspace" sentinel skip the hint entirely.
                let show_open_hint =
                    matches!(state.selected_row(), ManagerListRow::SavedWorkspace(_))
                        && state
                            .selected_workspace_summary()
                            .and_then(|s| config.workspaces.get(&s.name))
                            .is_some_and(|ws| {
                                !super::github_mounts::resolve_for_workspace(ws).is_empty()
                            });

                let mut items = vec![
                    // Navigation group
                    FooterItem::Key("\u{2191}\u{2193}"),
                    FooterItem::Sep,
                    FooterItem::Key("Enter"),
                    FooterItem::Text("launch"),
                    FooterItem::GroupSep,
                    // Per-row actions
                    FooterItem::Key("E"),
                    FooterItem::Text("edit"),
                    FooterItem::Sep,
                    FooterItem::Key("N"),
                    FooterItem::Text("new"),
                    FooterItem::Sep,
                    FooterItem::Key("D"),
                    FooterItem::Text("delete"),
                ];
                if show_open_hint {
                    items.push(FooterItem::Sep);
                    items.push(FooterItem::Key("O"));
                    items.push(FooterItem::Text("open in GitHub"));
                }
                items.push(FooterItem::GroupSep);
                // Exit
                items.push(FooterItem::Key("Q"));
                items.push(FooterItem::Text("quit"));
                items
            }
            ManagerStage::CreatePrelude(_) => vec![
                FooterItem::Dyn("Create workspace — follow the prompts".to_string()),
                FooterItem::GroupSep,
                FooterItem::Key("Esc"),
                FooterItem::Text("cancel"),
            ],
            ManagerStage::ConfirmDelete { .. } => vec![
                FooterItem::Key("Y"),
                FooterItem::Text("yes"),
                FooterItem::Sep,
                FooterItem::Key("N"),
                FooterItem::Text("no"),
                FooterItem::GroupSep,
                FooterItem::Key("Esc"),
                FooterItem::Text("cancel"),
            ],
            ManagerStage::Editor(_) => unreachable!("Editor has its own render path"),
        };
        render_footer(frame, chunks[2], &footer_items);
    }

    // Phase 2: overlay any active modal.
    //
    // The list-anchored modal lives on `ManagerState` itself rather
    // than on a stage variant, so its borrow has to be split off
    // separately from the stage-anchored modals to keep the borrow
    // checker happy with the shared `state` argument.
    let is_list_stage = matches!(state.stage, ManagerStage::List);
    if is_list_stage {
        if let Some(modal) = &mut state.list_modal {
            modal::render_modal(frame, modal);
        }
    } else {
        match &mut state.stage {
            ManagerStage::Editor(editor) => {
                if let Some(modal) = &mut editor.modal {
                    modal::render_modal(frame, modal);
                }
            }
            ManagerStage::CreatePrelude(prelude) => {
                if let Some(modal) = &mut prelude.modal {
                    modal::render_modal(frame, modal);
                }
            }
            ManagerStage::ConfirmDelete {
                state: confirm_state,
                ..
            } => {
                // ConfirmState is a top-level field on the variant, not wrapped
                // in Modal::Confirm, so render it directly.
                let area = frame.area();
                let modal_area = centered_rect_fixed(area, 60, 7);
                super::super::widgets::confirm::render(frame, modal_area, confirm_state);
            }
            ManagerStage::List => {
                // Handled above via the `is_list_stage` early branch.
            }
        }
    }
}

pub(super) fn render_header(frame: &mut Frame, area: Rect, title: &str) {
    let line = Line::from(vec![
        Span::styled("▓▓▓▓ ", Style::default().fg(PHOSPHOR_GREEN)),
        Span::styled(
            "jackin'",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::raw("     · "),
        Span::styled(title.to_string(), Style::default().fg(PHOSPHOR_DIM)),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Left), area);
}

/// Like `centered_rect` but takes a fixed number of rows for the height.
/// `pct_w` is still a percentage of the outer width. Rows are clamped to fit.
pub(super) fn centered_rect_fixed(outer: Rect, pct_w: u16, rows: u16) -> Rect {
    let w = outer.width * pct_w / 100;
    let h = rows.min(outer.height);
    Rect {
        x: outer.x + outer.width.saturating_sub(w) / 2,
        y: outer.y + outer.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

#[cfg(test)]
mod footer_tests {
    use super::{FOOTER_KEY, FOOTER_SEP, FOOTER_TEXT, FooterItem, footer_spans};

    // Sanity — the exported style colors match the palette.
    #[test]
    fn styling_colors_match_palette() {
        let key = FOOTER_KEY;
        let text = FOOTER_TEXT;
        let sep = FOOTER_SEP;
        assert_eq!(key.fg, Some(super::WHITE));
        assert_eq!(text.fg, Some(super::PHOSPHOR_GREEN));
        assert_eq!(sep.fg, Some(super::PHOSPHOR_DARK));
    }

    #[test]
    fn key_and_text_render_with_distinct_styles() {
        let items = vec![FooterItem::Key("Enter"), FooterItem::Text("launch")];
        let spans = footer_spans(&items);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content.as_ref(), "Enter");
        assert_eq!(spans[0].style.fg, Some(super::WHITE));
        assert_eq!(spans[1].content.as_ref(), " launch");
        assert_eq!(spans[1].style.fg, Some(super::PHOSPHOR_GREEN));
    }

    #[test]
    fn sep_renders_with_phosphor_dark() {
        let items = vec![
            FooterItem::Key("E"),
            FooterItem::Text("edit"),
            FooterItem::Sep,
            FooterItem::Key("N"),
            FooterItem::Text("new"),
        ];
        let spans = footer_spans(&items);
        // third item is the Sep
        assert_eq!(spans[2].content.as_ref(), " \u{b7} ");
        assert_eq!(spans[2].style.fg, Some(super::PHOSPHOR_DARK));
    }

    #[test]
    fn group_sep_renders_as_three_raw_spaces() {
        let items = vec![
            FooterItem::Key("Enter"),
            FooterItem::Text("launch"),
            FooterItem::GroupSep,
            FooterItem::Key("Q"),
            FooterItem::Text("quit"),
        ];
        let spans = footer_spans(&items);
        assert_eq!(spans[2].content.as_ref(), "   ");
        // GroupSep is styled with a plain ratatui::Style::default() — no fg set.
        assert_eq!(spans[2].style.fg, None);
    }

    #[test]
    fn dyn_item_uses_phosphor_dim() {
        let items = vec![FooterItem::Dyn("3 changes".to_string())];
        let spans = footer_spans(&items);
        assert_eq!(spans[0].content.as_ref(), " 3 changes");
        assert_eq!(spans[0].style.fg, Some(super::PHOSPHOR_DIM));
    }

    // Per-stage smoke tests — the List footer should have all six keys styled
    // as WHITE+BOLD and two GroupSep separators.
    #[test]
    fn list_footer_items_have_expected_structure() {
        let items: Vec<FooterItem> = vec![
            FooterItem::Key("\u{2191}\u{2193}"),
            FooterItem::Sep,
            FooterItem::Key("Enter"),
            FooterItem::Text("launch"),
            FooterItem::GroupSep,
            FooterItem::Key("E"),
            FooterItem::Text("edit"),
            FooterItem::Sep,
            FooterItem::Key("N"),
            FooterItem::Text("new"),
            FooterItem::Sep,
            FooterItem::Key("D"),
            FooterItem::Text("delete"),
            FooterItem::GroupSep,
            FooterItem::Key("Q"),
            FooterItem::Text("quit"),
        ];
        let spans = footer_spans(&items);
        // Every Key should be styled WHITE + BOLD; count them.
        let key_count = spans
            .iter()
            .filter(|s| s.style.fg == Some(super::WHITE))
            .count();
        assert_eq!(key_count, 6, "↑↓, Enter, E, N, D, Q");
        // Every Text should be styled PHOSPHOR_GREEN; count them.
        let text_count = spans
            .iter()
            .filter(|s| s.style.fg == Some(super::PHOSPHOR_GREEN))
            .count();
        assert_eq!(text_count, 5, "launch, edit, new, delete, quit");
        // GroupSep count (content == "   ", no fg).
        let group_sep_count = spans
            .iter()
            .filter(|s| s.content.as_ref() == "   " && s.style.fg.is_none())
            .count();
        assert_eq!(group_sep_count, 2, "nav | per-row | exit");
    }

    #[test]
    fn confirm_delete_footer_items_have_expected_structure() {
        let items: Vec<FooterItem> = vec![
            FooterItem::Key("Y"),
            FooterItem::Text("yes"),
            FooterItem::Sep,
            FooterItem::Key("N"),
            FooterItem::Text("no"),
            FooterItem::GroupSep,
            FooterItem::Key("Esc"),
            FooterItem::Text("cancel"),
        ];
        let spans = footer_spans(&items);
        let keys: Vec<&str> = spans
            .iter()
            .filter(|s| s.style.fg == Some(super::WHITE))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(keys, vec!["Y", "N", "Esc"]);
    }
}

// Re-export the per-item Styles used in tests so assertions don't need to
// recompute them from the palette.
#[cfg(test)]
const FOOTER_KEY: ratatui::style::Style = ratatui::style::Style::new()
    .fg(WHITE)
    .add_modifier(ratatui::style::Modifier::BOLD);
#[cfg(test)]
const FOOTER_TEXT: ratatui::style::Style = ratatui::style::Style::new().fg(PHOSPHOR_GREEN);
#[cfg(test)]
const FOOTER_SEP: ratatui::style::Style = ratatui::style::Style::new().fg(PHOSPHOR_DARK);

#[cfg(test)]
mod header_branding_tests {
    //! Pins the product-name rendering convention: the top-of-screen
    //! header must display the name as lowercase + trailing apostrophe
    //! (`jackin'`) in every user-facing string. All-caps `JACKIN` and
    //! apostrophe-less `jackin` are both disallowed for display text —
    //! though `jackin` without an apostrophe still appears in CLI-command
    //! references rendered in backticks (e.g. `` `jackin console` ``), in
    //! filesystem paths like `~/.jackin/`, and in URLs, all of which are
    //! intentionally exempt and not audited here.
    use super::render_header;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    #[test]
    fn tui_header_uses_lowercase_jackin_with_apostrophe() {
        let backend = TestBackend::new(40, 1);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_header(f, Rect::new(0, 0, 40, 1), "workspaces");
        })
        .unwrap();

        let buf = term.backend().buffer();
        let dump: String = buf
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect();

        assert!(
            dump.contains("jackin'"),
            "header must render 'jackin'' (lowercase + trailing apostrophe); got {dump:?}"
        );
        assert!(
            !dump.contains("JACKIN"),
            "header must not render 'JACKIN' (uppercase); got {dump:?}"
        );
    }
}

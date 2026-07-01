//! Terminal browser and SVG generator for the jackin-tui component lookbook.
//!
//! Usage:
//!   `tui-lookbook`                            — write SVGs to target/tui-lookbook/
//!   `tui-lookbook <out-dir>`                  — write SVGs to <out-dir>
//!   `tui-lookbook --check <dir>`              — verify SVGs are current
//!   `tui-lookbook --terminal`                 — launch interactive browser

mod interactors;
mod stories;
mod svg;

use std::{
    ffi::OsStr,
    io::{self, Stdout},
    path::PathBuf,
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use interactors::StoryInteraction;
use jackin_tui::{
    components::{
        KeyBinding, KeyChord, Keymap, LogicalKey, Visibility,
        hint_bar::render_hint_bar,
        panel::{Panel, PanelFocus, panel_body_area},
        render_brand_header,
        scrollable_panel::max_offset,
    },
    scroll::{self, ScrollSpan},
    theme::{PHOSPHOR_DARK, PHOSPHOR_GREEN, PREVIEW_CARD},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};
use stories::stories;
use svg::{check_svgs, write_story_svgs};

const USAGE: &str =
    "usage: tui-lookbook --terminal | tui-lookbook [out-dir] | tui-lookbook --check <dir>";
const CHECK_USAGE: &str = "usage: tui-lookbook --check <docs/public/tui-lookbook>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidebarAction {
    /// Up or Down (or j/k); direction resolved from the chord at the dispatch site.
    Navigate,
    /// Home or End; target (first/last) resolved from the chord at dispatch site.
    GoToEdge,
    FocusPreview,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewAction {
    BackToList,
    MovePreviewDown,
    MovePreviewUp,
    PageDown,
    PageUp,
    // Arrow keys and all other keys are forwarded to the active interactor.
    Forward,
}

static SIDEBAR_KEYMAP: Keymap<SidebarAction> = Keymap::new(&[
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Down),
            KeyChord::plain(LogicalKey::Up),
        ],
        action: SidebarAction::Navigate,
        hint: Some("navigate"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('j')),
            KeyChord::plain(LogicalKey::Char('k')),
        ],
        action: SidebarAction::Navigate,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Home),
            KeyChord::plain(LogicalKey::End),
        ],
        action: SidebarAction::GoToEdge,
        hint: Some("first/last"),
        visibility: Visibility::Shown,
        glyph: Some("Home/End"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Tab)],
        action: SidebarAction::FocusPreview,
        hint: Some("focus preview"),
        visibility: Visibility::Shown,
        glyph: Some("⇥"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Char('q')),
            KeyChord::plain(LogicalKey::Esc),
        ],
        action: SidebarAction::Quit,
        hint: Some("quit"),
        visibility: Visibility::Shown,
        glyph: Some("q/Esc"),
    },
]);

static PREVIEW_KEYMAP: Keymap<PreviewAction> = Keymap::new(&[
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Esc),
            KeyChord::plain(LogicalKey::Tab),
            KeyChord::plain(LogicalKey::BackTab),
        ],
        action: PreviewAction::BackToList,
        hint: Some("back to list"),
        visibility: Visibility::Shown,
        glyph: Some("Esc/⇥"),
    },
    KeyBinding {
        chords: &[
            KeyChord::plain(LogicalKey::Up),
            KeyChord::plain(LogicalKey::Down),
            KeyChord::plain(LogicalKey::Left),
            KeyChord::plain(LogicalKey::Right),
        ],
        action: PreviewAction::Forward,
        hint: Some("interact"),
        visibility: Visibility::Shown,
        glyph: Some("↑↓←→"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::PageDown)],
        action: PreviewAction::PageDown,
        hint: Some("page"),
        visibility: Visibility::Shown,
        glyph: Some("PgUp/PgDn"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::PageUp)],
        action: PreviewAction::PageUp,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('J'))],
        action: PreviewAction::MovePreviewDown,
        hint: Some("move preview"),
        visibility: Visibility::Shown,
        glyph: Some("J/K"),
    },
    KeyBinding {
        chords: &[KeyChord::plain(LogicalKey::Char('K'))],
        action: PreviewAction::MovePreviewUp,
        hint: None,
        visibility: Visibility::HiddenAlias,
        glyph: None,
    },
]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Sidebar,
    Preview,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let Some(first) = args.next() else {
        return write_svgs(PathBuf::from("target/tui-lookbook"));
    };

    if first == OsStr::new("--check") {
        let Some(dir) = args.next() else {
            return Err(CHECK_USAGE.into());
        };
        if args.next().is_some() {
            return Err(CHECK_USAGE.into());
        }
        return check_svgs(PathBuf::from(dir));
    }

    if first == OsStr::new("--terminal") {
        if args.next().is_some() {
            return Err("usage: tui-lookbook --terminal".into());
        }
        return run_terminal();
    }

    if args.next().is_some() {
        return Err(USAGE.into());
    }
    write_svgs(PathBuf::from(first))
}

#[allow(
    clippy::too_many_lines,
    reason = "Lookbook binary's terminal-driver fn: runs the story catalog loop \
              that mounts each story as the active pane and dispatches key \
              events. Single-binary entry point — the inline shape is the \
              canonical lookbook runner."
)]
#[allow(
    clippy::excessive_nesting,
    reason = "Same as too_many_lines: per-event / per-pane nested dispatch \
              through the catalog loop. Per-region helpers would obscure the \
              per-step readability of the catalog driver."
)]
fn run_terminal() -> Result<(), Box<dyn std::error::Error>> {
    let stories = stories();
    let mut terminal = TerminalGuard::enter()?;
    let mut selected = 0usize;
    let mut preview_scroll: u16 = 0;
    let mut sidebar_scroll: u16 = 0; // item-level scroll offset for the sidebar list
    let mut focus = Focus::Sidebar;
    let mut interactor: Box<dyn StoryInteraction> = stories[selected].make_interactor();
    // Rects updated after every draw for mouse hit-testing.
    let mut last_component_area = Rect::default();
    let mut last_preview_panel_area = Rect::default();
    let mut last_sidebar_area = Rect::default();
    let mut last_sidebar_viewport_items = 1usize;
    let mut last_preview_viewport_rows = 1usize;
    // Sidebar inner rect (inside the Panel border). Used to map click row
    // → story index (each story occupies 2 rows: component name + id).
    let mut last_sidebar_inner_area = Rect::default();

    loop {
        let story = stories[selected];
        let preview_content_rows = story.height as usize;

        terminal.draw(|frame| {
            let area = frame.area();

            // ── Global layout ─────────────────────────────────────────────────
            // brand(2) | main | spacer(1) | hint(1)
            let [brand_area, main_area, _spacer_area, hint_area] = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(area);

            // Full-width brand header on black background.
            render_brand_header(frame, brand_area, "lookbook");

            // Main: sidebar(30) | right
            let [sidebar_area, right_area] =
                Layout::horizontal([Constraint::Length(30), Constraint::Min(20)]).areas(main_area);

            // Right: description(fixed) | preview(rest)
            // Description height: 2 (title+component) + 1 (spacer) + 3 (desc wrapped) + 1 (spacer)
            let desc_height: u16 = 6;
            let [desc_area, preview_area] =
                Layout::vertical([Constraint::Length(desc_height), Constraint::Min(4)])
                    .areas(right_area);

            // ── Sidebar ───────────────────────────────────────────────────────
            let sidebar_panel_focus = if focus == Focus::Sidebar {
                PanelFocus::Focused
            } else {
                PanelFocus::Unfocused
            };
            let sidebar_block = Panel::new()
                .title(" Stories ")
                .focus(sidebar_panel_focus)
                .block();
            let sidebar_inner = sidebar_block.inner(sidebar_area);
            frame.render_widget(sidebar_block, sidebar_area);

            // Each story occupies 2 rows; compute the viewport in items.
            let sidebar_viewport_items = (usize::from(sidebar_inner.height) / 2).max(1);
            last_sidebar_viewport_items = sidebar_viewport_items;
            let total_stories = stories.len();
            // Cursor-follow: keep selected item visible.
            let eff_scroll = jackin_tui::components::cursor_follow_offset(
                selected,
                total_stories,
                sidebar_viewport_items,
                sidebar_scroll,
            );
            sidebar_scroll = eff_scroll;

            let items: Vec<ListItem<'_>> = stories
                .iter()
                .map(|s| {
                    ListItem::new(vec![
                        Line::from(Span::styled(s.component, jackin_tui::theme::BOLD_WHITE)),
                        Line::from(Span::styled(s.id, jackin_tui::theme::DIM)),
                    ])
                })
                .collect();
            let mut list_state = ListState::default()
                .with_offset(usize::from(eff_scroll))
                .with_selected(Some(selected));
            frame.render_stateful_widget(
                List::new(items)
                    .highlight_style(
                        Style::default()
                            .bg(PHOSPHOR_GREEN)
                            .fg(PHOSPHOR_DARK)
                            .add_modifier(Modifier::BOLD),
                    )
                    .highlight_symbol("▸ ")
                    .highlight_spacing(ratatui::widgets::HighlightSpacing::Always),
                sidebar_inner,
                &mut list_state,
            );
            // Vertical scrollbar: render in row units (2 rows per story).
            let sidebar_content_rows = total_stories * 2;
            let sidebar_viewport_rows = usize::from(sidebar_inner.height);
            if jackin_tui::components::is_scrollable(sidebar_content_rows, sidebar_viewport_rows) {
                jackin_tui::components::render_vertical_scrollbar(
                    frame,
                    sidebar_area,
                    sidebar_content_rows,
                    eff_scroll.saturating_mul(2),
                );
            }

            last_sidebar_area = sidebar_area;

            // ── Description block ─────────────────────────────────────────────
            let desc_block = Panel::new()
                .title(" About ")
                .focus(PanelFocus::Unfocused)
                .block();
            let desc_inner = panel_body_area(&desc_block, desc_area);
            frame.render_widget(desc_block, desc_area);

            let [title_row, spacer_row, desc_row] = Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .areas(desc_inner);
            let _ = spacer_row;

            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(story.title, jackin_tui::theme::BOLD_WHITE),
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        story.component,
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::DIM),
                    ),
                    Span::styled("  ", Style::default()),
                    Span::styled(story.id, jackin_tui::theme::DIM),
                ])),
                title_row,
            );
            frame.render_widget(
                Paragraph::new(story.description)
                    .style(jackin_tui::theme::BORDER)
                    .wrap(Wrap { trim: false }),
                desc_row,
            );

            // ── Preview block ─────────────────────────────────────────────────
            let preview_panel_focus = if focus == Focus::Preview {
                PanelFocus::Focused
            } else {
                PanelFocus::Unfocused
            };
            let preview_block = Panel::new()
                .title(" Preview ")
                .focus(preview_panel_focus)
                .block();
            let preview_inner = preview_block.inner(preview_area);
            frame.render_widget(preview_block, preview_area);

            // Fill the preview canvas with the PREVIEW_CARD charcoal so the
            // component floats on a distinct dark surface without the green
            // tint of PHOSPHOR_DARK.
            frame.render_widget(
                ratatui::widgets::Block::default().style(Style::default().bg(PREVIEW_CARD)),
                preview_inner,
            );

            // Apply 3-cell uniform padding so the component floats inside the
            // canvas with generous breathing room on all sides — makes the dark
            // preview background visible around every component.
            let canvas = preview_inner.inner(ratatui::layout::Margin {
                horizontal: 3,
                vertical: 3,
            });

            // Centre component both horizontally and vertically within the padded canvas.
            let vp_width = canvas.width;
            let vp_height = canvas.height;
            last_preview_viewport_rows = usize::from(vp_height);
            let content_width = story.width.min(vp_width);
            let content_height = story.height;

            let effective_scroll =
                preview_scroll.min(max_offset(content_height as usize, vp_height as usize));

            // Horizontal: centred within padded canvas.
            let cx = canvas.x + vp_width.saturating_sub(content_width) / 2;

            // Vertical: centred when content fits; scrollable when it doesn't.
            let cy = if content_height <= vp_height {
                canvas.y + vp_height.saturating_sub(content_height) / 2
            } else {
                canvas.y.saturating_sub(effective_scroll)
            };

            let clamped_height = if content_height <= vp_height {
                content_height
            } else {
                content_height
                    .saturating_sub(effective_scroll)
                    .min(vp_height)
            };

            let component_rect = Rect {
                x: cx,
                y: cy.max(canvas.y),
                width: content_width,
                height: clamped_height,
            };

            if component_rect.height > 0 && component_rect.width > 0 {
                // Clear the component area so every story renders on the
                // terminal's default background, with PREVIEW_CARD visible as
                // the dark surround. Dialog stories already call Clear
                // internally via render_dialog_shell; non-dialog stories need
                // this so they get the same visual treatment.
                frame.render_widget(ratatui::widgets::Clear, component_rect);
                interactor.render(frame, component_rect);
            }

            last_component_area = component_rect;
            last_preview_panel_area = preview_area;
            last_sidebar_inner_area = sidebar_inner;

            // ── Hint bar ──────────────────────────────────────────────────────
            let hint_spans = match focus {
                Focus::Preview => PREVIEW_KEYMAP.hint_spans(),
                Focus::Sidebar => SIDEBAR_KEYMAP.hint_spans(),
            };
            render_hint_bar(frame, hint_area, &hint_spans);
        })?;

        // event::poll returns false quickly when no event; avoids busy-loop.
        if !event::poll(Duration::from_millis(120))? {
            continue;
        }

        let _ = preview_content_rows; // used in scroll calls below
        match event::read()? {
            Event::Mouse(mouse) => {
                use crossterm::event::MouseEventKind;
                let col = mouse.column;
                let row = mouse.row;
                let over_sidebar = point_in_rect(col, row, last_sidebar_area);

                match mouse.kind {
                    MouseEventKind::Down(_) => {
                        // Click in sidebar: select the story at the clicked row and
                        // focus the sidebar. Each story occupies 2 rows in the list
                        // (component name line + id line). Per TUI design decisions:
                        // clicking a focusable container transfers focus immediately.
                        let s = last_sidebar_inner_area;
                        if col >= s.x && col < s.x + s.width && row >= s.y && row < s.y + s.height {
                            let row_in_inner = usize::from(row - s.y);
                            let clicked_idx = (usize::from(sidebar_scroll) + row_in_inner / 2)
                                .min(stories.len().saturating_sub(1));
                            if clicked_idx != selected {
                                preview_scroll = 0;
                                interactor = stories[clicked_idx].make_interactor();
                                selected = clicked_idx;
                            }
                            focus = Focus::Sidebar;
                        }

                        // Click in preview panel: transfer focus to preview so the
                        // component becomes keyboard-interactive.
                        let p = last_preview_panel_area;
                        if col >= p.x && col < p.x + p.width && row >= p.y && row < p.y + p.height {
                            focus = Focus::Preview;
                        }
                    }
                    // Mouse wheel over sidebar: scroll the story list and move
                    // selection with the viewport so render-time cursor-follow
                    // cannot snap the manual scroll back to the bottom.
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown if over_sidebar => {
                        let delta = if matches!(mouse.kind, MouseEventKind::ScrollUp) {
                            -1
                        } else {
                            1
                        };
                        let before = selected;
                        scroll::scroll_selectable_list(
                            &mut selected,
                            &mut sidebar_scroll,
                            stories.len(),
                            last_sidebar_viewport_items,
                            delta,
                        );
                        if selected != before {
                            preview_scroll = 0;
                            interactor = stories[selected].make_interactor();
                        }
                    }
                    MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
                    | MouseEventKind::ScrollLeft
                    | MouseEventKind::ScrollRight
                        if matches!(focus, Focus::Preview) =>
                    {
                        let axes = scroll::ScrollAxes {
                            vertical: scroll::is_scrollable(
                                preview_content_rows,
                                last_preview_viewport_rows,
                            ),
                            horizontal: false,
                        };
                        let mut ignored_scroll_x = 0;
                        scroll::apply_mouse_scroll_u16(
                            mouse.kind,
                            mouse.modifiers,
                            axes,
                            ScrollSpan::new(0, 0),
                            ScrollSpan::new(preview_content_rows, last_preview_viewport_rows),
                            &mut ignored_scroll_x,
                            &mut preview_scroll,
                        );
                    }
                    _ => {}
                }
                if point_in_rect(col, row, last_component_area) {
                    interactor.handle_mouse(mouse, last_component_area);
                }
            }
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                let chord = KeyChord::from(key);
                match focus {
                    Focus::Preview => {
                        match PREVIEW_KEYMAP
                            .dispatch(chord)
                            .unwrap_or(PreviewAction::Forward)
                        {
                            PreviewAction::BackToList => {
                                focus = Focus::Sidebar;
                            }
                            PreviewAction::MovePreviewDown => {
                                scroll::apply_delta_u16(
                                    preview_content_rows,
                                    last_preview_viewport_rows,
                                    &mut preview_scroll,
                                    1,
                                );
                            }
                            PreviewAction::MovePreviewUp => {
                                scroll::apply_delta_u16(
                                    preview_content_rows,
                                    last_preview_viewport_rows,
                                    &mut preview_scroll,
                                    -1,
                                );
                            }
                            PreviewAction::PageDown => {
                                scroll::apply_delta_u16(
                                    preview_content_rows,
                                    last_preview_viewport_rows,
                                    &mut preview_scroll,
                                    last_preview_viewport_rows as isize,
                                );
                            }
                            PreviewAction::PageUp => {
                                scroll::apply_delta_u16(
                                    preview_content_rows,
                                    last_preview_viewport_rows,
                                    &mut preview_scroll,
                                    -(last_preview_viewport_rows as isize),
                                );
                            }
                            PreviewAction::Forward => {
                                interactor.handle_key(key);
                            }
                        }
                    }
                    Focus::Sidebar => {
                        // Navigate and GoToEdge are directional: two chords share
                        // one action; direction resolved by inspecting chord.key.
                        use LogicalKey::{Char, Down, Home};
                        match SIDEBAR_KEYMAP.dispatch(chord) {
                            Some(SidebarAction::Quit) => break,
                            Some(SidebarAction::FocusPreview) => {
                                focus = Focus::Preview;
                            }
                            Some(SidebarAction::Navigate) => {
                                let down = matches!(chord.key, Down) || chord.key == Char('j');
                                let next = if down {
                                    (selected + 1).min(stories.len().saturating_sub(1))
                                } else {
                                    selected.saturating_sub(1)
                                };
                                if next != selected {
                                    preview_scroll = 0;
                                    interactor = stories[next].make_interactor();
                                }
                                selected = next;
                            }
                            Some(SidebarAction::GoToEdge) => {
                                let last = stories.len().saturating_sub(1);
                                let target = if matches!(chord.key, Home) { 0 } else { last };
                                if selected != target {
                                    interactor = stories[target].make_interactor();
                                }
                                selected = target;
                                preview_scroll = 0;
                            }
                            None => {}
                        }
                    }
                }
            }
            Event::Resize(_, _) => {
                // Ratatui handles resize automatically; just redraw.
            }
            _ => {}
        }
    }

    Ok(())
}

const fn point_in_rect(col: u16, row: u16, rect: Rect) -> bool {
    col >= rect.x
        && col < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self, Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, event::EnableMouseCapture) {
            drop(disable_raw_mode());
            return Err(error.into());
        }
        let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => terminal,
            Err(error) => {
                drop(disable_raw_mode());
                let _unused = execute!(
                    io::stdout(),
                    event::DisableMouseCapture,
                    LeaveAlternateScreen
                );
                return Err(error.into());
            }
        };
        Ok(Self { terminal })
    }

    fn draw<F>(&mut self, f: F) -> io::Result<()>
    where
        F: FnOnce(&mut ratatui::Frame<'_>),
    {
        self.terminal.draw(f).map(|_| ())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _unused = execute!(
            self.terminal.backend_mut(),
            event::DisableMouseCapture,
            LeaveAlternateScreen
        );
        drop(disable_raw_mode());
        drop(self.terminal.show_cursor());
    }
}

#[cfg(test)]
mod keymap_smoke;

#[allow(
    clippy::excessive_nesting,
    reason = "Lookbook binary's SVG writer: per-story + per-region nested loop. \
              Extracting per-region helpers would require threading mutable \
              state through fn calls and obscure the per-story rendering."
)]
fn write_svgs(out_dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    for path in write_story_svgs(&out_dir)? {
        let mut stdout = io::stdout().lock();
        drop(io::Write::write_fmt(
            &mut stdout,
            format_args!("{}\n", path.display()),
        ));
    }
    Ok(())
}

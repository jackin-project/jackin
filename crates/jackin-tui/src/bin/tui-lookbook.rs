use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs,
    io::{self, Stdout},
    path::{Path, PathBuf},
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use jackin_tui::lookbook::StoryInteraction;
use jackin_tui::{
    HintSpan,
    components::{
        hint_bar::render_hint_bar,
        panel::{Panel, PanelFocus},
        render_brand_header,
        scrollable_panel::{apply_scroll_delta, max_offset},
    },
    theme::{BORDER_GRAY, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
};

const USAGE: &str =
    "usage: tui-lookbook --terminal | tui-lookbook [out-dir] | tui-lookbook --check <dir>";
const CHECK_USAGE: &str = "usage: tui-lookbook --check <docs/public/tui-lookbook>";

const SIDEBAR_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::Sep,
    HintSpan::Key("⇥"),
    HintSpan::Text("focus preview"),
    HintSpan::Sep,
    HintSpan::Key("q/Esc"),
    HintSpan::Text("quit"),
];

const PREVIEW_FOCUS_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("Esc/⇥"),
    HintSpan::Text("back to list"),
    HintSpan::Sep,
    HintSpan::Key("↑↓←→"),
    HintSpan::Text("interact"),
    HintSpan::Sep,
    HintSpan::Key("J/K"),
    HintSpan::Text("scroll"),
];

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

fn run_terminal() -> Result<(), Box<dyn std::error::Error>> {
    let stories = jackin_tui::lookbook::stories();
    let mut terminal = TerminalGuard::enter()?;
    let mut selected = 0usize;
    let mut preview_scroll: u16 = 0;
    let mut focus = Focus::Sidebar;
    let mut interactor: Box<dyn StoryInteraction> = stories[selected].make_interactor();
    // Component rect updated after every draw for mouse hit-testing.
    let mut last_component_area = Rect::default();

    loop {
        let story = stories[selected];
        let preview_content_rows = story.height as usize;

        terminal.draw(|frame| {
            let area = frame.area();

            // Whole-screen black background.
            frame.render_widget(
                Block::default().style(Style::default().bg(Color::Black)),
                area,
            );

            // ── Global layout ─────────────────────────────────────────────────
            // brand(2) | main | hint(1)
            let [brand_area, main_area, hint_area] = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(1),
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
                .title(" stories ")
                .focus(sidebar_panel_focus)
                .block();
            let sidebar_inner = sidebar_block.inner(sidebar_area);
            frame.render_widget(sidebar_block, sidebar_area);

            let items: Vec<ListItem<'_>> = stories
                .iter()
                .map(|s| {
                    ListItem::new(vec![
                        Line::from(Span::styled(
                            s.component,
                            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                        )),
                        Line::from(Span::styled(s.id, Style::default().fg(PHOSPHOR_DIM))),
                    ])
                })
                .collect();
            let mut list_state = ListState::default().with_selected(Some(selected));
            frame.render_stateful_widget(
                List::new(items)
                    .highlight_style(
                        Style::default()
                            .bg(PHOSPHOR_GREEN)
                            .fg(PHOSPHOR_DARK)
                            .add_modifier(Modifier::BOLD),
                    )
                    .highlight_symbol("▸ "),
                sidebar_inner,
                &mut list_state,
            );

            // ── Description block ─────────────────────────────────────────────
            let desc_block = Panel::new()
                .title(" about ")
                .focus(PanelFocus::Unfocused)
                .block();
            let desc_inner = desc_block.inner(desc_area);
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
                    Span::styled(
                        story.title,
                        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        story.component,
                        Style::default()
                            .fg(PHOSPHOR_GREEN)
                            .add_modifier(Modifier::DIM),
                    ),
                    Span::styled("  ", Style::default()),
                    Span::styled(story.id, Style::default().fg(PHOSPHOR_DIM)),
                ])),
                title_row,
            );
            frame.render_widget(
                Paragraph::new(story.description)
                    .style(Style::default().fg(BORDER_GRAY))
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
                .title(" preview ")
                .focus(preview_panel_focus)
                .block();
            let preview_inner = preview_block.inner(preview_area);
            frame.render_widget(preview_block, preview_area);

            // Fill preview inner with black.
            frame.render_widget(
                Block::default().style(Style::default().bg(Color::Black)),
                preview_inner,
            );

            // Centre component horizontally, clip vertically.
            let vp_height = preview_inner.height;
            let content_height = story.height;
            let effective_scroll =
                preview_scroll.min(max_offset(content_height as usize, vp_height as usize));

            let cx = preview_inner.x + preview_inner.width.saturating_sub(story.width) / 2;
            let cy = preview_inner
                .y
                .saturating_sub(effective_scroll)
                .max(preview_inner.y);

            let clamped_height = content_height
                .saturating_sub(effective_scroll)
                .min(vp_height);

            let component_rect = Rect {
                x: cx,
                y: cy,
                width: story.width.min(preview_inner.width),
                height: clamped_height,
            };

            if component_rect.height > 0 && component_rect.width > 0 {
                interactor.render(frame, component_rect);
            }

            // Store for mouse hit-testing.
            // (Written into outer variable via closure; safe because draw is FnOnce.)
            let _ = component_rect; // captured below via last_component_area assignment
            last_component_area = component_rect;

            // ── Hint bar ──────────────────────────────────────────────────────
            let hint = match focus {
                Focus::Preview => PREVIEW_FOCUS_HINT,
                Focus::Sidebar => SIDEBAR_HINT,
            };
            render_hint_bar(frame, hint_area, hint);
        })?;

        // event::poll returns false quickly when no event; avoids busy-loop.
        if !event::poll(Duration::from_millis(120))? {
            continue;
        }

        let _ = preview_content_rows; // used in scroll calls below
        match event::read()? {
            Event::Mouse(mouse) => {
                interactor.handle_mouse(mouse, last_component_area);
            }
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match focus {
                    Focus::Preview => match key.code {
                        KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab => {
                            focus = Focus::Sidebar;
                        }
                        // J/K scroll the preview when in preview focus.
                        KeyCode::Char('J') => {
                            apply_scroll_delta(&mut preview_scroll, 1, 10, preview_content_rows);
                        }
                        KeyCode::Char('K') => {
                            apply_scroll_delta(&mut preview_scroll, -1, 10, preview_content_rows);
                        }
                        KeyCode::PageDown => {
                            apply_scroll_delta(&mut preview_scroll, 10, 10, preview_content_rows);
                        }
                        KeyCode::PageUp => {
                            apply_scroll_delta(&mut preview_scroll, -10, 10, preview_content_rows);
                        }
                        _ => {
                            interactor.handle_key(key);
                        }
                    },
                    Focus::Sidebar => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Tab => {
                            focus = Focus::Preview;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            let next = (selected + 1).min(stories.len().saturating_sub(1));
                            if next != selected {
                                preview_scroll = 0;
                                interactor = stories[next].make_interactor();
                            }
                            selected = next;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            let next = selected.saturating_sub(1);
                            if next != selected {
                                preview_scroll = 0;
                                interactor = stories[next].make_interactor();
                            }
                            selected = next;
                        }
                        KeyCode::Home => {
                            if selected != 0 {
                                interactor = stories[0].make_interactor();
                            }
                            selected = 0;
                            preview_scroll = 0;
                        }
                        KeyCode::End => {
                            let last = stories.len().saturating_sub(1);
                            if selected != last {
                                interactor = stories[last].make_interactor();
                            }
                            selected = last;
                            preview_scroll = 0;
                        }
                        _ => {}
                    },
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

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self, Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(
            stdout,
            EnterAlternateScreen,
            crossterm::event::EnableMouseCapture
        ) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = disable_raw_mode();
                let _ = execute!(
                    io::stdout(),
                    crossterm::event::DisableMouseCapture,
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
        let _ = execute!(
            self.terminal.backend_mut(),
            crossterm::event::DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
        let _ = self.terminal.show_cursor();
    }
}

fn write_svgs(out_dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    for path in jackin_tui::lookbook::write_story_svgs(&out_dir)? {
        println!("{}", path.display());
    }
    Ok(())
}

fn check_svgs(dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let expected = expected_svg_names();
    let actual = actual_svg_names(&dir)?;
    let mut failures = Vec::new();

    for missing in expected.difference(&actual) {
        failures.push(format!("missing generated preview: {missing}"));
    }
    for stale in actual.difference(&expected) {
        failures.push(format!("stale generated preview: {stale}"));
    }

    for story in jackin_tui::lookbook::stories() {
        let filename = jackin_tui::lookbook::story_svg_filename(story);
        let path = dir.join(&filename);
        if !path.exists() {
            continue;
        }
        let committed = fs::read_to_string(&path)?;
        let rendered = jackin_tui::lookbook::render_story_to_svg(story);
        if committed != rendered {
            failures.push(format!("generated preview is stale: {}", path.display()));
        }
    }

    if failures.is_empty() {
        println!("tui lookbook previews are current");
        Ok(())
    } else {
        for failure in &failures {
            eprintln!("{failure}");
        }
        Err(concat!(
            "tui lookbook previews are out of date; regenerate with ",
            "`cargo run -p jackin-tui --bin tui-lookbook -- docs/public/tui-lookbook`",
        )
        .into())
    }
}

fn expected_svg_names() -> BTreeSet<String> {
    jackin_tui::lookbook::stories()
        .into_iter()
        .map(jackin_tui::lookbook::story_svg_filename)
        .collect()
}

fn actual_svg_names(dir: &Path) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension() != Some(OsStr::new("svg")) {
            continue;
        }
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            return Err(format!("non-UTF-8 lookbook preview path: {}", path.display()).into());
        };
        names.insert(name.to_owned());
    }
    Ok(names)
}

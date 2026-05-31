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
use jackin_tui::{
    HintSpan,
    components::{
        hint_bar::render_hint_bar,
        panel::{Panel, PanelFocus},
        render_brand_header,
        scrollable_panel::{apply_scroll_delta, is_scrollable, max_offset, viewport_height},
    },
    theme::{BORDER_GRAY, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

const USAGE: &str =
    "usage: tui-lookbook --terminal | tui-lookbook [out-dir] | tui-lookbook --check <dir>";
const CHECK_USAGE: &str = "usage: tui-lookbook --check <docs/public/tui-lookbook>";

const SIDEBAR_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::Sep,
    HintSpan::Key("q/Esc"),
    HintSpan::Text("quit"),
];

const PREVIEW_SCROLL_HINT: &[HintSpan<'static>] = &[
    HintSpan::Key("↑↓"),
    HintSpan::Text("navigate"),
    HintSpan::Sep,
    HintSpan::Key("⇧↑↓"),
    HintSpan::Text("scroll preview"),
    HintSpan::Sep,
    HintSpan::Key("q/Esc"),
    HintSpan::Text("quit"),
];

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

    loop {
        let story = stories[selected];
        let preview_content_rows = story.height as usize;

        terminal.draw(|frame| {
            let area = frame.area();
            // Fill entire screen with the console background colour.
            frame.render_widget(
                Block::default().style(Style::default().bg(PHOSPHOR_DARK)),
                area,
            );
            frame.render_widget(Clear, area);
            frame.render_widget(
                Block::default().style(Style::default().bg(PHOSPHOR_DARK)),
                area,
            );

            // Reserve bottom row for the hint bar.
            let [main_area, hint_area] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);

            let [sidebar_area, preview_area] =
                Layout::horizontal([Constraint::Length(34), Constraint::Min(32)]).areas(main_area);

            // ── Sidebar ──────────────────────────────────────────────────────
            // Brand header occupies top 2 rows of sidebar (same as console).
            let [brand_area, list_area] =
                Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(sidebar_area);

            render_brand_header(frame, brand_area, "lookbook");

            let sidebar_block = Panel::new()
                .title(" stories ")
                .focus(PanelFocus::Unfocused)
                .block();
            let sidebar_inner = sidebar_block.inner(list_area);
            frame.render_widget(sidebar_block, list_area);

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

            // ── Preview panel ─────────────────────────────────────────────────
            let preview_vp = viewport_height(preview_area).saturating_sub(6); // minus header rows
            let scrollable = is_scrollable(preview_content_rows, preview_vp);
            let hint = if scrollable {
                PREVIEW_SCROLL_HINT
            } else {
                SIDEBAR_HINT
            };

            render_story_preview(frame, preview_area, story, preview_scroll);

            // ── Hint bar ──────────────────────────────────────────────────────
            render_hint_bar(frame, hint_area, hint);
        })?;

        if event::poll(Duration::from_millis(120))?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Down | KeyCode::Char('j') => {
                    let next = (selected + 1).min(stories.len().saturating_sub(1));
                    if next != selected {
                        preview_scroll = 0;
                    }
                    selected = next;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let next = selected.saturating_sub(1);
                    if next != selected {
                        preview_scroll = 0;
                    }
                    selected = next;
                }
                KeyCode::Home => {
                    selected = 0;
                    preview_scroll = 0;
                }
                KeyCode::End => {
                    selected = stories.len().saturating_sub(1);
                    preview_scroll = 0;
                }
                // Shift+Down / Shift+Up scroll the preview pane.
                KeyCode::Char('J') => {
                    let vp = 10usize; // approximate; recalculated per frame above
                    apply_scroll_delta(&mut preview_scroll, 1, vp, preview_content_rows);
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
                _ => {}
            }
        }
    }

    Ok(())
}

fn render_story_preview(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    story: jackin_tui::lookbook::Story,
    scroll: u16,
) {
    let block = Panel::new()
        .title(" preview ")
        .focus(PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // title + id
        Constraint::Length(1), // spacer
        Constraint::Length(2), // description
        Constraint::Length(1), // spacer
        Constraint::Min(1),    // component preview
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                story.title,
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(story.id, Style::default().fg(PHOSPHOR_DIM)),
        ])),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(story.description)
            .style(Style::default().fg(BORDER_GRAY))
            .wrap(Wrap { trim: false }),
        rows[2],
    );

    let preview_area = rows[4];
    frame.render_widget(
        Block::default().style(Style::default().bg(PHOSPHOR_DARK)),
        preview_area,
    );

    // Centre the component horizontally; clip vertically by scroll offset.
    let content_height = story.height;
    let vp_height = preview_area.height;
    let effective_scroll = scroll.min(max_offset(content_height as usize, vp_height as usize));

    let x = preview_area.x + preview_area.width.saturating_sub(story.width) / 2;
    let visible_start = effective_scroll;
    let visible_end = effective_scroll + vp_height;

    // Render story into a scratch rect and clip the visible rows into the frame.
    let scratch = Rect {
        x,
        y: preview_area.y.saturating_sub(visible_start),
        width: story.width.min(preview_area.width),
        height: content_height,
    };

    // Clip rendering to the preview viewport.
    let clip = Rect {
        x: preview_area.x,
        y: preview_area.y,
        width: preview_area.width,
        height: vp_height,
    };
    // Ratatui clips widgets to the frame area automatically when we render
    // with the scratch rect positioned above/within the clip area.
    let _ = clip; // viewport clipping is inherent via frame.area()
    let render_rect = Rect {
        x,
        y: preview_area.y.saturating_sub(visible_start),
        width: story.width.min(preview_area.width),
        height: content_height,
    };
    // Only render if at least part of the content is visible.
    if render_rect.y < preview_area.y + vp_height && visible_start < content_height {
        let clamped = Rect {
            x: render_rect.x,
            y: render_rect.y.max(preview_area.y),
            width: render_rect.width,
            height: render_rect
                .height
                .min(vp_height.saturating_sub(render_rect.y.saturating_sub(preview_area.y))),
        };
        story.render(frame, clamped);
    }
    let _ = scratch;
    let _ = visible_end;
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self, Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
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
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
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

use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs,
    io::{self, Stdout},
    path::{Path, PathBuf},
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

const USAGE: &str =
    "usage: tui-lookbook --terminal | tui-lookbook [out-dir] | tui-lookbook --check <dir>";
const CHECK_USAGE: &str = "usage: tui-lookbook --check <docs/public/tui-lookbook>";

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

    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            frame.render_widget(Clear, area);
            frame.render_widget(
                Block::default().style(Style::default().bg(Color::Black)),
                area,
            );

            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(34), Constraint::Min(32)])
                .split(area);

            let items: Vec<ListItem<'_>> = stories
                .iter()
                .map(|story| {
                    ListItem::new(vec![
                        Line::from(Span::styled(
                            story.component,
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        )),
                        Line::from(Span::styled(
                            story.id,
                            Style::default().fg(Color::Rgb(0, 140, 30)),
                        )),
                    ])
                })
                .collect();
            let mut list_state = ListState::default().with_selected(Some(selected));
            frame.render_stateful_widget(
                List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" jackin-tui lookbook ")
                            .style(Style::default().bg(Color::Black).fg(Color::White)),
                    )
                    .highlight_style(
                        Style::default()
                            .bg(Color::Rgb(0, 255, 65))
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD),
                    )
                    .highlight_symbol("▸ "),
                cols[0],
                &mut list_state,
            );

            render_story_preview(frame, cols[1], stories[selected]);
        })?;

        if event::poll(Duration::from_millis(120))? {
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = (selected + 1).min(stories.len().saturating_sub(1));
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = selected.saturating_sub(1);
                    }
                    KeyCode::Home => selected = 0,
                    KeyCode::End => selected = stories.len().saturating_sub(1),
                    _ => {}
                },
                Event::Resize(_, _) => {}
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
) {
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(" preview ")
            .style(Style::default().bg(Color::Black).fg(Color::White)),
        area,
    );
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                story.title,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(story.id, Style::default().fg(Color::Rgb(0, 140, 30))),
        ])),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(story.description)
            .style(Style::default().fg(Color::Rgb(0, 255, 65)))
            .wrap(Wrap { trim: false }),
        rows[2],
    );

    let max = rows[4];
    let preview = Rect {
        x: max.x,
        y: max.y,
        width: story.width.min(max.width),
        height: story.height.min(max.height),
    };
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        max,
    );
    story.render(frame, preview);
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

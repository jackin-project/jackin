// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU16;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Paragraph};
use tokio::sync::mpsc;

use crate::keys;
use crate::term::{self, TermScreen};
use crate::transport::{PaneCmd, PaneEvent, Remote, spawn_main, spawn_shell};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaneId {
    Left,
    Right,
}

impl PaneId {
    pub const ALL: [Self; 2] = [Self::Left, Self::Right];

    fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

pub enum AppEvent {
    Term(Event),
    Pane {
        pane: PaneId,
        generation: u64,
        event: PaneEvent,
    },
}

enum Link {
    Detached,
    Attaching(mpsc::UnboundedSender<PaneCmd>),
    Attached(mpsc::UnboundedSender<PaneCmd>),
    Ended(String),
}

pub struct Pane {
    id: PaneId,
    parser: vt100::Parser,
    inner: (u16, u16),
    link: Link,
    generation: u64,
    attaches: u32,
    rx_bytes: usize,
    notice: String,
    exit: Option<i32>,
}

/// Below this inner size the pane keeps its last remote size and the widget truncates.
const MIN_COLS: u16 = 20;
const MIN_ROWS: u16 = 5;

/// Where a replay that starts mid-stream becomes safe to paint: the first CR, LF or CUP
/// (`ESC [ row ; col H`). Everything before it belongs to a sequence or a row that was cut.
fn resync_point(bytes: &[u8]) -> usize {
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'\r' | b'\n' => return index,
            0x1b if bytes.get(index + 1) == Some(&b'[') => {
                let params = bytes[index + 2..]
                    .iter()
                    .take_while(|b| b.is_ascii_digit() || **b == b';')
                    .count();
                if matches!(bytes.get(index + 2 + params), Some(b'H' | b'f')) {
                    return index;
                }
            }
            _ => {}
        }
    }
    bytes.len()
}

fn nz(value: u16) -> NonZeroU16 {
    NonZeroU16::new(value).unwrap_or(NonZeroU16::MIN)
}

impl Pane {
    fn new(id: PaneId, inner: (u16, u16)) -> Self {
        Self {
            id,
            parser: vt100::Parser::new(nz(inner.0), nz(inner.1), 1000),
            inner,
            link: Link::Detached,
            generation: 0,
            attaches: 0,
            rx_bytes: 0,
            notice: String::new(),
            exit: None,
        }
    }

    fn sender(&self) -> Option<&mpsc::UnboundedSender<PaneCmd>> {
        match &self.link {
            Link::Attaching(tx) | Link::Attached(tx) => Some(tx),
            Link::Detached | Link::Ended(_) => None,
        }
    }

    fn status(&self) -> String {
        match &self.link {
            Link::Detached => "detached".into(),
            Link::Attaching(_) => format!("attaching #{}", self.attaches),
            Link::Attached(_) => format!("attached #{}", self.attaches),
            Link::Ended(reason) => format!("ended: {reason}"),
        }
    }
}

pub struct Config {
    pub color: bool,
    pub nudge: bool,
    pub resync: bool,
}

pub struct App {
    pub panes: [Pane; 2],
    pub focus: PaneId,
    pub quit: bool,
    term: (u16, u16),
    remote: Remote,
    events: mpsc::UnboundedSender<AppEvent>,
    config: Config,
    pub log: Vec<String>,
}

/// Body areas of the two panes and the footer, from the terminal size. Used by render and by
/// the size sync in update, so both agree.
fn layout(area: Rect) -> ([Rect; 2], Rect) {
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    let panes = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .areas::<2>(body);
    (panes, footer)
}

fn block(title: Line<'_>, focused: bool) -> Block<'_> {
    let style = if focused {
        Style::new().fg(Color::Yellow)
    } else {
        Style::new().fg(Color::DarkGray)
    };
    Block::bordered()
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .border_style(style)
        .title(title)
}

fn human(bytes: usize) -> String {
    if bytes >= 1_000_000 {
        format!("{:.2}MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1000 {
        format!("{:.1}kB", bytes as f64 / 1000.0)
    } else {
        format!("{bytes}B")
    }
}

impl App {
    pub fn new(
        term: (u16, u16),
        remote: Remote,
        events: mpsc::UnboundedSender<AppEvent>,
        config: Config,
    ) -> Self {
        let (areas, _) = layout(Rect::new(0, 0, term.0, term.1));
        let inner = |area: Rect| {
            let inner = Block::bordered().inner(area);
            (inner.height.max(MIN_ROWS), inner.width.max(MIN_COLS))
        };
        Self {
            panes: [
                Pane::new(PaneId::Left, inner(areas[0])),
                Pane::new(PaneId::Right, inner(areas[1])),
            ],
            focus: PaneId::Left,
            quit: false,
            term,
            remote,
            events,
            config,
            log: Vec::new(),
        }
    }

    fn note(&mut self, line: String) {
        self.log.push(line);
    }

    pub fn attach(&mut self, id: PaneId) {
        let remote = self.remote.clone();
        let nudge = self.config.nudge;
        let events = self.events.clone();
        let pane = &mut self.panes[id.index()];
        pane.generation += 1;
        pane.attaches += 1;
        pane.rx_bytes = 0;
        pane.notice.clear();
        pane.exit = None;
        pane.parser = vt100::Parser::new(nz(pane.inner.0), nz(pane.inner.1), 1000);
        let generation = pane.generation;
        let (rows, cols) = pane.inner;
        let (pane_tx, mut pane_rx) = mpsc::unbounded_channel();
        let cmd_tx = match id {
            PaneId::Left => spawn_main(remote, cols, rows, nudge, pane_tx),
            PaneId::Right => spawn_shell(remote, cols, rows, pane_tx),
        };
        tokio::spawn(async move {
            while let Some(event) = pane_rx.recv().await {
                if events
                    .send(AppEvent::Pane {
                        pane: id,
                        generation,
                        event,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        pane.link = Link::Attaching(cmd_tx);
        let attach = pane.attaches;
        self.note(format!(
            "{} attach #{attach} generation={generation} pty={cols}x{rows} fresh parser",
            id.name()
        ));
    }

    pub fn detach(&mut self, id: PaneId) {
        let pane = &mut self.panes[id.index()];
        if let Some(tx) = pane.sender() {
            let _ = tx.send(PaneCmd::Detach);
        }
        pane.link = Link::Detached;
        let total = pane.rx_bytes;
        self.note(format!(
            "{} detach: channel eof+close, {total} bytes received in this attachment",
            id.name()
        ));
    }

    pub fn update(&mut self, event: AppEvent) {
        match event {
            AppEvent::Term(Event::Key(key)) if key.kind != KeyEventKind::Release => self.key(key),
            AppEvent::Term(Event::Resize(cols, rows)) => {
                self.term = (cols, rows);
                self.sync_sizes();
            }
            AppEvent::Term(_) => {}
            AppEvent::Pane {
                pane,
                generation,
                event,
            } => self.pane_event(pane, generation, event),
        }
    }

    fn key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('w') => {
                    self.focus = match self.focus {
                        PaneId::Left => PaneId::Right,
                        PaneId::Right => PaneId::Left,
                    };
                    self.note(format!("focus -> {}", self.focus.name()));
                    return;
                }
                KeyCode::Char('q') => {
                    self.quit = true;
                    return;
                }
                KeyCode::Char('x') if self.focus == PaneId::Left => {
                    if self.panes[0].sender().is_some() {
                        self.detach(PaneId::Left);
                    } else {
                        self.attach(PaneId::Left);
                    }
                    return;
                }
                _ => {}
            }
        }
        let id = self.focus;
        let pane = &self.panes[id.index()];
        let Link::Attached(tx) = &pane.link else {
            return;
        };
        let app_cursor = pane.parser.screen().application_cursor();
        if let Some(bytes) = keys::encode(key, app_cursor) {
            let shown = bytes.escape_ascii().to_string();
            let _ = tx.send(PaneCmd::Input(bytes));
            self.note(format!("{} key -> remote bytes {shown}", id.name()));
        }
    }

    fn pane_event(&mut self, id: PaneId, generation: u64, event: PaneEvent) {
        let pane = &mut self.panes[id.index()];
        if generation != pane.generation {
            return;
        }
        match event {
            PaneEvent::Attached => {
                if let Link::Attaching(tx) = std::mem::replace(&mut pane.link, Link::Detached) {
                    pane.link = Link::Attached(tx);
                }
                self.note(format!("{} attached", id.name()));
            }
            PaneEvent::Output(bytes) => {
                let first = pane.rx_bytes == 0;
                pane.rx_bytes += bytes.len();
                let mut feed = &bytes[..];
                let mut dropped = 0;
                if first
                    && self.config.resync
                    && !matches!(feed.first(), Some(0x1b | b'\r' | b'\n'))
                {
                    dropped = resync_point(feed);
                    feed = &feed[dropped..];
                }
                pane.parser.process(feed);
                if first {
                    let head = bytes
                        .iter()
                        .copied()
                        .take(48)
                        .collect::<Vec<u8>>()
                        .escape_ascii()
                        .to_string();
                    self.note(format!(
                        "{} first chunk {} bytes, head={head}",
                        id.name(),
                        bytes.len()
                    ));
                    if self.config.resync {
                        self.note(format!(
                            "{} resync: dropped {dropped} leading bytes before the first CR/LF/CUP",
                            id.name()
                        ));
                    }
                }
            }
            PaneEvent::Notice(text) => {
                self.note(format!("{} notice: {text}", id.name()));
                self.panes[id.index()].notice = text;
            }
            PaneEvent::Exited(code) => {
                pane.exit = code;
                self.note(format!("{} remote exited code={code:?}", id.name()));
            }
            PaneEvent::Ended(reason) => {
                if matches!(pane.link, Link::Detached) {
                    return;
                }
                pane.link = Link::Ended(reason.clone());
                self.note(format!("{} ended: {reason}", id.name()));
            }
        }
    }

    /// Resize each parser and the remote PTY to the pane's inner area after a terminal resize.
    pub fn sync_sizes(&mut self) {
        let (areas, _) = layout(Rect::new(0, 0, self.term.0, self.term.1));
        for id in PaneId::ALL {
            let inner = Block::bordered().inner(areas[id.index()]);
            let size = (inner.height, inner.width);
            let pane = &mut self.panes[id.index()];
            if pane.inner == size {
                continue;
            }
            if size.0 < MIN_ROWS || size.1 < MIN_COLS {
                if self
                    .log
                    .last()
                    .is_none_or(|last| !last.contains("below floor"))
                {
                    self.log.push(format!(
                        "{} resize inner {}x{} below floor {MIN_COLS}x{MIN_ROWS}: remote keeps {}x{}, widget truncates",
                        id.name(), size.1, size.0, pane.inner.1, pane.inner.0
                    ));
                }
                continue;
            }
            pane.inner = size;
            pane.parser.set_size(nz(size.0), nz(size.1));
            let sent = if let Some(tx) = pane.sender() {
                let _ = tx.send(PaneCmd::Resize {
                    cols: size.1,
                    rows: size.0,
                });
                "sent"
            } else {
                "pane not attached, applies at next attach"
            };
            self.note(format!(
                "{} resize inner {}x{} -> window_change/Resize cols={} rows={} ({sent})",
                id.name(),
                size.1,
                size.0,
                size.1,
                size.0
            ));
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        let (areas, footer) = layout(frame.area());
        for id in PaneId::ALL {
            let pane = &self.panes[id.index()];
            let focused = self.focus == id;
            let kind = match id {
                PaneId::Left => "main",
                PaneId::Right => "shell",
            };
            let title = Line::from(format!(
                " {} {kind} [{} | {}x{} | rx {}] ",
                id.name(),
                pane.status(),
                pane.inner.1,
                pane.inner.0,
                human(pane.rx_bytes),
            ));
            let block = block(title, focused);
            let inner = block.inner(areas[id.index()]);
            frame.render_widget(block, areas[id.index()]);
            frame.render_widget(
                TermScreen {
                    screen: pane.parser.screen(),
                    color: self.config.color,
                },
                inner,
            );
            if focused
                && matches!(pane.link, Link::Attached(_))
                && let Some(position) = term::cursor(pane.parser.screen(), inner)
            {
                frame.set_cursor_position(position);
            }
        }
        let hint = format!(
            " ^W focus ({})  ^X detach/attach left  ^Q quit {}",
            self.focus.name(),
            self.panes
                .iter()
                .filter(|p| !p.notice.is_empty())
                .map(|p| format!("| {}: {}", p.id.name(), p.notice))
                .collect::<String>()
        );
        frame.render_widget(
            Paragraph::new(hint).style(Style::new().add_modifier(Modifier::REVERSED)),
            footer,
        );
    }
}

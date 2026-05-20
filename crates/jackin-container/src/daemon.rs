/// The multiplexer daemon — runs as PID 1, manages sessions and clients.
///
/// Architecture:
///   - One active client at a time (the operator's exec'd terminal)
///   - Client handler task: reads ClientMsg → sends to cmd_tx; writes
///     outbound bytes from out_rx → socket
///   - Main loop: selects on PTY events, cmd_rx, and periodic state ticker
use std::collections::HashMap;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use crate::dialog::{Dialog, DialogAction, PaletteCommand};
use crate::input::{ArrowDir, InputEvent, parse};
use crate::layout::{Direction, Rect, Tab};
use crate::protocol::{
    AgentState, ClientMsg, ServerMsg, SessionInfo, b64_decode, b64_encode, frame,
};
use crate::session::{
    Session, SessionEvent, available_agents, build_agent_command, build_shell_command,
};
use crate::socket;
use crate::statusbar::{StatusBar, draw_horizontal_border, draw_vertical_border};

pub struct Multiplexer {
    sessions: HashMap<u64, Session>,
    tabs: Vec<Tab>,
    active_tab: usize,
    term_rows: u16,
    term_cols: u16,
    status_bar: StatusBar,
    dialog: Option<Dialog>,
    content_rows: u16,
    available_agents: Vec<String>,
    env_passthrough: Vec<(String, String)>,
    event_tx: mpsc::UnboundedSender<SessionEvent>,
    event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    zoomed: Option<u64>,
}

impl Multiplexer {
    pub fn new(rows: u16, cols: u16) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let content_rows = rows.saturating_sub(1);
        let agents = available_agents();

        let env_passthrough: Vec<(String, String)> = [
            "GIT_AUTHOR_NAME",
            "GIT_AUTHOR_EMAIL",
            "GH_TOKEN",
            "JACKIN_DEBUG",
            "JACKIN_GIT_COAUTHOR_TRAILER",
            "JACKIN_GIT_DCO",
        ]
        .iter()
        .filter_map(|&k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect();

        Self {
            sessions: HashMap::new(),
            tabs: Vec::new(),
            active_tab: 0,
            term_rows: rows,
            term_cols: cols,
            status_bar: StatusBar::new(),
            dialog: None,
            content_rows,
            available_agents: agents,
            env_passthrough,
            event_tx,
            event_rx,
            zoomed: None,
        }
    }

    pub fn spawn_initial(&mut self, agent: &str) -> Result<u64> {
        self.spawn_session(Some(agent.to_string()))
    }

    fn spawn_session(&mut self, agent: Option<String>) -> Result<u64> {
        let (label, cmd) = match &agent {
            Some(slug) => (
                capitalize(slug),
                build_agent_command(slug, &self.env_passthrough),
            ),
            None => ("Shell".to_string(), build_shell_command()),
        };
        let (session, id) = Session::spawn(
            &label,
            agent.clone(),
            cmd,
            self.content_rows,
            self.term_cols,
            self.event_tx.clone(),
        )?;
        let tab_label = label.clone();
        self.sessions.insert(id, session);
        if self.tabs.is_empty() {
            self.tabs.push(Tab::new_single(tab_label, id));
            self.active_tab = 0;
        } else {
            self.tabs.push(Tab::new_single(tab_label, id));
            self.active_tab = self.tabs.len() - 1;
        }
        Ok(id)
    }

    fn split_focused(&mut self, horizontal: bool) -> Result<()> {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return Ok(());
        };
        let from_id = tab.focused_id;
        let agent_slug = self.sessions.get(&from_id).and_then(|s| s.agent.clone());
        let (label, cmd) = match &agent_slug {
            Some(slug) => (
                capitalize(slug),
                build_agent_command(slug, &self.env_passthrough),
            ),
            None => ("Shell".to_string(), build_shell_command()),
        };
        let (session, new_id) = Session::spawn(
            &label,
            agent_slug,
            cmd,
            self.content_rows / 2,
            self.term_cols,
            self.event_tx.clone(),
        )?;
        self.sessions.insert(new_id, session);
        let tab = &mut self.tabs[self.active_tab];
        if horizontal {
            tab.tree.split_h(from_id, new_id);
        } else {
            tab.tree.split_v(from_id, new_id);
        }
        tab.focused_id = new_id;
        self.resize_panes();
        Ok(())
    }

    fn close_focused_pane(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        let id = tab.focused_id;
        let all = tab.tree.all_ids();
        let next_focus = all.iter().find(|&&sid| sid != id).copied();
        tab.tree.remove(id);
        self.sessions.remove(&id);
        if let Some(nf) = next_focus {
            tab.focused_id = nf;
        } else {
            self.tabs.remove(self.active_tab);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len().saturating_sub(1);
            }
        }
        self.resize_panes();
    }

    fn toggle_zoom(&mut self) {
        let focused = self.tabs.get(self.active_tab).map(|t| t.focused_id);
        self.zoomed = if self.zoomed.is_some() { None } else { focused };
        self.resize_panes();
    }

    fn resize_panes(&mut self) {
        let content_rect = Rect::new(1, 0, self.content_rows, self.term_cols);
        if let Some(zoom_id) = self.zoomed {
            let (rows, cols) = (self.content_rows, self.term_cols);
            if let Some(session) = self.sessions.get_mut(&zoom_id) {
                session.resize(rows, cols);
                session.vterminal.resize(rows, cols);
            }
            return;
        }
        let leaves: Vec<(u64, Rect)> = self
            .tabs
            .iter()
            .flat_map(|tab| tab.tree.leaves(content_rect))
            .collect();
        for (id, rect) in leaves {
            if let Some(session) = self.sessions.get_mut(&id) {
                session.resize(rect.rows, rect.cols);
                session.vterminal.resize(rect.rows, rect.cols);
            }
        }
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        self.term_rows = rows;
        self.term_cols = cols;
        self.content_rows = rows.saturating_sub(1);
        self.resize_panes();
    }

    fn active_focused_id(&self) -> Option<u64> {
        self.tabs.get(self.active_tab).map(|t| t.focused_id)
    }

    fn move_focus(&mut self, dir: ArrowDir) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };
        let content_rect = Rect::new(1, 0, self.content_rows, self.term_cols);
        let d = match dir {
            ArrowDir::Left => Direction::Left,
            ArrowDir::Right => Direction::Right,
            ArrowDir::Up => Direction::Up,
            ArrowDir::Down => Direction::Down,
        };
        if let Some(next_id) = tab.tree.adjacent(content_rect, tab.focused_id, d) {
            self.tabs[self.active_tab].focused_id = next_id;
            if let Some(session) = self.sessions.get(&next_id) {
                session.force_redraw();
            }
        }
    }

    /// Handle a parsed input event from the client terminal.
    /// Returns bytes to send to the client (e.g. redraws), if any.
    fn handle_input(&mut self, event: InputEvent) -> Option<Vec<u8>> {
        match event {
            InputEvent::CommandPalette => {
                if self.dialog.is_some() {
                    // Ctrl+J while dialog is open → close dialog and redraw.
                    self.dialog = None;
                    Some(self.compose_frame())
                } else {
                    self.dialog = Some(Dialog::CommandPalette { selected: 0 });
                    Some(self.compose_frame())
                }
            }
            InputEvent::AltArrow(dir) => {
                self.move_focus(dir);
                Some(self.compose_frame())
            }
            InputEvent::MousePress {
                row,
                col,
                button: 0,
            } if row == 0 => {
                // Left click on status bar → tab switch.
                if let Some(idx) = self.status_bar.tab_at_col(col + 1) {
                    if idx < self.tabs.len() {
                        self.active_tab = idx;
                        if let Some(focused) = self.active_focused_id() {
                            if let Some(s) = self.sessions.get(&focused) {
                                s.force_redraw();
                            }
                        }
                        return Some(self.compose_frame());
                    }
                }
                None
            }
            InputEvent::MousePress { .. } => {
                // Mouse in content area — pass through to active session.
                if let Some(focused) = self.active_focused_id() {
                    if let Some(session) = self.sessions.get(&focused) {
                        // Re-encode as the original mouse bytes and send to PTY.
                        // For now pass-through is handled by the Data path below.
                        let _ = session;
                    }
                }
                None
            }
            InputEvent::Data(bytes) => {
                if let Some(ref mut dialog) = self.dialog {
                    let action = dialog.handle_key(&bytes);
                    match action {
                        DialogAction::Dismiss => {
                            self.dialog = None;
                            Some(self.compose_frame())
                        }
                        DialogAction::Redraw => Some(self.compose_frame()),
                        DialogAction::Command(cmd) => {
                            self.handle_palette_command(cmd);
                            self.dialog = None;
                            Some(self.compose_frame())
                        }
                        DialogAction::SpawnAgent { agent } => {
                            let _ = self.spawn_session(agent);
                            self.dialog = None;
                            Some(self.compose_frame())
                        }
                    }
                } else {
                    // Route raw bytes to the focused session's PTY.
                    if let Some(focused) = self.active_focused_id() {
                        if let Some(session) = self.sessions.get(&focused) {
                            session.send_input(&bytes);
                        }
                    }
                    None
                }
            }
        }
    }

    fn handle_palette_command(&mut self, cmd: PaletteCommand) -> Option<Vec<u8>> {
        match cmd {
            PaletteCommand::SplitHorizontal => {
                let _ = self.split_focused(true);
            }
            PaletteCommand::SplitVertical => {
                let _ = self.split_focused(false);
            }
            PaletteCommand::NewTab | PaletteCommand::NewSession => {
                let agents = self.available_agents.clone();
                self.dialog = Some(Dialog::AgentPicker {
                    agents,
                    selected: 0,
                });
            }
            PaletteCommand::ClosePane => {
                self.close_focused_pane();
            }
            PaletteCommand::ZoomPane => {
                self.toggle_zoom();
            }
        }
        None
    }

    fn compose_frame(&mut self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(65536);
        buf.extend_from_slice(b"\x1b[?25l");

        let states: Vec<(u64, AgentState)> =
            self.sessions.iter().map(|(&id, s)| (id, s.state)).collect();
        self.status_bar.render(
            &mut buf,
            self.term_cols,
            &self.tabs,
            self.active_tab,
            &states,
        );

        let content_rect = Rect::new(1, 0, self.content_rows, self.term_cols);
        let focused_id = self.active_focused_id();

        if let Some(zoom_id) = self.zoomed {
            if let Some(session) = self.sessions.get(&zoom_id) {
                session.vterminal.render_to(1, 0, &mut buf);
            }
        } else if let Some(tab) = self.tabs.get(self.active_tab) {
            let leaves = tab.tree.leaves(content_rect);
            let needs_borders = leaves.len() > 1;
            for (id, rect) in &leaves {
                if let Some(session) = self.sessions.get(id) {
                    session.vterminal.render_to(rect.row, rect.col, &mut buf);
                }
                if needs_borders {
                    let is_active = Some(*id) == focused_id;
                    let right_edge = rect.col + rect.cols;
                    if right_edge < self.term_cols {
                        draw_vertical_border(
                            &mut buf,
                            right_edge,
                            rect.row,
                            rect.row + rect.rows.saturating_sub(1),
                            is_active,
                        );
                    }
                    let bot_edge = rect.row + rect.rows;
                    if bot_edge < self.term_rows {
                        draw_horizontal_border(
                            &mut buf,
                            bot_edge,
                            rect.col,
                            rect.col + rect.cols.saturating_sub(1),
                            is_active,
                        );
                    }
                }
            }
        }

        if let Some(dialog) = &self.dialog {
            dialog.render(&mut buf, self.term_rows, self.term_cols);
        }

        buf.extend_from_slice(b"\x1b[?25h");
        buf
    }

    fn session_infos(&self) -> Vec<SessionInfo> {
        let focused = self.active_focused_id();
        self.sessions
            .iter()
            .map(|(&id, s)| SessionInfo {
                id,
                label: s.label.clone(),
                agent: s.agent.clone(),
                state: s.state,
                active: Some(id) == focused,
            })
            .collect()
    }
}

/// Run the multiplexer daemon. Called from `main` when PID == 1.
pub async fn run_daemon(initial_agent: String) -> Result<()> {
    crate::pid1::install_sigchld_reaper();

    let rows = std::env::var("JACKIN_ROWS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(24u16);
    let cols = std::env::var("JACKIN_COLS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(80u16);

    let mut mux = Multiplexer::new(rows, cols);
    mux.spawn_initial(&initial_agent)?;

    let mut new_clients = socket::start_listener()?;
    let mut state_ticker = interval(Duration::from_secs(1));

    // Outbound channel: main loop → connected client stream.
    let (out_tx, out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    // Inbound channel: client handler → main loop.
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<ClientMsg>();

    // Shared out_rx wrapped in an Option so we can move it into the client task.
    let mut out_rx_slot: Option<mpsc::UnboundedReceiver<Vec<u8>>> = Some(out_rx);

    loop {
        tokio::select! {
            // New client connected via socket.
            Some(mut stream) = new_clients.recv() => {
                let msg = socket::read_msg(&mut stream).await;
                match msg {
                    Some(ClientMsg::Hello { rows, cols }) => {
                        mux.resize(rows, cols);
                        let welcome = frame(&ServerMsg::Welcome { session_count: mux.sessions.len() });
                        let _ = out_tx.send(welcome);
                        // Send initial full frame.
                        let frame_data = mux.compose_frame();
                        let _ = out_tx.send(frame(&ServerMsg::Output { data: b64_encode(&frame_data) }));
                        // Spawn bidirectional client handler.
                        let rx = out_rx_slot.take().unwrap_or_else(|| {
                            // Previous client disconnected; create fresh channel pair.
                            let (_, new_rx) = mpsc::unbounded_channel();
                            new_rx
                        });
                        tokio::spawn(handle_client(stream, rx, cmd_tx.clone()));
                    }
                    Some(ClientMsg::Status) => {
                        socket::handle_status_query(stream, mux.session_infos()).await;
                    }
                    _ => {}
                }
            }

            // Inbound command from client handler.
            Some(msg) = cmd_rx.recv() => {
                match msg {
                    ClientMsg::Input { data } => {
                        let bytes = b64_decode(&data);
                        let events = parse(&bytes);
                        for event in events {
                            if let Some(redraw) = mux.handle_input(event) {
                                let _ = out_tx.send(frame(&ServerMsg::Output { data: b64_encode(&redraw) }));
                            }
                        }
                    }
                    ClientMsg::Resize { rows, cols } => {
                        mux.resize(rows, cols);
                        let frame_data = mux.compose_frame();
                        let _ = out_tx.send(frame(&ServerMsg::Output { data: b64_encode(&frame_data) }));
                    }
                    ClientMsg::NewSession { agent } => {
                        let _ = mux.spawn_session(agent);
                        let frame_data = mux.compose_frame();
                        let _ = out_tx.send(frame(&ServerMsg::Output { data: b64_encode(&frame_data) }));
                    }
                    ClientMsg::SwitchSession { id } => {
                        // Find the tab containing this session and switch to it.
                        for (i, tab) in mux.tabs.iter().enumerate() {
                            if tab.tree.all_ids().contains(&id) {
                                mux.active_tab = i;
                                break;
                            }
                        }
                        if let Some(session) = mux.sessions.get(&id) {
                            session.force_redraw();
                        }
                        let frame_data = mux.compose_frame();
                        let _ = out_tx.send(frame(&ServerMsg::Output { data: b64_encode(&frame_data) }));
                    }
                    ClientMsg::KillSession { id } => {
                        // Find the tab that owns this session, focus it, then close.
                        for (i, tab) in mux.tabs.iter().enumerate() {
                            if tab.tree.all_ids().contains(&id) {
                                mux.active_tab = i;
                                mux.tabs[i].focused_id = id;
                                break;
                            }
                        }
                        mux.close_focused_pane();
                        let frame_data = mux.compose_frame();
                        let _ = out_tx.send(frame(&ServerMsg::Output { data: b64_encode(&frame_data) }));
                    }
                    ClientMsg::Status => {
                        let list = frame(&ServerMsg::SessionList { sessions: mux.session_infos() });
                        let _ = out_tx.send(list);
                    }
                    ClientMsg::Hello { .. } => {} // second hello from re-attach — ignore
                }
            }

            // PTY output or exit event from a session.
            Some(event) = mux.event_rx.recv() => {
                match event {
                    SessionEvent::Output { session_id, data } => {
                        if let Some(session) = mux.sessions.get_mut(&session_id) {
                            session.vterminal.process(&data);
                            session.last_output_at = std::time::Instant::now();
                            session.state = AgentState::Working;

                            if mux.dialog.is_none() {
                                let focused = mux.tabs.get(mux.active_tab).map(|t| t.focused_id);
                                if focused == Some(session_id) {
                                    // Fast path: stream raw output for the active pane.
                                    let _ = out_tx.send(frame(&socket::encode_output(&data)));
                                } else {
                                    // Non-active pane: only redraw status bar.
                                    let mut sbuf = Vec::new();
                                    let states: Vec<(u64, AgentState)> = mux.sessions.iter()
                                        .map(|(&id, s)| (id, s.state))
                                        .collect();
                                    mux.status_bar.render(&mut sbuf, mux.term_cols, &mux.tabs, mux.active_tab, &states);
                                    let _ = out_tx.send(frame(&ServerMsg::Output { data: b64_encode(&sbuf) }));
                                }
                            }
                        }
                    }
                    SessionEvent::Exited { session_id } => {
                        if let Some(session) = mux.sessions.get_mut(&session_id) {
                            session.alive = false;
                            session.state = AgentState::Done;
                        }
                        if mux.sessions.values().all(|s| !s.alive) {
                            let _ = out_tx.send(frame(&ServerMsg::Shutdown));
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            std::process::exit(0);
                        }
                        let frame_data = mux.compose_frame();
                        let _ = out_tx.send(frame(&ServerMsg::Output { data: b64_encode(&frame_data) }));
                    }
                }
            }

            // Periodic state refresh.
            _ = state_ticker.tick() => {
                for session in mux.sessions.values_mut() {
                    session.refresh_state();
                }
                let mut sbuf = Vec::new();
                let states: Vec<(u64, AgentState)> = mux.sessions.iter()
                    .map(|(&id, s)| (id, s.state))
                    .collect();
                mux.status_bar.render(&mut sbuf, mux.term_cols, &mux.tabs, mux.active_tab, &states);
                let _ = out_tx.send(frame(&ServerMsg::Output { data: b64_encode(&sbuf) }));
            }
        }
    }
}

/// Per-client connection handler: bidirectional bridge between the socket
/// and the main daemon loop.
///
/// Reads `ClientMsg` from the socket → forwards to `cmd_tx`.
/// Reads outbound bytes from `out_rx` → writes to the socket.
async fn handle_client(
    mut stream: UnixStream,
    mut out_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    cmd_tx: mpsc::UnboundedSender<ClientMsg>,
) {
    let mut len_buf = [0u8; 4];
    loop {
        tokio::select! {
            // Read inbound framed ClientMsg from client terminal.
            result = stream.read_exact(&mut len_buf) => {
                if result.is_err() { break; }
                let len = u32::from_be_bytes(len_buf) as usize;
                if len > 4 * 1024 * 1024 { break; }
                let mut body = vec![0u8; len];
                if stream.read_exact(&mut body).await.is_err() { break; }
                let Ok(msg) = serde_json::from_slice::<ClientMsg>(&body) else { continue };
                if cmd_tx.send(msg).is_err() { break; }
            }
            // Write outbound bytes to client terminal.
            Some(bytes) = out_rx.recv() => {
                if stream.write_all(&bytes).await.is_err() { break; }
            }
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

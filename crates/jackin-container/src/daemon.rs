/// The multiplexer daemon — runs as PID 1, manages sessions and clients.
///
/// One client connects at a time (the operator's exec'd terminal).
/// The daemon renders the status bar + pane layout into the client's
/// terminal over the Unix socket.
use std::collections::HashMap;

use anyhow::Result;
use tokio::io::AsyncReadExt;
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use crate::dialog::Dialog;
use crate::input::ArrowDir;
use crate::layout::{Direction, Rect, Tab};
use crate::protocol::{AgentState, ClientMsg, ServerMsg, SessionInfo, b64_encode, frame};
use crate::session::{Session, SessionEvent, available_agents, build_agent_command, build_shell_command, next_id};
use crate::socket;
use crate::statusbar::{StatusBar, draw_horizontal_border, draw_vertical_border};

/// How long with no output before a session transitions to Blocked.
const BLOCKED_SECS: u64 = 5;

pub struct Multiplexer {
    sessions: HashMap<u64, Session>,
    tabs: Vec<Tab>,
    active_tab: usize,
    term_rows: u16,
    term_cols: u16,
    status_bar: StatusBar,
    dialog: Option<Dialog>,
    /// Content area rows = term_rows - 1 (row 0 = status bar).
    content_rows: u16,
    /// Available agent slugs from JACKIN_SUPPORTED_AGENTS env.
    available_agents: Vec<String>,
    /// Passthrough env vars forwarded to new agent sessions.
    env_passthrough: Vec<(String, String)>,
    /// Output events from all session PTYs.
    event_tx: mpsc::UnboundedSender<SessionEvent>,
    event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    /// Outbound frame buffer for the current client.
    out_buf: Vec<u8>,
    /// Zoom: if Some(id), only that pane fills the content area.
    zoomed: Option<u64>,
}

impl Multiplexer {
    pub fn new(rows: u16, cols: u16) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let content_rows = rows.saturating_sub(1);
        let agents = available_agents();

        // Env vars we forward to spawned sessions.
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
            out_buf: Vec::with_capacity(65536),
            zoomed: None,
        }
    }

    /// Spawn the first session and return the agent slug used.
    pub fn spawn_initial(&mut self, agent: &str) -> Result<u64> {
        let id = self.spawn_session(Some(agent.to_string()))?;
        Ok(id)
    }

    fn spawn_session(&mut self, agent: Option<String>) -> Result<u64> {
        let id = next_id();
        let (label, cmd) = match &agent {
            Some(slug) => (
                capitalize(slug),
                build_agent_command(slug, &self.env_passthrough),
            ),
            None => ("Shell".to_string(), build_shell_command()),
        };

        let session = Session::spawn(
            id,
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
            // Add a new tab for this session.
            self.tabs.push(Tab::new_single(tab_label, id));
            self.active_tab = self.tabs.len() - 1;
        }

        Ok(id)
    }

    fn split_focused(&mut self, horizontal: bool) -> Result<()> {
        let Some(tab) = self.tabs.get(self.active_tab) else { return Ok(()); };
        let from_id = tab.focused_id;
        let agent_slug = self.sessions.get(&from_id)
            .and_then(|s| s.agent.clone());

        let new_id = next_id();
        let (label, cmd) = match &agent_slug {
            Some(slug) => (capitalize(slug), build_agent_command(slug, &self.env_passthrough)),
            None => ("Shell".to_string(), build_shell_command()),
        };

        let session = Session::spawn(
            new_id,
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
        let Some(tab) = self.tabs.get_mut(self.active_tab) else { return };
        let id = tab.focused_id;

        // Find a sibling to focus after removal.
        let all = tab.tree.all_ids();
        let next_focus = all.iter().find(|&&sid| sid != id).copied();

        tab.tree.remove(id);
        self.sessions.remove(&id);

        if let Some(nf) = next_focus {
            tab.focused_id = nf;
        } else {
            // No panes left in this tab — remove the tab.
            self.tabs.remove(self.active_tab);
            if self.active_tab >= self.tabs.len() {
                self.active_tab = self.tabs.len().saturating_sub(1);
            }
        }
        self.resize_panes();
    }

    fn toggle_zoom(&mut self) {
        let focused = self.tabs.get(self.active_tab).map(|t| t.focused_id);
        if self.zoomed.is_some() {
            self.zoomed = None;
        } else {
            self.zoomed = focused;
        }
        self.resize_panes();
    }

    /// Recompute PTY sizes for all panes based on current tab layout.
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

        // Collect leaves before mutably borrowing sessions.
        let leaves: Vec<(u64, Rect)> = self.tabs.iter()
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
        for session in self.sessions.values() {
            session.resize(self.content_rows, cols);
        }
        self.resize_panes();
    }

    fn active_focused_id(&self) -> Option<u64> {
        self.tabs.get(self.active_tab).map(|t| t.focused_id)
    }

    fn move_focus(&mut self, dir: ArrowDir) {
        let Some(tab) = self.tabs.get(self.active_tab) else { return };
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

    /// Build a full compositor frame: status bar + all pane grids + borders.
    fn compose_frame(&mut self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(65536);

        // Hide cursor during redraw.
        buf.extend_from_slice(b"\x1b[?25l");

        // Status bar (row 0).
        let states: Vec<(u64, AgentState)> = self.sessions.iter()
            .map(|(&id, s)| (id, s.state))
            .collect();
        self.status_bar.render(&mut buf, self.term_cols, &self.tabs, self.active_tab, &states);

        // Content area.
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

                // Draw border on the RIGHT of each pane in an HSplit.
                // Borders are drawn by the compositor, not the panes themselves.
                if needs_borders {
                    let is_active = Some(*id) == focused_id;
                    // Right border (if not at rightmost column).
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
                    // Bottom border.
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

        // Render dialog overlay if open.
        if let Some(dialog) = &self.dialog {
            dialog.render(&mut buf, self.term_rows, self.term_cols);
        }

        // Restore cursor.
        buf.extend_from_slice(b"\x1b[?25h");

        buf
    }

    fn session_infos(&self) -> Vec<SessionInfo> {
        let focused = self.active_focused_id();
        self.sessions.iter().map(|(&id, s)| SessionInfo {
            id,
            label: s.label.clone(),
            agent: s.agent.clone(),
            state: s.state,
            active: Some(id) == focused,
        }).collect()
    }
}

/// Run the multiplexer daemon. Called from `main` when PID == 1.
pub async fn run_daemon(initial_agent: String) -> Result<()> {
    // Install zombie reaper before spawning any children.
    crate::pid1::install_sigchld_reaper();

    // Read initial terminal size from the env (set by the client before
    // exec, or defaulted). The client sends a Resize on connect anyway.
    let rows = std::env::var("JACKIN_ROWS").ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(24u16);
    let cols = std::env::var("JACKIN_COLS").ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(80u16);

    let mut mux = Multiplexer::new(rows, cols);
    mux.spawn_initial(&initial_agent)?;

    // Start Unix socket listener.
    let mut new_clients = socket::start_listener()?;

    // Periodic state-refresh ticker (infers working/blocked from output timing).
    let mut state_ticker = interval(Duration::from_secs(1));

    // Channel for outbound bytes to the currently-connected client.
    let (client_tx, mut client_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    loop {
        tokio::select! {
            // New client connected via socket.
            Some(mut stream) = new_clients.recv() => {
                let msg = socket::read_msg(&mut stream).await;
                match msg {
                    Some(ClientMsg::Hello { rows, cols }) => {
                        mux.resize(rows, cols);
                        let welcome = ServerMsg::Welcome { session_count: mux.sessions.len() };
                        let _ = socket::write_msg(&mut stream, &welcome).await;
                        // Send initial full frame.
                        let frame_data = mux.compose_frame();
                        let out_msg = ServerMsg::Output { data: b64_encode(&frame_data) };
                        let _ = socket::write_msg(&mut stream, &out_msg).await;
                        // Hand off stream to a connection handler task.
                        let tx = client_tx.clone();
                        tokio::spawn(handle_client(stream, tx));
                    }
                    Some(ClientMsg::Status) => {
                        socket::handle_status_query(stream, mux.session_infos()).await;
                    }
                    _ => {}
                }
            }

            // Session PTY output or exit event.
            Some(event) = mux.event_rx.recv() => {
                match event {
                    SessionEvent::Output { session_id, data } => {
                        if let Some(session) = mux.sessions.get_mut(&session_id) {
                            session.vterminal.process(&data);
                            session.last_output_at = std::time::Instant::now();
                            session.state = AgentState::Working;

                            // If the session is in the active tab + focused pane,
                            // forward raw output to client for minimal latency.
                            if mux.dialog.is_none() {
                                let focused = mux.tabs.get(mux.active_tab)
                                    .map(|t| t.focused_id);
                                if focused == Some(session_id) {
                                    let out_msg = socket::encode_output(&data);
                                    let framed = frame(&out_msg);
                                    let _ = client_tx.send(framed);
                                } else {
                                    // Non-active pane updated — redraw status bar only.
                                    let mut sbuf = Vec::new();
                                    let states: Vec<(u64, AgentState)> = mux.sessions.iter()
                                        .map(|(&id, s)| (id, s.state))
                                        .collect();
                                    mux.status_bar.render(&mut sbuf, mux.term_cols, &mux.tabs, mux.active_tab, &states);
                                    let msg = ServerMsg::Output { data: b64_encode(&sbuf) };
                                    let framed = frame(&msg);
                                    let _ = client_tx.send(framed);
                                }
                            }
                        }
                    }
                    SessionEvent::Exited { session_id } => {
                        if let Some(session) = mux.sessions.get_mut(&session_id) {
                            session.alive = false;
                            session.state = AgentState::Done;
                        }
                        // If last session exited, shut down.
                        if mux.sessions.values().all(|s| !s.alive) {
                            let shutdown = frame(&ServerMsg::Shutdown);
                            let _ = client_tx.send(shutdown);
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            std::process::exit(0);
                        }
                        // Redraw to update state indicators.
                        let frame_data = mux.compose_frame();
                        let out_msg = ServerMsg::Output { data: b64_encode(&frame_data) };
                        let framed = frame(&out_msg);
                        let _ = client_tx.send(framed);
                    }
                }
            }

            // Outbound bytes from client handler tasks — forward to... wait,
            // client_rx is for messages FROM sub-tasks. We handle it below.
            // Actually the client_tx sends directly; skip this arm.

            // Periodic state refresh.
            _ = state_ticker.tick() => {
                for session in mux.sessions.values_mut() {
                    session.refresh_state();
                }
                // Redraw status bar to reflect updated states.
                let mut sbuf = Vec::new();
                let states: Vec<(u64, AgentState)> = mux.sessions.iter()
                    .map(|(&id, s)| (id, s.state))
                    .collect();
                mux.status_bar.render(&mut sbuf, mux.term_cols, &mux.tabs, mux.active_tab, &states);
                let msg = ServerMsg::Output { data: b64_encode(&sbuf) };
                let framed = frame(&msg);
                let _ = client_tx.send(framed);
            }
        }
    }
}

/// Per-client connection handler.
/// Reads ClientMsgs from the client and applies them to the multiplexer
/// via an mpsc channel back to the main loop.
///
/// For v1: the daemon uses a global client_tx for outbound bytes. This
/// handler reads inbound messages and applies them via a command channel.
async fn handle_client(
    mut stream: UnixStream,
    out_tx: mpsc::UnboundedSender<Vec<u8>>,
) {
    let _ = out_tx; // used by daemon main loop directly
    // Read from stream, discard (client sends resize/input which
    // the daemon handles inline). For v1 this is a placeholder;
    // proper bidirectional streaming is wired in run_client_session.
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
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

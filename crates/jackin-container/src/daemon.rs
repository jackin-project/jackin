/// The multiplexer daemon — runs as PID 1, manages sessions and clients.
///
/// Architecture:
///   - One active attach client at a time. A new `Hello` from a second
///     client sends `Shutdown` to the old one and takes over.
///   - Attach traffic uses the binary tag+length protocol in
///     `protocol::attach`. The hot path forwards raw PTY bytes without
///     base64 or JSON nesting.
///   - The control channel still speaks length-prefixed JSON for one-shot
///     `status` queries from the host CLI. Channel dispatch is by first
///     byte: `0x00` → control (length prefix), anything else → attach.
///   - The daemon is persistent: it does not exit when the last session
///     dies. Only `SIGTERM` triggers shutdown.
use std::collections::HashMap;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

use crate::dialog::{Dialog, DialogAction, PaletteCommand};
use crate::input::{ArrowDir, InputEvent, InputParser, PrefixCommand};
use crate::layout::{Direction, Rect, Tab};
use crate::protocol::attach::{ClientFrame, ServerFrame, encode_server, read_client_frame};
use crate::protocol::control::{AgentState, SessionInfo};
use crate::render::{draw_scrollbar, render_pane};
use crate::session::SCROLLBACK_LEN;
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
    input_parser: InputParser,
    detach_requested: bool,
    attached_out: Option<mpsc::UnboundedSender<Vec<u8>>>,
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

        let input_parser = InputParser::default();
        let mut status_bar = StatusBar::new();
        status_bar.set_prefix_enabled(input_parser.prefix_enabled());

        Self {
            sessions: HashMap::new(),
            tabs: Vec::new(),
            active_tab: 0,
            term_rows: rows,
            term_cols: cols,
            status_bar,
            dialog: None,
            content_rows,
            available_agents: agents,
            env_passthrough,
            event_tx,
            event_rx,
            zoomed: None,
            input_parser,
            detach_requested: false,
            attached_out: None,
        }
    }

    fn send_to_client(&self, frame: ServerFrame) {
        if let Some(tx) = &self.attached_out {
            let _ = tx.send(encode_server(frame));
        }
    }

    fn send_output(&self, bytes: Vec<u8>) {
        self.send_to_client(ServerFrame::Output(bytes));
    }

    fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let prev = self.active_focused_id();
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
        self.synthesise_focus_swap(prev, self.active_focused_id());
    }

    fn prev_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let prev = self.active_focused_id();
        self.active_tab = if self.active_tab == 0 {
            self.tabs.len() - 1
        } else {
            self.active_tab - 1
        };
        self.synthesise_focus_swap(prev, self.active_focused_id());
    }

    fn jump_tab(&mut self, idx: usize) {
        if idx < self.tabs.len() && idx != self.active_tab {
            let prev = self.active_focused_id();
            self.active_tab = idx;
            self.synthesise_focus_swap(prev, self.active_focused_id());
        }
    }

    fn close_focused_tab(&mut self) {
        if self.active_tab >= self.tabs.len() {
            return;
        }
        let tab_ids = self.tabs[self.active_tab].tree.all_ids();
        for id in tab_ids {
            self.sessions.remove(&id);
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len().saturating_sub(1);
        }
        self.zoomed = None;
        self.resize_panes();
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

    /// True when nothing the operator could attach to is still alive.
    /// `sessions.is_empty()` covers the operator-explicitly-killed-all
    /// case; `all !alive` covers the natural-exit case (every agent /
    /// shell process closed its PTY).
    fn no_live_sessions(&self) -> bool {
        self.sessions.is_empty() || self.sessions.values().all(|s| !s.alive)
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
        let prev = tab.focused_id;
        if let Some(next_id) = tab.tree.adjacent(content_rect, tab.focused_id, d) {
            self.tabs[self.active_tab].focused_id = next_id;
            self.synthesise_focus_swap(Some(prev), Some(next_id));
        }
    }

    /// Synthesise `\x1b[O` / `\x1b[I` to track which pane the operator
    /// is actually looking at. Agents that watch focus events use them
    /// to pause polling / animations; without synthesis, a backgrounded
    /// pane thinks it is still focused.
    fn synthesise_focus_swap(&self, old: Option<u64>, new: Option<u64>) {
        if old == new {
            return;
        }
        if let Some(o) = old
            && let Some(s) = self.sessions.get(&o)
        {
            s.send_input(b"\x1b[O");
        }
        if let Some(n) = new
            && let Some(s) = self.sessions.get(&n)
        {
            s.send_input(b"\x1b[I");
        }
    }

    /// Handle a parsed input event from the client terminal.
    /// Returns bytes to send to the client (e.g. redraws), if any.
    fn handle_input(&mut self, event: InputEvent) -> Option<Vec<u8>> {
        match event {
            InputEvent::OpenPalette => {
                // Toggle: second palette-key press closes the dialog.
                if self.dialog.is_some() {
                    self.dialog = None;
                } else {
                    self.dialog = Some(Dialog::CommandPalette { selected: 0 });
                }
                Some(self.compose_frame())
            }
            InputEvent::PrefixCommand(cmd) => self.handle_prefix_command(cmd),
            InputEvent::FocusIn | InputEvent::FocusOut => {
                // Forward focus events to the focused pane's PTY so the
                // agent can pause/resume animations. Synthesised events
                // on tab/pane focus changes are not implemented here yet
                // — Phase 3d wires that up.
                let bytes = if matches!(event, InputEvent::FocusIn) {
                    b"\x1b[I".as_ref()
                } else {
                    b"\x1b[O".as_ref()
                };
                if let Some(focused) = self.active_focused_id()
                    && let Some(session) = self.sessions.get(&focused)
                {
                    session.send_input(bytes);
                }
                None
            }
            InputEvent::MousePress { button, .. } if button == 64 || button == 65 => {
                // SGR mouse wheel: 64 = wheel up, 65 = wheel down.
                // The multiplexer intercepts the wheel for scrollback
                // navigation rather than forwarding to the agent —
                // most agents have no notion of multiplexer-level
                // history, and scrollback is the operator UX the
                // wheel naturally maps to.
                let delta = if button == 64 { 3 } else { -3 };
                if let Some(focused) = self.active_focused_id()
                    && let Some(session) = self.sessions.get_mut(&focused)
                {
                    session.scroll_by(delta);
                }
                Some(self.compose_frame())
            }
            InputEvent::MousePress {
                row: 0,
                col,
                button: 0,
            } => {
                if let Some(idx) = self.status_bar.tab_at_col(col + 1)
                    && idx < self.tabs.len()
                    && idx != self.active_tab
                {
                    let prev = self.active_focused_id();
                    self.active_tab = idx;
                    self.synthesise_focus_swap(prev, self.active_focused_id());
                    return Some(self.compose_frame());
                }
                None
            }
            InputEvent::MousePress { col, row, button } => {
                // Re-encode mouse press relative to the focused pane's
                // rect origin and forward to its PTY in SGR mouse form.
                self.forward_mouse_to_focused_pane(col, row, button);
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
                            // `handle_palette_command` owns the dialog
                            // state — it closes the dialog by default
                            // and overwrites it when the command opens
                            // a sub-dialog (e.g. NewTab → agent picker).
                            self.handle_palette_command(cmd);
                            Some(self.compose_frame())
                        }
                        DialogAction::SpawnAgent { agent } => {
                            let _ = self.spawn_session(agent);
                            self.dialog = None;
                            Some(self.compose_frame())
                        }
                    }
                } else {
                    // Any keyboard input from the operator returns the
                    // focused pane to the live tail. Matches the
                    // common multiplexer convention that "I'm typing
                    // again" implies "show me what's happening now."
                    let mut snapped = false;
                    if let Some(focused) = self.active_focused_id()
                        && let Some(session) = self.sessions.get_mut(&focused)
                    {
                        if session.scrollback_offset != 0 {
                            session.scroll_to_live();
                            snapped = true;
                        }
                        session.send_input(&bytes);
                    }
                    if snapped {
                        Some(self.compose_frame())
                    } else {
                        None
                    }
                }
            }
        }
    }

    fn handle_prefix_command(&mut self, cmd: PrefixCommand) -> Option<Vec<u8>> {
        match cmd {
            PrefixCommand::NewTab => {
                let agents = self.available_agents.clone();
                self.dialog = Some(Dialog::AgentPicker {
                    agents,
                    selected: 0,
                });
            }
            PrefixCommand::NextTab => self.next_tab(),
            PrefixCommand::PrevTab => self.prev_tab(),
            PrefixCommand::JumpTab(i) => self.jump_tab(i),
            PrefixCommand::SplitTopBottom => {
                let _ = self.split_focused(false);
            }
            PrefixCommand::SplitSideBySide => {
                let _ = self.split_focused(true);
            }
            PrefixCommand::MoveFocus(dir) => self.move_focus(dir),
            PrefixCommand::ZoomToggle => self.toggle_zoom(),
            PrefixCommand::KillPane => self.close_focused_pane(),
            PrefixCommand::KillTab => self.close_focused_tab(),
            PrefixCommand::Detach => {
                self.detach_requested = true;
            }
            PrefixCommand::Palette => {
                self.dialog = Some(Dialog::CommandPalette { selected: 0 });
            }
            PrefixCommand::Redraw => {}
        }
        Some(self.compose_frame())
    }

    fn forward_mouse_to_focused_pane(&mut self, col: u16, row: u16, button: u8) {
        let Some(focused) = self.active_focused_id() else {
            return;
        };
        let content_rect = Rect::new(1, 0, self.content_rows, self.term_cols);
        let pane_rect = if let Some(zoom_id) = self.zoomed {
            if zoom_id == focused {
                Some(content_rect)
            } else {
                None
            }
        } else {
            self.tabs
                .get(self.active_tab)
                .and_then(|tab| {
                    tab.tree
                        .leaves(content_rect)
                        .into_iter()
                        .find(|(id, _)| *id == focused)
                })
                .map(|(_, rect)| rect)
        };
        let Some(rect) = pane_rect else {
            return;
        };
        if row < rect.row || row >= rect.row + rect.rows {
            return;
        }
        if col < rect.col || col >= rect.col + rect.cols {
            return;
        }
        let local_row = row - rect.row;
        let local_col = col - rect.col;
        // SGR mouse press: ESC [ < button ; col+1 ; row+1 M
        let buf = format!("\x1b[<{};{};{}M", button, local_col + 1, local_row + 1);
        if let Some(session) = self.sessions.get(&focused) {
            session.send_input(buf.as_bytes());
        }
    }

    fn handle_palette_command(&mut self, cmd: PaletteCommand) -> Option<Vec<u8>> {
        // Default: close the dialog after the command runs. Commands
        // that need a follow-up choice (e.g. NewTab → "which agent?")
        // overwrite `self.dialog` themselves AFTER this reset, so the
        // sub-dialog survives this handler.
        self.dialog = None;
        match cmd {
            PaletteCommand::SplitHorizontal => {
                let _ = self.split_focused(true);
            }
            PaletteCommand::SplitVertical => {
                let _ = self.split_focused(false);
            }
            PaletteCommand::NewTab => {
                // Always show the agent picker — even when the role
                // declares a single agent. The operator must
                // explicitly choose between that agent and a Shell;
                // jumping straight into the agent would surprise an
                // operator who picked "New tab" to open a shell.
                let agents = self.available_agents.clone();
                self.dialog = Some(Dialog::AgentPicker {
                    agents,
                    selected: 0,
                });
            }
            PaletteCommand::NextTab => self.next_tab(),
            PaletteCommand::PrevTab => self.prev_tab(),
            PaletteCommand::ClosePane => self.close_focused_pane(),
            PaletteCommand::CloseTab => self.close_focused_tab(),
            PaletteCommand::ZoomPane => self.toggle_zoom(),
            PaletteCommand::Detach => {
                self.detach_requested = true;
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
        let mut focused_pane_rect: Option<Rect> = None;

        // Dim the panes when a dialog is open so the operator gets an
        // unmistakable "focus is inside the dialog" cue.
        let dim_panes = self.dialog.is_some();

        if let Some(zoom_id) = self.zoomed {
            if let Some(session) = self.sessions.get(&zoom_id) {
                let rect = Rect::new(1, 0, self.content_rows, self.term_cols);
                render_pane(
                    session.screen(),
                    rect.row,
                    rect.col,
                    rect.rows,
                    rect.cols,
                    dim_panes,
                    &mut buf,
                );
                draw_scrollbar(
                    &mut buf,
                    rect.row,
                    rect.col,
                    rect.rows,
                    rect.cols,
                    session.scrollback_offset,
                    SCROLLBACK_LEN,
                );
                if Some(zoom_id) == focused_id {
                    focused_pane_rect = Some(rect);
                }
            }
        } else if let Some(tab) = self.tabs.get(self.active_tab) {
            let leaves = tab.tree.leaves(content_rect);
            let needs_borders = leaves.len() > 1;
            for (id, rect) in &leaves {
                if let Some(session) = self.sessions.get(id) {
                    render_pane(
                        session.screen(),
                        rect.row,
                        rect.col,
                        rect.rows,
                        rect.cols,
                        dim_panes,
                        &mut buf,
                    );
                    draw_scrollbar(
                        &mut buf,
                        rect.row,
                        rect.col,
                        rect.rows,
                        rect.cols,
                        session.scrollback_offset,
                        SCROLLBACK_LEN,
                    );
                    if Some(*id) == focused_id {
                        focused_pane_rect = Some(*rect);
                    }
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

        // Position cursor at the focused pane's screen cursor; honour
        // the agent's hide-cursor request when no dialog is open.
        if self.dialog.is_none() {
            if let (Some(fid), Some(rect)) = (focused_id, focused_pane_rect)
                && let Some(session) = self.sessions.get(&fid)
            {
                let screen = session.screen();
                let (vt_row, vt_col) = screen.cursor_position();
                use std::io::Write as _;
                let _ = write!(
                    buf,
                    "\x1b[{};{}H",
                    rect.row + vt_row + 1,
                    rect.col + vt_col + 1
                );
                if !screen.hide_cursor() {
                    buf.extend_from_slice(b"\x1b[?25h");
                }
            } else {
                buf.extend_from_slice(b"\x1b[?25h");
            }
        }

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
    // Spawn the first tab. Treat any spawn error as fatal at boot —
    // it usually means the entrypoint binary is missing from the
    // derived image, and silently degrading to an empty multiplexer
    // would hide the real problem behind a blank screen.
    if !initial_agent.is_empty() {
        if let Err(err) = mux.spawn_initial(&initial_agent) {
            eprintln!(
                "[jackin-container] initial agent spawn failed (agent={initial_agent:?}): {err:?}"
            );
            return Err(err);
        }
    } else if let Err(err) = mux.spawn_session(None) {
        eprintln!("[jackin-container] initial shell spawn failed: {err:?}");
        return Err(err);
    }

    let mut new_clients = socket::start_listener()?;
    let mut state_ticker = interval(Duration::from_secs(1));
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    // Inbound: attach handler tasks → main loop.
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<ClientFrame>();

    loop {
        tokio::select! {
            biased;

            _ = sigterm.recv() => {
                if let Some(tx) = mux.attached_out.take() {
                    let _ = tx.send(encode_server(ServerFrame::Shutdown));
                }
                return Ok(());
            }
            _ = sigint.recv() => {
                if let Some(tx) = mux.attached_out.take() {
                    let _ = tx.send(encode_server(ServerFrame::Shutdown));
                }
                return Ok(());
            }

            // New socket connection.
            Some(mut stream) = new_clients.recv() => {
                let mut first = [0u8; 1];
                if stream.read_exact(&mut first).await.is_err() {
                    continue;
                }
                if first[0] == 0x00 {
                    // Control channel — one-shot length-prefixed JSON.
                    socket::handle_control_request(stream, first[0], mux.session_infos()).await;
                    continue;
                }
                // Attach channel — first byte is the first frame's tag.
                let Ok(Some(initial_frame)) = read_client_frame(&mut stream, first[0]).await else {
                    continue;
                };
                let ClientFrame::Hello { rows, cols } = initial_frame else {
                    // Attach clients must say Hello first; drop.
                    continue;
                };
                mux.resize(rows, cols);
                // Take over from any existing attach client.
                if let Some(old) = mux.attached_out.take() {
                    let _ = old.send(encode_server(ServerFrame::Shutdown));
                }
                let (new_out_tx, new_out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
                mux.attached_out = Some(new_out_tx.clone());
                let welcome = encode_server(ServerFrame::Welcome {
                    session_count: mux.sessions.len() as u32,
                });
                let _ = new_out_tx.send(welcome);
                let mut initial = b"\x1b[2J".to_vec();
                initial.extend(mux.compose_frame());
                let _ = new_out_tx.send(encode_server(ServerFrame::Output(initial)));
                tokio::spawn(handle_attach_client(stream, new_out_rx, cmd_tx.clone()));
            }

            // Inbound attach frame from the active client task.
            Some(frame) = cmd_rx.recv() => {
                handle_client_frame(&mut mux, frame).await;
                if mux.detach_requested {
                    mux.detach_requested = false;
                    if let Some(tx) = mux.attached_out.take() {
                        let _ = tx.send(encode_server(ServerFrame::Shutdown));
                    }
                }
                if mux.no_live_sessions() {
                    drain_and_exit(&mut mux).await;
                    return Ok(());
                }
            }

            // PTY output or exit event from a session.
            Some(event) = mux.event_rx.recv() => {
                match event {
                    SessionEvent::Output { session_id, data } => {
                        let focused_id = mux.active_focused_id();
                        if let Some(session) = mux.sessions.get_mut(&session_id) {
                            session.feed_pty(&data);
                            // Drain OSC and unhandled-CSI sequences and
                            // forward them to the client only when this
                            // session is the focused pane in the active
                            // tab — backgrounded panes' notifications,
                            // clipboard writes, and titles must not
                            // reach the operator's outer terminal.
                            let passthrough = session.drain_passthrough();
                            if Some(session_id) == focused_id {
                                for bytes in passthrough {
                                    mux.send_output(bytes);
                                }
                            }
                            if mux.dialog.is_none() {
                                let frame_data = mux.compose_frame();
                                mux.send_output(frame_data);
                            }
                        }
                    }
                    SessionEvent::Exited { session_id } => {
                        if let Some(session) = mux.sessions.get_mut(&session_id) {
                            session.alive = false;
                            session.state = AgentState::Done;
                        }
                        let frame_data = mux.compose_frame();
                        mux.send_output(frame_data);
                        // When the last live session exits — whether
                        // the operator typed `/exit` in the agent or
                        // the agent crashed — there is nothing left to
                        // attach to. Tear down the container so the
                        // host cleanup path fires.
                        if mux.no_live_sessions() {
                            drain_and_exit(&mut mux).await;
                            return Ok(());
                        }
                    }
                }
            }

            // Periodic state refresh: re-render the status bar so the tab
            // strip's state glyph follows the four-state model. The full
            // pane bodies stay where they are.
            _ = state_ticker.tick() => {
                for session in mux.sessions.values_mut() {
                    session.refresh_state();
                }
                let mut sbuf = Vec::new();
                let states: Vec<(u64, AgentState)> = mux.sessions.iter()
                    .map(|(&id, s)| (id, s.state))
                    .collect();
                mux.status_bar.render(&mut sbuf, mux.term_cols, &mux.tabs, mux.active_tab, &states);
                mux.send_output(sbuf);
            }
        }
    }
}

async fn handle_client_frame(mux: &mut Multiplexer, frame: ClientFrame) {
    match frame {
        ClientFrame::Hello { .. } => {
            // The initial Hello is consumed by the accept handler; any
            // further Hello on the same connection is ignored.
        }
        ClientFrame::Resize { rows, cols } => {
            mux.resize(rows, cols);
            let frame_data = mux.compose_frame();
            mux.send_output(frame_data);
        }
        ClientFrame::Input(bytes) => {
            let events = mux.input_parser.parse(&bytes);
            for event in events {
                if let Some(redraw) = mux.handle_input(event) {
                    mux.send_output(redraw);
                }
            }
            // Reflect prefix-await state in the status bar so the right
            // hint switches between `detach: …` and `prefix…`.
            let mode = if mux.input_parser.is_awaiting_prefix() {
                crate::statusbar::PrefixMode::Awaiting
            } else {
                crate::statusbar::PrefixMode::Idle
            };
            if mux.status_bar.prefix_mode != mode {
                mux.status_bar.set_prefix_mode(mode);
                let frame_data = mux.compose_frame();
                mux.send_output(frame_data);
            }
        }
        ClientFrame::Command(_payload) => {
            // Reserved for future structured commands from the host
            // CLI. Phase 3 has no senders yet.
        }
        ClientFrame::Detach => {
            mux.detach_requested = true;
        }
        ClientFrame::FocusIn => {
            if let Some(focused) = mux.active_focused_id()
                && let Some(s) = mux.sessions.get(&focused)
            {
                s.send_input(b"\x1b[I");
            }
        }
        ClientFrame::FocusOut => {
            if let Some(focused) = mux.active_focused_id()
                && let Some(s) = mux.sessions.get(&focused)
            {
                s.send_input(b"\x1b[O");
            }
        }
    }
}

/// Per-client connection handler: bidirectional bridge between the socket
/// and the main daemon loop.
/// Send `Shutdown` to the attached client and pause briefly so the
/// frame actually leaves the socket before PID 1 exits. Called when
/// the daemon decides to tear the container down (last session died,
/// last pane killed, or SIGTERM arrived).
async fn drain_and_exit(mux: &mut Multiplexer) {
    if let Some(tx) = mux.attached_out.take() {
        let _ = tx.send(encode_server(ServerFrame::Shutdown));
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
}

async fn handle_attach_client(
    mut stream: UnixStream,
    mut out_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    cmd_tx: mpsc::UnboundedSender<ClientFrame>,
) {
    let mut tag = [0u8; 1];
    loop {
        tokio::select! {
            result = stream.read_exact(&mut tag) => {
                if result.is_err() { break; }
                let Ok(Some(frame)) = read_client_frame(&mut stream, tag[0]).await else {
                    break;
                };
                if cmd_tx.send(frame).is_err() { break; }
            }
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

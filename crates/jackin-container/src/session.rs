/// PTY session: one PTY + one `vt100::Parser` + state-inference timer.
///
/// Each session owns a PTY pair, a child process (agent or shell), and
/// the `vt100::Parser` whose `Screen` mirrors the agent's view. The
/// parser is the source of truth for re-rendering on tab switch, pane
/// switch, and client reattach.
///
/// The parser is constructed with an `OscCapture` callback that
/// preserves OSC and unhandled-CSI byte sequences as the agent emits
/// them. The daemon drains the captured payloads after each PTY chunk
/// and forwards them to the attached client *only* when the session
/// owns the focused pane in the active tab — the routing rule the
/// roadmap calls out under "OSC passthrough". Without this layer the
/// `vt100` parser silently consumes OSC, so agent desktop
/// notifications (OSC 9), clipboard writes (OSC 52), window titles
/// (OSC 0/1/2), hyperlinks (OSC 8), kitty-keyboard protocol switches
/// (`\x1b[>{n}u`), synchronised output markers (`\x1b[?2026h/l`), and
/// every other terminal extension the operator's outer terminal
/// understands would vanish at the multiplexer boundary.
use std::io::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;
use vt100::{Callbacks, Screen};

use crate::protocol::AgentState;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Lines of scrollback every PTY session retains. ~1.5 MB worst-case
/// per session at 200 cols. Empty cells cost less. Operators need
/// scrollback to read Codex / Claude responses that exceed one
/// viewport, so this stays generous.
pub const SCROLLBACK_LEN: usize = 10_000;

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// `vt100::Callbacks` impl that captures OSC and unhandled-CSI byte
/// sequences for later focused-pane forwarding to the attached client.
#[derive(Default)]
pub struct OscCapture {
    pub pending: Vec<Vec<u8>>,
    pub title: Option<String>,
    pub icon_name: Option<String>,
}

impl OscCapture {
    pub fn drain(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.pending)
    }
}

impl Callbacks for OscCapture {
    fn set_window_title(&mut self, _: &mut Screen, title: &[u8]) {
        if let Ok(s) = std::str::from_utf8(title) {
            self.title = Some(s.to_string());
        }
        let mut osc = b"\x1b]2;".to_vec();
        osc.extend_from_slice(title);
        osc.extend_from_slice(b"\x07");
        self.pending.push(osc);
    }

    fn set_window_icon_name(&mut self, _: &mut Screen, icon_name: &[u8]) {
        if let Ok(s) = std::str::from_utf8(icon_name) {
            self.icon_name = Some(s.to_string());
        }
        let mut osc = b"\x1b]1;".to_vec();
        osc.extend_from_slice(icon_name);
        osc.extend_from_slice(b"\x07");
        self.pending.push(osc);
    }

    fn copy_to_clipboard(&mut self, _: &mut Screen, ty: &[u8], data: &[u8]) {
        let mut osc = b"\x1b]52;".to_vec();
        osc.extend_from_slice(ty);
        osc.push(b';');
        osc.extend_from_slice(data);
        osc.extend_from_slice(b"\x07");
        self.pending.push(osc);
    }

    fn unhandled_osc(&mut self, _: &mut Screen, params: &[&[u8]]) {
        let mut osc = b"\x1b]".to_vec();
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                osc.push(b';');
            }
            osc.extend_from_slice(p);
        }
        osc.extend_from_slice(b"\x07");
        self.pending.push(osc);
    }

    fn unhandled_csi(
        &mut self,
        _: &mut Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        // Kitty-keyboard push (`\x1b[>{n}u`) and pop (`\x1b[<{n}u`) are
        // NOT forwarded to the outer terminal. The outer terminal is
        // shared across panes; if one agent flips it into kitty mode,
        // every other pane (a shell, a pre-mount agent, a different
        // agent that does not use kitty keys) starts receiving
        // operator keystrokes in `\x1b[<code>;<mod>u` form and
        // surfaces them as garbage at the prompt. The agent's own
        // vt100 still parses kitty key sequences inside its own
        // screen state — we just keep the outer terminal in plain
        // CSI mode so shells stay sane. Trade-off: an operator
        // typing Shift+Enter into a backgrounded agent sees plain
        // Enter; that is acceptable next to "Shell prints
        // `t16;1:3u` when I type t".
        if c == 'u' && matches!(i1, Some(b'>') | Some(b'<')) {
            return;
        }
        // Re-emit verbatim. vt100 routes here only for CSI sequences
        // it does not itself handle — `modifyOtherKeys`
        // (`\x1b[>4;{n}m`), synchronised-output (`\x1b[?2026h/l`),
        // and any other extension the outer terminal understands but
        // `vt100` does not.
        let mut buf = b"\x1b[".to_vec();
        if let Some(b) = i1 {
            buf.push(b);
        }
        if let Some(b) = i2 {
            buf.push(b);
        }
        for (idx, sub) in params.iter().enumerate() {
            if idx > 0 {
                buf.push(b';');
            }
            for (jdx, n) in sub.iter().enumerate() {
                if jdx > 0 {
                    buf.push(b':');
                }
                let _ = write!(buf, "{}", n);
            }
        }
        let mut tmp = [0u8; 4];
        buf.extend_from_slice(c.encode_utf8(&mut tmp).as_bytes());
        self.pending.push(buf);
    }
}

pub struct Session {
    pub label: String,
    pub agent: Option<String>,
    pub state: AgentState,
    pub parser: vt100::Parser<OscCapture>,
    pub input_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub last_output_at: std::time::Instant,
    pub alive: bool,
    /// Current scrollback view offset in lines from the live tail.
    /// `0` = following live output; `> 0` = paused, looking back.
    /// `vt100::Screen::set_scrollback` mirrors this value so
    /// `screen().cell(r, c)` returns the right slice during render.
    pub scrollback_offset: usize,
    /// Most recently observed value of `Screen::bracketed_paste()`.
    /// The daemon compares this to the post-feed state to detect
    /// transitions, then re-emits the matching `\x1b[?2004h/l`
    /// sequence to the attached client so the outer terminal wraps
    /// pastes with `\x1b[200~`/`\x1b[201~` markers. Without this,
    /// vt100 silently consumes the agent's `?2004h` and outer
    /// terminals never wrap pastes — multi-line clipboard content
    /// then arrives one `\n`-terminated chunk at a time, which agents
    /// treat as multiple separate messages.
    pub bracketed_paste_active: bool,
    /// `true` once the PTY has produced any output. Stays `false`
    /// during the brief window between `Session::spawn` and the
    /// child's first write — when the parser's cursor sits at (0, 0)
    /// of a blank primary screen with no agent UI drawn yet. The
    /// daemon gates `\x1b[?25h` (cursor visible) on this so a
    /// freshly-split pane does not paint a stray blinking cursor
    /// inside an otherwise empty rectangle.
    pub received_output: bool,
}

pub enum SessionEvent {
    Output { session_id: u64, data: Vec<u8> },
    Exited { session_id: u64 },
}

impl Session {
    pub fn spawn(
        label: impl Into<String>,
        agent: Option<String>,
        cmd: CommandBuilder,
        rows: u16,
        cols: u16,
        event_tx: mpsc::UnboundedSender<SessionEvent>,
    ) -> Result<(Self, u64)> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open PTY")?;

        let master = pair.master;
        let slave = pair.slave;

        let child = slave
            .spawn_command(cmd)
            .context("failed to spawn session process")?;
        drop(slave);

        let master: Arc<Mutex<Box<dyn MasterPty + Send>>> = Arc::new(Mutex::new(master));
        let master_for_read = Arc::clone(&master);
        let master_for_write = Arc::clone(&master);

        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        tokio::task::spawn_blocking(move || {
            let mut writer = master_for_write
                .lock()
                .unwrap()
                .take_writer()
                .expect("failed to get PTY writer");
            let rt = tokio::runtime::Handle::current();
            while let Some(data) = rt.block_on(input_rx.recv()) {
                let _ = std::io::Write::write_all(&mut writer, &data);
            }
        });

        let event_tx_output = event_tx.clone();
        let sid = next_id();
        tokio::task::spawn_blocking(move || {
            let mut reader = master_for_read
                .lock()
                .unwrap()
                .try_clone_reader()
                .expect("failed to clone PTY reader");
            let mut buf = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) => {
                        eprintln!("[jackin-container] session {sid}: PTY read EOF");
                        break;
                    }
                    Err(e) => {
                        eprintln!(
                            "[jackin-container] session {sid}: PTY read error: {e} (errno={:?})",
                            e.raw_os_error()
                        );
                        break;
                    }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if event_tx_output
                            .send(SessionEvent::Output {
                                session_id: sid,
                                data,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
            let _ = event_tx_output.send(SessionEvent::Exited { session_id: sid });
            drop(child);
        });

        Ok((
            Session {
                label: label.into(),
                agent,
                state: AgentState::Working,
                parser: vt100::Parser::new_with_callbacks(
                    rows,
                    cols,
                    SCROLLBACK_LEN,
                    OscCapture::default(),
                ),
                input_tx,
                pty_master: master,
                last_output_at: std::time::Instant::now(),
                alive: true,
                scrollback_offset: 0,
                bracketed_paste_active: false,
                received_output: false,
            },
            sid,
        ))
    }

    /// Scroll the view by `delta` lines. Positive = scroll up (into
    /// history); negative = scroll down (toward live tail).
    ///
    /// Up-scroll is clamped to the **actual filled scrollback** at
    /// call time. Without this clamp, scrolling past the top would
    /// silently inflate `scrollback_offset` while vt100 clamped
    /// itself to the filled count — and subsequent down-scrolls
    /// would have to chew through the phantom distance before the
    /// visible view moved. Operator's symptom was "I scrolled too
    /// far up, scrolling back down doesn't react for a while."
    pub fn scroll_by(&mut self, delta: i32) {
        let new = if delta > 0 {
            let filled = self.scrollback_filled();
            self.scrollback_offset
                .saturating_add(delta as usize)
                .min(filled)
        } else {
            self.scrollback_offset.saturating_sub((-delta) as usize)
        };
        self.scrollback_offset = new;
        self.parser.screen_mut().set_scrollback(new);
    }

    /// Drop scrollback view, return to the live tail.
    pub fn scroll_to_live(&mut self) {
        if self.scrollback_offset != 0 {
            self.scrollback_offset = 0;
            self.parser.screen_mut().set_scrollback(0);
        }
    }

    /// Number of scrollback lines currently filled in the primary
    /// grid. Probed by setting the scrollback to `usize::MAX` — vt100
    /// clamps it to the actual filled count, which we read back via
    /// `Screen::scrollback`. The saved offset is restored so this is
    /// safe to call from a render path.
    ///
    /// Returns `0` while the alternate screen is active, because alt
    /// grid has no scrollback by design — the agent owns the whole
    /// surface and there is nothing for the operator to scroll into.
    pub fn scrollback_filled(&mut self) -> usize {
        if self.parser.screen().alternate_screen() {
            return 0;
        }
        let saved = self.parser.screen().scrollback();
        self.parser.screen_mut().set_scrollback(usize::MAX);
        let filled = self.parser.screen().scrollback();
        self.parser.screen_mut().set_scrollback(saved);
        filled
    }

    pub fn send_input(&self, data: &[u8]) {
        let _ = self.input_tx.send(data.to_vec());
    }

    /// True when the session's program has enabled an SGR mouse
    /// protocol (any variant past `None`). Used by the daemon to decide
    /// whether to forward a mouse press to the PTY: forwarding to a
    /// program that did not opt in (a shell prompt, an agent before its
    /// TUI mounts) leaks the raw SGR bytes as visible text — the
    /// operator sees `;col;rowM` garbage at the prompt.
    pub fn mouse_enabled(&self) -> bool {
        !matches!(
            self.parser.screen().mouse_protocol_mode(),
            vt100::MouseProtocolMode::None
        )
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Feed PTY bytes into the VT parser and update activity timestamps.
    pub fn feed_pty(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.received_output = true;
        }
        self.parser.process(bytes);
        self.last_output_at = std::time::Instant::now();
        self.state = AgentState::Working;
    }

    /// Drain the OSC / unhandled-CSI byte sequences the parser captured
    /// during the last `feed_pty` call. The daemon forwards these to
    /// the attached client only when this session owns the focused
    /// pane in the active tab — see `OscCapture` for the routing
    /// rationale.
    pub fn drain_passthrough(&mut self) -> Vec<Vec<u8>> {
        self.parser.callbacks_mut().drain()
    }

    /// Compare current vt100 mode state against the last observed
    /// snapshot and produce the matching `?<mode>h/l` byte sequences
    /// for any transitions. Used by the daemon to keep the outer
    /// terminal's mode state in sync with the focused agent's
    /// requests — currently bracketed paste, which vt100 absorbs
    /// silently otherwise and which breaks multi-line paste UX when
    /// the outer terminal stops wrapping clipboard content.
    pub fn drain_mode_transitions(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let cur_bracketed = self.parser.screen().bracketed_paste();
        if cur_bracketed != self.bracketed_paste_active {
            out.push(if cur_bracketed {
                b"\x1b[?2004h".to_vec()
            } else {
                b"\x1b[?2004l".to_vec()
            });
            self.bracketed_paste_active = cur_bracketed;
        }
        out
    }

    /// Snapshot of every mode the daemon should restore on the
    /// outer terminal when an attach client connects. Mirrors the
    /// "what does the agent currently want?" set so a reattach
    /// looks identical to a brand-new attach.
    pub fn current_mode_state(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        if self.parser.screen().bracketed_paste() {
            out.push(b"\x1b[?2004h".to_vec());
        }
        out
    }

    pub fn title(&self) -> Option<&str> {
        self.parser.callbacks().title.as_deref()
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        if let Ok(master) = self.pty_master.lock() {
            let _ = master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
        self.parser.screen_mut().set_size(rows, cols);
    }

    pub fn refresh_state(&mut self) {
        if !self.alive {
            if self.state == AgentState::Working || self.state == AgentState::Blocked {
                self.state = AgentState::Done;
            }
            return;
        }
        let elapsed = self.last_output_at.elapsed();
        self.state = if elapsed < std::time::Duration::from_secs(3) {
            AgentState::Working
        } else {
            AgentState::Blocked
        };
    }
}

/// Read the list of available agent slugs from the `JACKIN_SUPPORTED_AGENTS`
/// environment variable injected by the derived image build.
pub fn available_agents() -> Vec<String> {
    std::env::var("JACKIN_SUPPORTED_AGENTS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// Build a CommandBuilder for an agent session.
/// Entrypoint is `/jackin/runtime/entrypoint.sh` with `JACKIN_AGENT=<slug>`.
pub fn build_agent_command(agent: &str, env_passthrough: &[(String, String)]) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("/jackin/runtime/entrypoint.sh");
    cmd.env("JACKIN_AGENT", agent);
    for (k, v) in env_passthrough {
        cmd.env(k, v);
    }
    cmd.env("TERM", "xterm-256color");
    cmd
}

/// Build a CommandBuilder for an interactive shell session.
pub fn build_shell_command() -> CommandBuilder {
    let mut cmd = CommandBuilder::new("/bin/zsh");
    cmd.env("TERM", "xterm-256color");
    cmd
}

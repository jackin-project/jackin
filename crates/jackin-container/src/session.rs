/// PTY session: one PTY + one `vt100::Parser` + state-inference timer.
///
/// Each session owns a PTY pair, a child process (agent or shell), and
/// the `vt100::Parser` whose `Screen` mirrors the agent's view. The
/// parser is the source of truth for re-rendering on tab switch, pane
/// switch, and client reattach.
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;

use crate::protocol::AgentState;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub struct Session {
    pub label: String,
    pub agent: Option<String>,
    pub state: AgentState,
    pub parser: vt100::Parser,
    pub input_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub last_output_at: std::time::Instant,
    pub alive: bool,
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
                parser: vt100::Parser::new(rows, cols, 0),
                input_tx,
                pty_master: master,
                last_output_at: std::time::Instant::now(),
                alive: true,
            },
            sid,
        ))
    }

    pub fn send_input(&self, data: &[u8]) {
        let _ = self.input_tx.send(data.to_vec());
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Feed PTY bytes into the VT parser and update activity timestamps.
    pub fn feed_pty(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
        self.last_output_at = std::time::Instant::now();
        self.state = AgentState::Working;
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

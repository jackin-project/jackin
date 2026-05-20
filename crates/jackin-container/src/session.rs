use std::sync::atomic::{AtomicU64, Ordering};
/// PTY session management — one session per pane leaf.
///
/// Each session owns a PTY pair and a child process (agent or shell).
/// Output is captured into a VirtualTerminal so we can re-render on
/// session switch without re-running the process.
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;

use crate::protocol::AgentState;
use crate::terminal::VirtualTerminal;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub struct Session {
    pub label: String,
    pub agent: Option<String>,
    pub state: AgentState,
    pub vterminal: VirtualTerminal,
    /// Writes to this sender go to the PTY master (agent stdin).
    pub input_tx: mpsc::UnboundedSender<Vec<u8>>,
    /// PTY master handle — used to resize.
    pub pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    /// Last observed output timestamp — used for state inference.
    pub last_output_at: std::time::Instant,
    /// Whether the PTY child is still alive.
    pub alive: bool,
}

/// Event from a session's PTY output loop back to the multiplexer.
pub enum SessionEvent {
    Output { session_id: u64, data: Vec<u8> },
    Exited { session_id: u64 },
}

impl Session {
    /// Spawn a new session running `cmd` inside the given PTY dimensions.
    ///
    /// Returns the session and a receiver for `SessionEvent`s.
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

        // Spawn the child process in the slave side of the PTY.
        let child = slave
            .spawn_command(cmd)
            .context("failed to spawn session process")?;
        drop(slave);

        let master: Arc<Mutex<Box<dyn MasterPty + Send>>> = Arc::new(Mutex::new(master));
        let master_for_read = Arc::clone(&master);
        let master_for_write = Arc::clone(&master);

        // Input channel: multiplexer → PTY master.
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        // Writer task: forward input_rx → PTY master stdin.
        tokio::task::spawn_blocking(move || {
            let mut writer = master_for_write
                .lock()
                .unwrap()
                .take_writer()
                .expect("failed to get PTY writer");
            let rt = tokio::runtime::Handle::current();
            loop {
                let Some(data) = rt.block_on(input_rx.recv()) else {
                    break;
                };
                let _ = std::io::Write::write_all(&mut writer, &data);
            }
        });

        // Reader task: PTY master stdout → SessionEvent::Output.
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
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        let _ = event_tx_output.send(SessionEvent::Output {
                            session_id: sid,
                            data,
                        });
                    }
                }
            }
            let _ = event_tx_output.send(SessionEvent::Exited { session_id: sid });
            drop(child); // ensure child is waited on
        });

        Ok((
            Session {
                label: label.into(),
                agent,
                state: AgentState::Working,
                vterminal: VirtualTerminal::new(rows, cols),
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

    pub fn resize(&self, rows: u16, cols: u16) {
        if let Ok(master) = self.pty_master.lock() {
            let _ = master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    /// Send SIGWINCH to force the child to redraw after a switch.
    pub fn force_redraw(&self) {
        if let Ok(master) = self.pty_master.lock() {
            let size = master.get_size().unwrap_or(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            });
            let _ = master.resize(size);
        }
    }

    /// Infer state from output activity.
    /// - Recent output (< 3 s) → Working
    /// - No output > 3 s → Blocked
    /// - State only moves to Done/Idle via explicit operator action.
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

/// Read the list of available agent slugs from the JACKIN_SUPPORTED_AGENTS
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

/// Build a CommandBuilder for an agent session or a shell fallback.
/// The entrypoint is always `/jackin/runtime/entrypoint.sh` with
/// `JACKIN_AGENT=<slug>` set in the environment.
pub fn build_agent_command(agent: &str, env_passthrough: &[(String, String)]) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("/jackin/runtime/entrypoint.sh");
    cmd.env("JACKIN_AGENT", agent);
    for (k, v) in env_passthrough {
        cmd.env(k, v);
    }
    // Prevent nested-multiplexer warnings from agent CLIs that check TERM.
    cmd.env("TERM", "xterm-256color");
    cmd
}

/// Build a CommandBuilder for an interactive shell session.
pub fn build_shell_command() -> CommandBuilder {
    CommandBuilder::new("/bin/zsh")
}

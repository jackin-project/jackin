// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    OscPolicy, SCROLLBACK_LEN, Session, SessionEvent, SessionSpawnSpec, SessionTerminal,
    capture_pty_fixture_bytes, child_exit_reason, emit_pty_exit, emit_pty_spawn, inject_status_env,
    lock_or_record_poison, next_id, record_terminal_bytes,
};

use crate::agent_status::SessionStatus;
use crate::protocol::AgentState;
use anyhow::{Context, Result};
use jackin_telemetry::ResultTelemetryExt as _;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

pub fn validate_spawn_token_syntax(raw: &str) -> Result<&str, &'static str> {
    if raw.is_empty() {
        return Err("empty value");
    }
    if raw.starts_with('-') {
        return Err("looks like a flag");
    }
    if raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("contains whitespace or control characters");
    }
    Ok(raw)
}

impl Session {
    #[expect(
        clippy::excessive_nesting,
        reason = "Session spawn wires PTY + child handle + agent + env into the \
              multiplexer state. The nested `is_err` + governed INFO event + state- \
              update branches are the per-stage error-reporting protocol."
    )]
    #[expect(
        clippy::too_many_lines,
        reason = "Same justification as the too_many_lines + excessive_nesting \
          allows: session spawn wires PTY + child handle + agent + env into \
          the multiplexer state. Inline shape preserves captured-runtime \
          state across the per-stage error-reporting branches."
    )]
    pub fn spawn(
        spec: SessionSpawnSpec,
        mut cmd: CommandBuilder,
        terminal: SessionTerminal,
        event_tx: mpsc::UnboundedSender<SessionEvent>,
    ) -> Result<(Self, u64)> {
        let SessionSpawnSpec {
            label,
            agent,
            account_id,
            identity,
            provider,
            cache_dir,
        } = spec;
        let conversation_id = agent.as_ref().map(|_| uuid::Uuid::new_v4().to_string());
        let rows = terminal.rows;
        let cols = terminal.cols;
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

        let sid = next_id();
        let control_capability = uuid::Uuid::new_v4().to_string();
        inject_status_env(
            &mut cmd,
            sid,
            agent.as_deref(),
            cache_dir.as_deref(),
            &control_capability,
        );

        let mut child = slave
            .spawn_command(cmd)
            .context("failed to spawn session process")?;
        let child_pid = child.process_id();
        if let Some(pid) = child_pid {
            crate::pid1::register_managed_child(pid);
        }
        let child_killer = Arc::new(Mutex::new(child.clone_killer()));
        let termination_requested = Arc::new(AtomicBool::new(false));
        drop(slave);

        let master: Arc<Mutex<Box<dyn MasterPty + Send>>> = Arc::new(Mutex::new(master));
        let master_for_read = Arc::clone(&master);
        let master_for_write = Arc::clone(&master);

        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();

        let event_tx_output = event_tx.clone();
        let event_tx_exit = event_tx.clone();
        let event_tx_writer_err = event_tx.clone();
        emit_pty_spawn(agent.as_deref(), conversation_id.as_deref());

        jackin_telemetry::spawn::stream_blocking("pty.reader", move || {
            let writer = match lock_or_record_poison(&master_for_write) {
                None => None,
                Some(guard) => guard
                    .take_writer()
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)
                    .ok(),
            };
            let Some(mut writer) = writer else {
                drop(event_tx_writer_err.send(SessionEvent::Exited {
                    session_id: sid,
                    reason: Some("session PTY writer failed to initialize".to_owned()),
                }));
                return;
            };
            while let Some(data) = input_rx.blocking_recv() {
                if std::io::Write::write_all(&mut writer, &data)
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)
                    .is_err()
                {
                    drop(event_tx_writer_err.send(SessionEvent::Exited {
                        session_id: sid,
                        reason: Some("session PTY write failed".to_owned()),
                    }));
                    return;
                }
                record_terminal_bytes(
                    jackin_telemetry::schema::enums::StreamDirection::Input,
                    data.len(),
                );
            }
        });

        let event_tx_reader_err = event_tx.clone();
        jackin_telemetry::spawn::stream_blocking("pty.writer", move || {
            let reader = match lock_or_record_poison(&master_for_read) {
                None => None,
                Some(guard) => guard
                    .try_clone_reader()
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)
                    .ok(),
            };
            let Some(mut reader) = reader else {
                drop(event_tx_reader_err.send(SessionEvent::Exited {
                    session_id: sid,
                    reason: Some("session PTY reader failed to initialize".to_owned()),
                }));
                return;
            };
            let mut buf = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) => break,
                    Err(error) => {
                        drop(Err::<(), _>(error).record_telemetry_error(
                            jackin_telemetry::schema::enums::ErrorType::IoError,
                        ));
                        break;
                    }
                    Ok(n) => {
                        record_terminal_bytes(
                            jackin_telemetry::schema::enums::StreamDirection::Output,
                            n,
                        );
                        capture_pty_fixture_bytes(&buf[..n]);
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
        });

        let exit_agent = agent.clone();
        let exit_conversation_id = conversation_id.clone();
        let exit_termination_requested = Arc::clone(&termination_requested);
        jackin_telemetry::spawn::stream_blocking("pty.wait", move || {
            let status = child.wait();
            emit_pty_exit(
                exit_agent.as_deref(),
                exit_conversation_id.as_deref(),
                status.as_ref(),
                exit_termination_requested.load(Ordering::Acquire),
            );
            if let Some(pid) = child_pid {
                crate::pid1::unregister_managed_child(pid);
                crate::pid1::reap_zombies();
            }
            drop(event_tx_exit.send(SessionEvent::Exited {
                session_id: sid,
                reason: child_exit_reason(status.as_ref()),
            }));
        });

        Ok((
            Session {
                label,
                agent,
                account_id,
                usage_capability: None,
                identity,
                control_capability,
                conversation_id,
                provider,
                state: AgentState::Unknown,
                status: SessionStatus::new(),
                pending_transition: crate::agent_status::policy::PendingTransition::default(),
                status_transition_times: std::collections::VecDeque::new(),
                status_flapping: false,
                gate_states: std::collections::HashMap::new(),
                authority: None,
                subagents_active: 0,
                child_pid,
                cpu_sample: None,
                saw_agent_foreground: false,
                osc: crate::agent_status::evidence::OscEvidence::default(),
                osc_status_decoder: crate::agent_status::OscStatusDecoder::default(),
                input_tx,
                pty_master: master,
                child_killer,
                termination_requested,
                last_output_at: std::time::Instant::now(),
                last_input_at: std::time::Instant::now(),
                received_output: false,
                shadow_grid: {
                    let mut grid = Box::new(termpane::DamageGrid::with_row_arena(
                        rows,
                        cols,
                        SCROLLBACK_LEN,
                        terminal.row_arena,
                    ));
                    grid.set_reported_colors(terminal.default_fg, terminal.default_bg);
                    grid
                },
                osc_policy: OscPolicy::from_env(),
                title: None,
                icon_name: None,
                cwd: None,
                pending_passthrough: Vec::new(),
                modify_other_keys: None,
            },
            sid,
        ))
    }
}

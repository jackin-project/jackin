// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn start_fault_pty_tasks(
    fault: FaultMasterPty,
) -> (
    mpsc::UnboundedSender<Vec<u8>>,
    mpsc::UnboundedReceiver<SessionEvent>,
) {
    let master: Arc<Mutex<Box<dyn MasterPty + Send>>> = Arc::new(Mutex::new(Box::new(fault)));
    let master_for_write = Arc::clone(&master);
    let master_for_read = Arc::clone(&master);
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (event_tx, event_rx) = mpsc::unbounded_channel::<SessionEvent>();
    let sid = 1u64;
    let event_tx_writer_err = event_tx.clone();
    tokio::task::spawn_blocking(move || {
        let writer = master_for_write
            .lock()
            .ok()
            .and_then(|guard| guard.take_writer().ok());
        let Some(mut writer) = writer else {
            drop(event_tx_writer_err.send(SessionEvent::Exited {
                session_id: sid,
                reason: Some("session PTY writer failed to initialize".to_owned()),
            }));
            return;
        };
        while let Some(data) = input_rx.blocking_recv() {
            if let Err(e) = std::io::Write::write_all(&mut writer, &data) {
                drop(event_tx_writer_err.send(SessionEvent::Exited {
                    session_id: sid,
                    reason: Some(format!("session PTY write failed: {e}")),
                }));
                return;
            }
        }
    });
    let event_tx_reader_err = event_tx.clone();
    tokio::task::spawn_blocking(move || {
        let reader = master_for_read
            .lock()
            .ok()
            .and_then(|guard| guard.try_clone_reader().ok());
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
                Ok(_) => {}
                Err(_) => break, // read error: no Exited (reaper is authoritative)
            }
        }
    });
    (input_tx, event_rx)
}

pub(super) fn v2_credentials_fixture() -> jackin_protocol::AgentCredentialEnv {
    serde_json::from_value(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "opencode-personal": {
                "agent": "opencode",
                "account_id": "acc-personal",
                "env": {"ANTHROPIC_API_KEY": "personal-secret"},
            },
            "claude-work": {
                "agent": "claude",
                "account_id": "acc-work",
                "env": {"ANTHROPIC_API_KEY": "work-secret"},
            },
            "claude-personal": {
                "agent": "claude",
                "account_id": "acc-personal",
                "env": {"ANTHROPIC_API_KEY": "personal-secret"},
            },
        },
    }))
    .expect("v2 fixture must decode")
}

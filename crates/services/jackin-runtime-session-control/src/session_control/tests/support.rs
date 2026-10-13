// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn record(
    session: u64,
    state: AgentState,
    kind: SessionEventKind,
) -> SessionEventRecord {
    SessionEventRecord {
        seq: 0,
        session,
        agent: Some("claude".to_owned()),
        account_id: Some("acc-1".to_owned()),
        state,
        last_output_ms: Some(10),
        last_input_ms: Some(20),
        kind,
    }
}

pub(super) fn events_from(
    records: Vec<Result<SessionEventRecord>>,
) -> (SessionEvents, mpsc::Sender<Result<SessionEventRecord>>) {
    let (tx, rx) = mpsc::channel();
    for entry in records {
        tx.send(entry).expect("seed the event channel");
    }
    (
        SessionEvents {
            records: rx,
            transport: ControlTransport::DirectSocket,
            child: None,
            operation: None,
        },
        tx,
    )
}

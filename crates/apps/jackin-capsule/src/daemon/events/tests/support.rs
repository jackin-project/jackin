// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn observation(session: u64, state: AgentState) -> SessionObservation {
    let now = Instant::now();
    SessionObservation {
        session,
        agent: Some("claude".to_owned()),
        account_id: Some("acc-1".to_owned()),
        state,
        last_output_at: Some(now),
        last_input_at: Some(now),
    }
}

pub(super) fn record(msg: ServerMsg) -> SessionEventRecord {
    match msg {
        ServerMsg::SessionEvent { event } => *event,
        other => panic!("expected a session event, got {}", other.kind()),
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use jackin_protocol::control::{
    AgentState, ServerMsg, SessionEventKind, SessionEventRecord, SessionSendRejection,
};
use tokio::sync::mpsc;

use super::{ClientMsg, Multiplexer, Session, control_server_operation};

const RPC_ERROR: jackin_telemetry::schema::enums::ErrorType =
    jackin_telemetry::schema::enums::ErrorType::RpcError;

pub(crate) const ACTIVITY_REPORT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

struct Subscriber {
    filter: Option<u64>,

    seq: u64,
    tx: mpsc::UnboundedSender<ServerMsg>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionObservation {
    pub(crate) session: u64,
    pub(crate) agent: Option<String>,
    pub(crate) account_id: Option<String>,
    pub(crate) state: AgentState,
    pub(crate) last_output_at: Option<Instant>,
    pub(crate) last_input_at: Option<Instant>,
}

impl SessionObservation {
    fn into_record(self, seq: u64, now: Instant, kind: SessionEventKind) -> SessionEventRecord {
        let elapsed_ms = |at: Option<Instant>| {
            at.map(|at| {
                u64::try_from(now.saturating_duration_since(at).as_millis()).unwrap_or(u64::MAX)
            })
        };
        SessionEventRecord {
            seq,
            session: self.session,
            agent: self.agent,
            account_id: self.account_id,
            state: self.state,
            last_output_ms: elapsed_ms(self.last_output_at),
            last_input_ms: elapsed_ms(self.last_input_at),
            kind,
        }
    }
}

#[derive(Default)]
pub(crate) struct EventSubscribers {
    subscribers: Vec<Subscriber>,

    last_activity_report: std::collections::HashMap<u64, Instant>,
}

impl std::fmt::Debug for EventSubscribers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventSubscribers")
            .field("subscribers", &self.subscribers.len())
            .finish_non_exhaustive()
    }
}

impl EventSubscribers {
    pub(crate) fn is_empty(&self) -> bool {
        self.subscribers.is_empty()
    }

    pub(crate) fn subscribe(
        &mut self,
        filter: Option<u64>,
        tx: mpsc::UnboundedSender<ServerMsg>,
        now: Instant,
        baseline: impl IntoIterator<Item = SessionObservation>,
    ) {
        let mut subscriber = Subscriber { filter, seq: 0, tx };
        for observation in baseline {
            if !send_to(
                &mut subscriber,
                now,
                &observation,
                &SessionEventKind::Subscribed,
            ) {
                return;
            }
        }
        self.subscribers.push(subscriber);
    }

    pub(crate) fn publish(
        &mut self,
        now: Instant,
        observation: &SessionObservation,
        kind: &SessionEventKind,
    ) {
        if matches!(kind, SessionEventKind::Exited { .. }) {
            self.last_activity_report.remove(&observation.session);
        }
        self.subscribers
            .retain_mut(|subscriber| send_to(subscriber, now, observation, kind));
    }

    pub(crate) fn publish_activity(&mut self, now: Instant, observation: &SessionObservation) {
        let due = self
            .last_activity_report
            .get(&observation.session)
            .is_none_or(|last| now.saturating_duration_since(*last) >= ACTIVITY_REPORT_INTERVAL);
        if !due {
            return;
        }
        self.last_activity_report.insert(observation.session, now);
        self.publish(now, observation, &SessionEventKind::Activity);
    }
}

fn send_to(
    subscriber: &mut Subscriber,
    now: Instant,
    observation: &SessionObservation,
    kind: &SessionEventKind,
) -> bool {
    if subscriber
        .filter
        .is_some_and(|id| id != observation.session)
    {
        return !subscriber.tx.is_closed();
    }
    let record = observation
        .clone()
        .into_record(subscriber.seq, now, kind.clone());
    subscriber.seq = subscriber.seq.saturating_add(1);
    subscriber
        .tx
        .send(ServerMsg::SessionEvent {
            event: Box::new(record),
        })
        .is_ok()
}

pub(super) fn handle_control_subscription(
    mux: &mut Multiplexer,
    ctx: &jackin_protocol::TelemetryContext,
    msg: &ClientMsg,
    tx: mpsc::UnboundedSender<ServerMsg>,
) {
    let Ok(operation) = control_server_operation(ctx, msg) else {
        let _error = jackin_telemetry::record_error(RPC_ERROR);
        return;
    };
    let ClientMsg::Events { session } = *msg else {
        let _error = jackin_telemetry::record_error(RPC_ERROR);
        if let Some(operation) = operation {
            operation.complete(
                jackin_telemetry::schema::enums::OutcomeValue::Failure,
                Some(RPC_ERROR),
            );
        }
        return;
    };
    let now = Instant::now();
    let baseline: Vec<_> = mux
        .session_supervisor
        .sessions
        .iter()
        .filter(|(id, _)| session.is_none_or(|wanted| wanted == *id))
        .map(|(id, s)| session_observation(id, s))
        .collect();
    mux.control
        .event_subscribers
        .subscribe(session, tx, now, baseline);

    if let Some(operation) = operation {
        operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    }
}

pub(super) fn session_observation(id: u64, session: &Session) -> SessionObservation {
    SessionObservation {
        session: id,
        agent: session.agent.clone(),
        account_id: session.account_id.clone(),
        state: session.state,
        last_output_at: Some(session.last_output_at),
        last_input_at: Some(session.last_input_at),
    }
}

pub(super) fn publish_status_events(
    mux: &mut Multiplexer,
    states_before: &[(u64, AgentState)],
    states_after: &[(u64, AgentState)],
    now: Instant,
) {
    if mux.control.event_subscribers.is_empty() {
        return;
    }
    let previous_state = |id: u64| {
        states_before
            .iter()
            .find_map(|(before_id, state)| (*before_id == id).then_some(*state))
    };
    for (id, state) in states_after {
        let Some(session) = mux.session_supervisor.sessions.get(*id) else {
            continue;
        };
        let observation = session_observation(*id, session);

        if let Some(previous) = previous_state(*id).filter(|previous| previous != state) {
            mux.control.event_subscribers.publish(
                now,
                &observation,
                &SessionEventKind::StateChanged { previous },
            );
        }

        if now.saturating_duration_since(session.last_output_at) <= ACTIVITY_REPORT_INTERVAL {
            mux.control
                .event_subscribers
                .publish_activity(now, &observation);
        }
    }
}

pub(crate) const fn session_send_reply(
    session: u64,
    outcome: Result<u64, SessionSendRejection>,
) -> ServerMsg {
    match outcome {
        Ok(bytes) => ServerMsg::SessionSent { session, bytes },
        Err(reason) => ServerMsg::SessionSendDenied { session, reason },
    }
}

#[cfg(test)]
mod tests;

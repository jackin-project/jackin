// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! The `events` subscription registry: the daemon-side fan-out that turns
//! session observations into [`ServerMsg::SessionEvent`] frames.
//!
//! Not responsible for: authoring agent state. The arbitrated effective state
//! is produced once by `Session::advance_status` (the terminal-observation
//! authority) and only *read* here. A stream that recomputed state would be a
//! second authority, and the two would drift.

use std::time::Instant;

use jackin_protocol::control::{
    AgentState, ServerMsg, SessionEventKind, SessionEventRecord, SessionSendRejection,
};
use tokio::sync::mpsc;

use super::{ClientMsg, Multiplexer, Session, control_server_operation};

const RPC_ERROR: jackin_telemetry::schema::enums::ErrorType =
    jackin_telemetry::schema::enums::ErrorType::RpcError;

/// Minimum gap between two [`SessionEventKind::Activity`] records for one
/// session. A chatty agent writes to its PTY thousands of times a second; the
/// stream reports *that* it is active on a human cadence and lets
/// [`SessionEventRecord::last_output_ms`] carry the precise recency. State
/// changes and exits are never throttled — those are the events a subscriber
/// is waiting on.
pub(crate) const ACTIVITY_REPORT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// One open `events` connection.
struct Subscriber {
    /// `Some(id)` restricts the stream to one session; `None` streams all.
    filter: Option<u64>,
    /// Per-subscription record counter, so a gap in `seq` is visible to the
    /// peer as a dropped record rather than silently swallowed.
    seq: u64,
    tx: mpsc::UnboundedSender<ServerMsg>,
}

/// Everything one session contributes to an event record, read from the
/// session at emit time. Keeping it a struct means adding a field to the wire
/// record is one compile error here, not a silently-defaulted value.
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

/// The daemon's set of live `events` subscriptions.
#[derive(Default)]
pub(crate) struct EventSubscribers {
    subscribers: Vec<Subscriber>,
    /// When each session last had an [`SessionEventKind::Activity`] record
    /// emitted, for [`ACTIVITY_REPORT_INTERVAL`] throttling.
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
    /// True while at least one peer is listening. The emit sites check this
    /// first so a container with no subscriber does no per-tick work beyond
    /// one boolean read.
    pub(crate) fn is_empty(&self) -> bool {
        self.subscribers.is_empty()
    }

    /// Register a new subscription and send it one
    /// [`SessionEventKind::Subscribed`] baseline record per live session, so
    /// the peer's first read is a complete picture rather than a wait for the
    /// next change.
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
                // The peer hung up between connect and baseline; drop it
                // rather than registering a dead sender.
                return;
            }
        }
        self.subscribers.push(subscriber);
    }

    /// Emit one record to every subscriber whose filter accepts the session.
    /// Subscribers whose receiver has been dropped are reaped here — that is
    /// the only unsubscribe path, because a control connection has no
    /// goodbye frame.
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

    /// Emit a throttled [`SessionEventKind::Activity`] record. Returns without
    /// emitting when this session already reported activity inside
    /// [`ACTIVITY_REPORT_INTERVAL`].
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

/// Write one record to one subscriber. Returns false when the peer is gone and
/// the subscriber should be reaped.
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
        // Not this subscriber's session; it stays registered and its `seq`
        // stays dense (a filtered stream must not show phantom gaps).
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

/// Register a long-lived control subscription. `events` is the only one; any
/// other message arriving on a stream channel is a client that mis-declared
/// itself, and gets nothing — dropping `tx` closes the connection rather than
/// leaving the peer blocked on a frame that will never come.
pub(super) fn handle_control_subscription(
    mux: &mut Multiplexer,
    ctx: &jackin_protocol::TelemetryContext,
    msg: &ClientMsg,
    tx: mpsc::UnboundedSender<ServerMsg>,
) {
    // A rejected trace correlation refuses the subscription outright, exactly
    // as it refuses a query: dropping `tx` closes the connection.
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
    // The subscription itself succeeded here; the stream's own lifetime is the
    // connection task's, not this operation's.
    if let Some(operation) = operation {
        operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    }
}

/// Read everything the event stream reports about one session. The effective
/// state is taken as-is from the session — `Session::advance_status` is the
/// sole authority for it, and the stream must never author a second opinion.
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

/// Fan the status tick's outcome out to `events` subscribers.
///
/// Diffing the before/after state vectors — rather than emitting from inside
/// `advance_status` — is what makes the stream agree with `Status` and
/// `Snapshot` by construction: whatever changed `Session::state` during this
/// tick is reported, whether that was arbitration or the focused-pane
/// acknowledge that follows it. There is one authority for state and one place
/// that observes it.
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
        // A session that appeared during this tick has no "before" entry; its
        // baseline is the `Subscribed` record a new subscriber already got, so
        // there is no transition to report for it here.
        if let Some(previous) = previous_state(*id).filter(|previous| previous != state) {
            mux.control.event_subscribers.publish(
                now,
                &observation,
                &SessionEventKind::StateChanged { previous },
            );
        }
        // Activity: PTY output inside the last report interval. Once a session
        // goes quiet this predicate stops holding, so an idle pane emits
        // nothing at all rather than a heartbeat forever.
        if now.saturating_duration_since(session.last_output_at) <= ACTIVITY_REPORT_INTERVAL {
            mux.control
                .event_subscribers
                .publish_activity(now, &observation);
        }
    }
}

/// Reply shape for a `session.send` that reached, or failed to reach, a PTY.
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

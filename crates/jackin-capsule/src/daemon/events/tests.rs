// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for the `events` subscription fan-out.
use super::*;

fn observation(session: u64, state: AgentState) -> SessionObservation {
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

fn record(msg: ServerMsg) -> SessionEventRecord {
    match msg {
        ServerMsg::SessionEvent { event } => *event,
        other => panic!("expected a session event, got {}", other.kind()),
    }
}

#[test]
fn subscribe_sends_one_baseline_record_per_live_session() {
    let mut subscribers = EventSubscribers::default();
    let (tx, mut rx) = mpsc::unbounded_channel();
    subscribers.subscribe(
        None,
        tx,
        Instant::now(),
        [
            observation(1, AgentState::Idle),
            observation(2, AgentState::Working),
        ],
    );

    let first = record(rx.try_recv().expect("baseline for session 1"));
    assert_eq!((first.seq, first.session), (0, 1));
    assert_eq!(first.kind, SessionEventKind::Subscribed);
    assert_eq!(first.state, AgentState::Idle);

    let second = record(rx.try_recv().expect("baseline for session 2"));
    assert_eq!((second.seq, second.session), (1, 2));
    assert_eq!(second.state, AgentState::Working);
    assert!(
        matches!(rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "no further records without an event"
    );
}

#[test]
fn state_change_reaches_subscribers_and_carries_the_previous_state() {
    let mut subscribers = EventSubscribers::default();
    let (tx, mut rx) = mpsc::unbounded_channel();
    subscribers.subscribe(None, tx, Instant::now(), []);

    subscribers.publish(
        Instant::now(),
        &observation(1, AgentState::Working),
        &SessionEventKind::StateChanged {
            previous: AgentState::Idle,
        },
    );

    let event = record(rx.try_recv().expect("state change delivered"));
    assert_eq!(event.state, AgentState::Working);
    assert_eq!(
        event.kind,
        SessionEventKind::StateChanged {
            previous: AgentState::Idle
        }
    );
}

#[test]
fn a_session_filter_hides_other_sessions_without_leaving_seq_gaps() {
    let mut subscribers = EventSubscribers::default();
    let (tx, mut rx) = mpsc::unbounded_channel();
    subscribers.subscribe(Some(2), tx, Instant::now(), []);

    let kind = SessionEventKind::StateChanged {
        previous: AgentState::Idle,
    };
    subscribers.publish(Instant::now(), &observation(1, AgentState::Working), &kind);
    subscribers.publish(Instant::now(), &observation(2, AgentState::Working), &kind);
    subscribers.publish(Instant::now(), &observation(2, AgentState::Blocked), &kind);

    let first = record(rx.try_recv().expect("session 2 event"));
    let second = record(rx.try_recv().expect("second session 2 event"));
    assert_eq!(first.session, 2);
    assert_eq!(second.session, 2);
    // Dense seq: the filtered-out session 1 event must not consume a number,
    // or every filtered subscriber would read a phantom drop.
    assert_eq!((first.seq, second.seq), (0, 1));
    assert!(
        matches!(rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "no third record for a filtered subscription"
    );
}

#[test]
fn activity_records_are_throttled_per_session() {
    let mut subscribers = EventSubscribers::default();
    let (tx, mut rx) = mpsc::unbounded_channel();
    subscribers.subscribe(None, tx, Instant::now(), []);

    let start = Instant::now();
    subscribers.publish_activity(start, &observation(1, AgentState::Working));
    // Well inside the interval: suppressed.
    subscribers.publish_activity(start, &observation(1, AgentState::Working));
    // A different session has its own budget.
    subscribers.publish_activity(start, &observation(2, AgentState::Working));
    // Past the interval: reported again.
    subscribers.publish_activity(
        start + ACTIVITY_REPORT_INTERVAL,
        &observation(1, AgentState::Working),
    );

    let sessions: Vec<u64> = std::iter::from_fn(|| rx.try_recv().ok())
        .map(|msg| record(msg).session)
        .collect();
    assert_eq!(sessions, vec![1, 2, 1]);
}

#[test]
fn state_changes_are_never_throttled_by_the_activity_budget() {
    let mut subscribers = EventSubscribers::default();
    let (tx, mut rx) = mpsc::unbounded_channel();
    subscribers.subscribe(None, tx, Instant::now(), []);

    let now = Instant::now();
    subscribers.publish_activity(now, &observation(1, AgentState::Idle));
    subscribers.publish(
        now,
        &observation(1, AgentState::Working),
        &SessionEventKind::StateChanged {
            previous: AgentState::Idle,
        },
    );
    subscribers.publish(
        now,
        &observation(1, AgentState::Idle),
        &SessionEventKind::StateChanged {
            previous: AgentState::Working,
        },
    );

    let kinds: Vec<&'static str> = std::iter::from_fn(|| rx.try_recv().ok())
        .map(|msg| record(msg).kind.label())
        .collect();
    assert_eq!(kinds, vec!["activity", "state_changed", "state_changed"]);
}

#[test]
fn a_hung_up_subscriber_is_reaped_on_the_next_publish() {
    let mut subscribers = EventSubscribers::default();
    let (tx, rx) = mpsc::unbounded_channel();
    subscribers.subscribe(None, tx, Instant::now(), []);
    assert!(!subscribers.is_empty());

    drop(rx);
    subscribers.publish(
        Instant::now(),
        &observation(1, AgentState::Working),
        &SessionEventKind::Activity,
    );
    assert!(
        subscribers.is_empty(),
        "a closed receiver is the only unsubscribe signal a control connection has"
    );
}

#[test]
fn exit_is_the_last_record_and_clears_the_activity_budget() {
    let mut subscribers = EventSubscribers::default();
    let (tx, mut rx) = mpsc::unbounded_channel();
    subscribers.subscribe(None, tx, Instant::now(), []);

    let now = Instant::now();
    subscribers.publish_activity(now, &observation(1, AgentState::Working));
    drop(rx.try_recv());
    subscribers.publish(
        now,
        &observation(1, AgentState::Idle),
        &SessionEventKind::Exited {
            reason: Some("exit status 1".to_owned()),
        },
    );
    let exit = record(rx.try_recv().expect("exit delivered"));
    assert_eq!(
        exit.kind,
        SessionEventKind::Exited {
            reason: Some("exit status 1".to_owned())
        }
    );
    // A recycled session id must not inherit the dead session's throttle.
    subscribers.publish_activity(now, &observation(1, AgentState::Working));
    assert_eq!(
        record(rx.try_recv().expect("fresh session reports immediately")).kind,
        SessionEventKind::Activity
    );
}

#[test]
fn observations_report_activity_recency_in_milliseconds() {
    let mut subscribers = EventSubscribers::default();
    let (tx, mut rx) = mpsc::unbounded_channel();
    subscribers.subscribe(None, tx, Instant::now(), []);

    let start = Instant::now();
    let mut observed = observation(1, AgentState::Working);
    observed.last_output_at = Some(start);
    observed.last_input_at = None;
    subscribers.publish(
        start + std::time::Duration::from_millis(250),
        &observed,
        &SessionEventKind::Activity,
    );

    let event = record(rx.try_recv().expect("activity delivered"));
    assert_eq!(event.last_output_ms, Some(250));
    assert_eq!(event.last_input_ms, None);
}

#[test]
fn session_send_reply_maps_both_outcomes() {
    assert!(matches!(
        session_send_reply(3, Ok(8)),
        ServerMsg::SessionSent {
            session: 3,
            bytes: 8
        }
    ));
    assert!(matches!(
        session_send_reply(3, Err(SessionSendRejection::WriterClosed)),
        ServerMsg::SessionSendDenied {
            session: 3,
            reason: SessionSendRejection::WriterClosed
        }
    ));
}

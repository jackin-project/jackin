// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for the host-side `session.send` / `events` client.
use super::*;

fn record(session: u64, state: AgentState, kind: SessionEventKind) -> SessionEventRecord {
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

/// Build a subscription whose records come from a plain channel, so the
/// waiting logic is testable without a container or a socket.
///
/// The returned sender is the live half: the caller binds it for the duration
/// of the test, because dropping it is what makes the stream read as *ended*
/// rather than merely quiet, and those are different outcomes here.
fn events_from(
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

#[test]
fn single_quote_survives_every_prompt_byte() {
    // The payload crosses a `sh -lc` boundary on the docker-exec transport; a
    // space, a newline, a `$`, or a quote must reach the PTY unchanged.
    assert_eq!(single_quote("go"), "'go'");
    assert_eq!(single_quote("ship it\r"), "'ship it\r'");
    assert_eq!(single_quote("cost is $5"), "'cost is $5'");
    assert_eq!(single_quote("it's fine"), r"'it'\''s fine'");
    assert_eq!(single_quote("`whoami`"), "'`whoami`'");
}

#[test]
fn send_exec_script_quotes_the_payload_and_never_the_session_id() {
    assert_eq!(
        send_exec_script(7, "review; rm -rf /"),
        "exec /jackin/runtime/jackin-capsule send 7 'review; rm -rf /'"
    );
}

#[test]
fn events_exec_script_passes_the_session_filter_through() {
    assert_eq!(
        events_exec_script(None),
        "exec /jackin/runtime/jackin-capsule events"
    );
    assert_eq!(
        events_exec_script(Some(3)),
        "exec /jackin/runtime/jackin-capsule events --session 3"
    );
}

#[test]
fn bytes_sent_reports_a_refusal_as_an_error_not_a_zero_write() {
    assert_eq!(
        bytes_sent(ServerMsg::SessionSent {
            session: 1,
            bytes: 4
        })
        .expect("accepted send"),
        4
    );
    let refused = bytes_sent(ServerMsg::SessionSendDenied {
        session: 9,
        reason: jackin_protocol::control::SessionSendRejection::UnknownSession,
    })
    .expect_err("a refused send must be an error");
    assert!(format!("{refused:#}").contains("no such session"));
    let unknown = bytes_sent(ServerMsg::Unknown).expect_err("an unknown reply is not a byte count");
    assert!(format!("{unknown:#}").contains("unknown ServerMsg variant"));
    let acked = bytes_sent(ServerMsg::Ack).expect_err("an Ack is not a byte count");
    assert!(format!("{acked:#}").contains("expected SessionSent"));
}

#[test]
fn wait_for_state_matches_the_working_transition() {
    let (mut events, _open) = events_from(vec![
        Ok(record(1, AgentState::Idle, SessionEventKind::Subscribed)),
        Ok(record(1, AgentState::Idle, SessionEventKind::Activity)),
        Ok(record(
            1,
            AgentState::Working,
            SessionEventKind::StateChanged {
                previous: AgentState::Idle,
            },
        )),
    ]);
    let hit = events
        .wait_for_state(
            1,
            AgentState::Working,
            Instant::now() + Duration::from_secs(5),
        )
        .expect("the Working transition is delivered");
    assert_eq!(hit.state, AgentState::Working);
}

#[test]
fn wait_for_state_accepts_a_baseline_that_already_reports_the_state() {
    // The agent may already be working by the time the subscription lands.
    // Only accepting transitions would hang on that race.
    let (mut events, _open) = events_from(vec![Ok(record(
        1,
        AgentState::Working,
        SessionEventKind::Subscribed,
    ))]);
    let hit = events
        .wait_for_state(
            1,
            AgentState::Working,
            Instant::now() + Duration::from_secs(5),
        )
        .expect("a baseline in the target state counts as reaching it");
    assert_eq!(hit.kind, SessionEventKind::Subscribed);
}

#[test]
fn wait_for_state_ignores_other_sessions() {
    let (mut events, _open) = events_from(vec![
        Ok(record(
            2,
            AgentState::Working,
            SessionEventKind::StateChanged {
                previous: AgentState::Idle,
            },
        )),
        Ok(record(
            1,
            AgentState::Working,
            SessionEventKind::StateChanged {
                previous: AgentState::Idle,
            },
        )),
    ]);
    let hit = events
        .wait_for_state(
            1,
            AgentState::Working,
            Instant::now() + Duration::from_secs(5),
        )
        .expect("the addressed session's transition wins");
    assert_eq!(hit.session, 1);
}

#[test]
fn wait_for_state_fails_fast_when_the_session_exits_first() {
    let (mut events, _open) = events_from(vec![Ok(record(
        1,
        AgentState::Idle,
        SessionEventKind::Exited {
            reason: Some("exit status 1".to_owned()),
        },
    ))]);
    let error = events
        .wait_for_state(
            1,
            AgentState::Working,
            Instant::now() + Duration::from_secs(5),
        )
        .expect_err("an exited session can never reach Working");
    let message = format!("{error:#}");
    assert!(
        message.contains("exited before reaching working"),
        "{message}"
    );
    assert!(message.contains("exit status 1"), "{message}");
}

#[test]
fn wait_for_state_times_out_rather_than_blocking_forever() {
    let (mut events, _open) = events_from(vec![Ok(record(
        1,
        AgentState::Idle,
        SessionEventKind::Activity,
    ))]);
    let error = events
        .wait_for_state(
            1,
            AgentState::Working,
            Instant::now() + Duration::from_millis(50),
        )
        .expect_err("a quiet stream must time out");
    assert!(format!("{error:#}").contains("timed out"));
}

#[test]
fn next_event_distinguishes_a_quiet_stream_from_an_ended_one() {
    let (mut quiet, _open) = events_from(vec![]);
    assert!(
        quiet
            .next_event(Duration::from_millis(20))
            .expect("a quiet stream is not a failure")
            .is_none()
    );

    let (tx, rx) = mpsc::channel();
    drop(tx);
    let mut ended = SessionEvents {
        records: rx,
        transport: ControlTransport::DirectSocket,
        child: None,
        operation: None,
    };
    let ended = ended
        .next_event(Duration::from_millis(20))
        .expect_err("an ended stream must never read as 'nothing happened yet'");
    assert!(format!("{ended:#}").contains("event stream ended"));
}

#[test]
fn a_decode_failure_on_the_stream_surfaces_to_the_caller() {
    let (mut events, _open) =
        events_from(vec![Err(anyhow::anyhow!("parsing capsule event frame"))]);
    let error = events
        .next_event(Duration::from_millis(50))
        .expect_err("a malformed record is reported, not skipped");
    assert!(format!("{error:#}").contains("parsing capsule event frame"));
}

#[test]
fn ndjson_reader_decodes_the_records_the_capsule_cli_prints() {
    // Exactly the shape `jackin-capsule events` writes: one
    // `SessionEventRecord` per line, blank lines tolerated.
    let payload = format!(
        "{}\n\n{}\n",
        serde_json::to_string(&record(1, AgentState::Idle, SessionEventKind::Subscribed)).unwrap(),
        serde_json::to_string(&record(
            1,
            AgentState::Working,
            SessionEventKind::StateChanged {
                previous: AgentState::Idle,
            },
        ))
        .unwrap(),
    );
    let mut decoded = payload
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<SessionEventRecord>(line).expect("decode NDJSON line"));
    assert_eq!(decoded.next().expect("baseline").state, AgentState::Idle);
    let transition = decoded.next().expect("transition");
    assert_eq!(transition.state, AgentState::Working);
    assert_eq!(
        transition.kind,
        SessionEventKind::StateChanged {
            previous: AgentState::Idle
        }
    );
}

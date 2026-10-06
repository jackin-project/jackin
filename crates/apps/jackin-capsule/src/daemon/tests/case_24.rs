// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn session_send_to_an_unknown_session_denies_and_writes_nothing() {
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    mux.session_supervisor.sessions.insert(1, session);

    let reply = control_reply_for_request(
        &mut mux,
        ClientMsg::SessionSend {
            session: 999,
            text: "hello".to_owned(),
        },
    );

    assert!(matches!(
        reply,
        ServerMsg::SessionSendDenied {
            session: 999,
            reason: jackin_protocol::control::SessionSendRejection::UnknownSession,
        }
    ));
    assert!(
        matches!(input_rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "a misaddressed send must never reach another session's PTY"
    );
}

#[test]
fn session_send_reports_a_closed_writer_instead_of_claiming_delivery() {
    let mut mux = single_pane_tab_mux();
    let (session, input_rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    mux.session_supervisor.sessions.insert(1, session);
    // The writer task owns the receiver; dropping it is exactly the state a
    // session is in between process exit and the daemon reaping it.
    drop(input_rx);

    let reply = control_reply_for_request(
        &mut mux,
        ClientMsg::SessionSend {
            session: 1,
            text: "hello".to_owned(),
        },
    );

    assert!(matches!(
        reply,
        ServerMsg::SessionSendDenied {
            session: 1,
            reason: jackin_protocol::control::SessionSendRejection::WriterClosed,
        }
    ));
}

#[test]
fn session_send_records_input_recency_but_never_authors_state() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _input_rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    // A pane latched on `Blocked` by evidence arbitration. Input is recency
    // evidence only — the same rule the keyboard path follows — so the state
    // must survive the send unchanged and wait for the next arbitration tick.
    session.state = crate::protocol::AgentState::Blocked;
    let before_input_at = session.last_input_at;
    mux.session_supervisor.sessions.insert(1, session);

    drop(control_reply_for_request(
        &mut mux,
        ClientMsg::SessionSend {
            session: 1,
            text: "y\r".to_owned(),
        },
    ));

    let session = mux
        .session_supervisor
        .sessions
        .get(1)
        .expect("session is registered");
    assert_eq!(session.state, crate::protocol::AgentState::Blocked);
    assert!(
        session.last_input_at > before_input_at,
        "the send must land as input recency evidence"
    );
}

#[test]
fn events_subscription_opens_with_a_baseline_record_per_live_session() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    session.state = crate::protocol::AgentState::Working;
    mux.session_supervisor.sessions.insert(1, session);

    let mut events = subscribe_events(&mut mux, None);

    let baseline = next_event(&mut events);
    assert_eq!(baseline.session, 1);
    assert_eq!(baseline.seq, 0);
    assert_eq!(baseline.agent.as_deref(), Some("codex"));
    // The stream reports the arbitrated state as-is; it never recomputes it.
    assert_eq!(baseline.state, crate::protocol::AgentState::Working);
    assert_eq!(baseline.kind, SessionEventKind::Subscribed);
}

#[test]
fn events_subscription_filters_to_the_requested_session() {
    let mut mux = single_pane_tab_mux();
    for id in [1, 2] {
        let (session, _rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
        mux.session_supervisor.sessions.insert(id, session);
    }

    let mut events = subscribe_events(&mut mux, Some(2));

    assert_eq!(next_event(&mut events).session, 2);
    assert!(
        matches!(events.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "a filtered subscription must not see other sessions"
    );
}

#[test]
fn a_status_tick_publishes_the_working_transition_the_host_waits_on() {
    let mut mux = single_pane_tab_mux();
    let (session, _rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    mux.session_supervisor.sessions.insert(1, session);
    let mut events = subscribe_events(&mut mux, None);
    drop(next_event(&mut events));

    // Exactly what the tick loop does: diff the state vectors it captured
    // around `advance_status` and fan the difference out.
    let now = Instant::now();
    mux.session_supervisor
        .sessions
        .get_mut(1)
        .expect("session is registered")
        .state = crate::protocol::AgentState::Working;
    publish_status_events(
        &mut mux,
        &[(1, crate::protocol::AgentState::Idle)],
        &[(1, crate::protocol::AgentState::Working)],
        now,
    );

    let event = next_event(&mut events);
    assert_eq!(event.seq, 1);
    assert_eq!(event.state, crate::protocol::AgentState::Working);
    assert_eq!(
        event.kind,
        SessionEventKind::StateChanged {
            previous: crate::protocol::AgentState::Idle
        }
    );
}

#[test]
fn an_unchanged_status_tick_publishes_no_state_record() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    // Push the last output well outside the activity window so the quiet tick
    // is genuinely silent rather than merely free of state records.
    session.last_output_at = Instant::now()
        .checked_sub(Duration::from_mins(1))
        .expect("a minute before now is representable");
    mux.session_supervisor.sessions.insert(1, session);
    let mut events = subscribe_events(&mut mux, None);
    drop(next_event(&mut events));

    let states = [(1, crate::protocol::AgentState::Idle)];
    publish_status_events(&mut mux, &states, &states, Instant::now());

    assert!(
        matches!(events.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "a quiet tick must not push anything to subscribers"
    );
}

#[test]
fn session_send_then_status_tick_is_observable_end_to_end_in_process() {
    // The in-process shape of the host integration test: subscribe, send text
    // into a running session, then let the status tick report the transition.
    let mut mux = single_pane_tab_mux();
    let (session, mut input_rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    mux.session_supervisor.sessions.insert(1, session);
    let mut events = subscribe_events(&mut mux, Some(1));
    assert_eq!(next_event(&mut events).kind, SessionEventKind::Subscribed);

    let reply = control_reply_for_request(
        &mut mux,
        ClientMsg::SessionSend {
            session: 1,
            text: "go\r".to_owned(),
        },
    );
    assert!(matches!(reply, ServerMsg::SessionSent { session: 1, .. }));
    assert_eq!(
        input_rx.try_recv().expect("payload reached the PTY"),
        b"go\r".to_vec()
    );

    mux.session_supervisor
        .sessions
        .get_mut(1)
        .expect("session is registered")
        .state = crate::protocol::AgentState::Working;
    publish_status_events(
        &mut mux,
        &[(1, crate::protocol::AgentState::Idle)],
        &[(1, crate::protocol::AgentState::Working)],
        Instant::now(),
    );

    let event = next_event(&mut events);
    assert_eq!(event.session, 1);
    assert_eq!(event.state, crate::protocol::AgentState::Working);
    assert!(
        event.last_input_ms.is_some(),
        "the record must carry input recency from the send"
    );
}

#[test]
fn daemon_session_boundary_keeps_account_credentials_per_instance() {
    let mut mux = test_mux(24, 80);
    mux.launch_env.launch_config.instances = vec!["work".into(), "personal".into()];
    mux.launch_env.launch_config.agents = BTreeMap::from([
        ("work".into(), "claude".into()),
        ("personal".into(), "opencode".into()),
    ]);
    mux.launch_env.launch_config.auth_modes = BTreeMap::from([
        ("work".into(), "sync".into()),
        ("personal".into(), "api_key".into()),
    ]);
    mux.launch_env.launch_config.credential_provider_surfaces =
        BTreeMap::from([("personal".into(), "claude".into())]);
    mux.launch_env.launch_config.instance_home_dirs = BTreeMap::from([
        ("work".into(), "/home/agent/.claude".into()),
        ("personal".into(), "/home/agent/.local".into()),
    ]);
    mux.launch_env.launch_config.instance_forwarded_dirs = BTreeMap::from([
        ("work".into(), "/jackin/claude".into()),
        ("personal".into(), "/jackin/opencode".into()),
    ]);
    mux.launch_env.launch_config.instance_credential_files = BTreeMap::from([
        (
            "work".into(),
            jackin_protocol::account_credentials_container_path("work"),
        ),
        (
            "personal".into(),
            jackin_protocol::account_credentials_container_path("personal"),
        ),
    ]);
    mux.launch_env.launch_config.instance_mount_paths = BTreeMap::from([
        (
            "work".into(),
            vec!["/home/agent/.claude".into(), "/jackin/claude".into()],
        ),
        (
            "personal".into(),
            vec!["/home/agent/.local".into(), "/jackin/opencode".into()],
        ),
    ]);
    mux.launch_env.launch_config.instance_identities = BTreeMap::from([
        (
            "work".into(),
            jackin_protocol::SessionIdentity {
                uid: 2_000,
                gid: 2_000,
            },
        ),
        (
            "personal".into(),
            jackin_protocol::SessionIdentity {
                uid: 2_001,
                gid: 2_001,
            },
        ),
    ]);
    mux.launch_env.launch_config.shell_identity = Some(jackin_protocol::SessionIdentity {
        uid: 2_002,
        gid: 2_002,
    });
    mux.launch_env.agent_credentials = serde_json::from_value(serde_json::json!({
        "schema_version": 2,
        "instances": {
            "personal": {
                "agent": "opencode",
                "account_id": "acc-personal",
                "env": {
                    "ANTHROPIC_API_KEY": "opencode-anthropic",
                },
            },
        },
    }))
    .expect("v2 fixture must decode");
    let ambient = vec![("ANTHROPIC_API_KEY".into(), "ambient-secret".into())];
    let launch = mux
        .session_launch(Some("work"), None, &ambient, "test")
        .expect("known instance launches");
    assert!(launch.cmd.get_env("ANTHROPIC_API_KEY").is_none());
    assert!(launch.cmd.get_env("OPENAI_API_KEY").is_none());
    let launch = mux
        .session_launch(Some("personal"), None, &ambient, "test")
        .expect("known instance launches");
    assert_eq!(
        launch
            .cmd
            .get_env("ANTHROPIC_API_KEY")
            .and_then(|v| v.to_str()),
        Some("opencode-anthropic")
    );
    assert!(launch.cmd.get_env("OPENAI_API_KEY").is_none());
    assert!(
        mux.session_launch(Some("missing"), None, &ambient, "test")
            .is_err()
    );
}

#[test]
fn codex_session_launch_fans_model_and_effort_to_each_slot() {
    let mut mux = two_codex_mux();
    let stale_global_env = mux.launch_env.env_passthrough.clone();
    let work = mux
        .session_launch(Some("codex-work"), None, &stale_global_env, "test")
        .expect("work slot launches");
    let personal = mux
        .session_launch(Some("codex-personal"), None, &stale_global_env, "test")
        .expect("personal slot launches");

    let argv = |command: &CommandBuilder| {
        command
            .get_argv()
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        argv(&work.cmd),
        vec![
            jackin_core::container_paths::ENTRYPOINT.to_owned(),
            "-m".to_owned(),
            "k3".to_owned()
        ]
    );
    assert_eq!(
        argv(&personal.cmd),
        vec![
            jackin_core::container_paths::ENTRYPOINT.to_owned(),
            "-m".to_owned(),
            "glm-5.3".to_owned()
        ]
    );
    assert_eq!(
        work.cmd
            .get_env(jackin_core::CODEX_LANE_MODEL_ENV_NAME)
            .and_then(|value| value.to_str()),
        Some("k3")
    );
    assert_eq!(
        work.cmd
            .get_env(jackin_core::CODEX_LANE_EFFORT_ENV_NAME)
            .and_then(|value| value.to_str()),
        Some("max")
    );
    assert_eq!(
        personal
            .cmd
            .get_env(jackin_core::CODEX_LANE_MODEL_ENV_NAME)
            .and_then(|value| value.to_str()),
        Some("glm-5.3")
    );
    assert_eq!(
        personal
            .cmd
            .get_env(jackin_core::CODEX_LANE_EFFORT_ENV_NAME)
            .and_then(|value| value.to_str()),
        Some("low")
    );
}

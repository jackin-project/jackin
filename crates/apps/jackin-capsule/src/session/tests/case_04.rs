// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn pty_exit_reason_covers_the_closed_registry() {
    use jackin_telemetry::schema::enums::{ErrorType, PtyExitReason};

    let clean = portable_pty::ExitStatus::with_exit_code(0);
    let nonzero = portable_pty::ExitStatus::with_exit_code(7);
    let signal = portable_pty::ExitStatus::with_signal("SIGTERM");
    let wait_error = std::io::Error::other("wait failed");
    assert_eq!(pty_exit_reason(Ok(&clean), false), PtyExitReason::Clean);
    assert_eq!(
        pty_exit_reason(Ok(&nonzero), false),
        PtyExitReason::NonzeroExit
    );
    assert_eq!(pty_exit_reason(Ok(&signal), false), PtyExitReason::Signal);
    assert_eq!(
        pty_exit_reason(Err(&wait_error), false),
        PtyExitReason::WaitFailed
    );
    assert_eq!(pty_exit_reason(Ok(&clean), true), PtyExitReason::Cancelled);
    assert_eq!(pty_exit_error_type(PtyExitReason::Clean), None);
    assert_eq!(pty_exit_error_type(PtyExitReason::Cancelled), None);
    assert_eq!(
        pty_exit_error_type(PtyExitReason::Signal),
        Some(ErrorType::ProcessExitNonzero)
    );
    assert_eq!(
        pty_exit_error_type(PtyExitReason::NonzeroExit),
        Some(ErrorType::ProcessExitNonzero)
    );
    assert_eq!(
        pty_exit_error_type(PtyExitReason::WaitFailed),
        Some(ErrorType::IoError)
    );
}

#[test]
fn pty_spawn_exit_pair_is_bounded_and_does_not_export_wait_errors() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let private_error = std::io::Error::other("private PTY bytes and /private/workspace");
    tracing::subscriber::with_default(subscriber, || {
        emit_pty_spawn(Some("codex"), Some("conversation-proof"));
        emit_pty_exit(
            Some("codex"),
            Some("conversation-proof"),
            Err(&private_error),
            false,
        );
    });
    export.force_flush();

    assert_eq!(export.event_count("pty.spawn"), 1);
    assert_eq!(export.event_count("pty.exit"), 1);
    assert!(export.contains_log_text("wait_failed"));
    assert!(export.contains_log_text("io_error"));
    assert!(export.contains_log_text("codex"));
    assert!(export.contains_log_text("conversation-proof"));
    assert!(!export.contains_log_text("private PTY bytes"));
    assert!(!export.contains_log_text("/private/workspace"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawn_keeps_provider_route_with_same_agent_instance_and_account() {
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let slots = [
        (
            "codex-work",
            "openai-work",
            "Codex · Work",
            "https://work.example.test/v1",
            2_001,
        ),
        (
            "codex-personal",
            "openai-personal",
            "Codex · Personal",
            "https://personal.example.test/v1",
            2_002,
        ),
    ];

    let mut sessions = Vec::with_capacity(slots.len());
    for (instance_id, account_id, label, endpoint, uid) in slots {
        let mut command = CommandBuilder::new("/bin/sh");
        command.arg("-c");
        command.arg("exit 0");
        let (session, _id) = Session::spawn(
            SessionSpawnSpec {
                label: label.to_owned(),
                agent: Some(instance_id.to_owned()),
                account_id: Some(account_id.to_owned()),
                identity: jackin_protocol::SessionIdentity { uid, gid: uid },
                provider: Some(SessionProvider {
                    label: "OpenAI".to_owned(),
                    env_overrides: vec![("OPENAI_BASE_URL".to_owned(), endpoint.to_owned())],
                }),
                cache_dir: None,
            },
            command,
            SessionTerminal {
                rows: 24,
                cols: 80,
                row_arena: termpane::RowArena::default(),
                default_fg: None,
                default_bg: None,
            },
            event_tx.clone(),
        )
        .expect("spawn real PTY session");
        sessions.push(session);
    }

    for (session, (instance_id, account_id, label, endpoint, uid)) in sessions.iter().zip(slots) {
        assert_eq!(session.label, label);
        // `Session.agent` stores the stable instance configuration ID, not
        // the shared executable slug (`codex`).
        assert_eq!(session.agent.as_deref(), Some(instance_id));
        assert_eq!(session.account_id.as_deref(), Some(account_id));
        assert_eq!(session.identity.uid, uid);
        let provider = session.provider.as_ref().expect("provider route retained");
        assert_eq!(provider.label, "OpenAI");
        assert_eq!(
            provider.env_overrides,
            vec![("OPENAI_BASE_URL".to_owned(), endpoint.to_owned())]
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conformance_wire_real_pty_spawn_stream_and_exit_exclude_private_content() {
    if crate::process_telemetry::run_wire_test_in_child(
        "session::tests::case_04::conformance_wire_real_pty_spawn_stream_and_exit_exclude_private_content",
        "JACKIN_SESSION_WIRE_CHILD",
    )
    .expect("dispatch isolated session wire test")
    {
        return;
    }
    let _telemetry_guard = crate::support::telemetry_test_guard_async().await;
    let testbed = jackin_otlp_testbed::Testbed::start().expect("start OTLP testbed");
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )
    .expect("initialize wire test export");
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut command = CommandBuilder::new("/bin/sh");
    command.arg("-c");
    command.arg("printf wire-private-pty-output; exit 17");
    let terminal = SessionTerminal {
        rows: 24,
        cols: 80,
        row_arena: termpane::RowArena::default(),
        default_fg: None,
        default_bg: None,
    };

    let (_session, session_id) = Session::spawn(
        SessionSpawnSpec {
            label: "wire-private-tab-label".to_owned(),
            agent: Some("codex".to_owned()),
            account_id: Some("acc-codex".to_owned()),
            identity: jackin_protocol::SessionIdentity {
                uid: 2_002,
                gid: 2_002,
            },
            provider: None,
            cache_dir: None,
        },
        command,
        terminal,
        event_tx,
    )
    .expect("spawn real PTY session");
    let exit_reason = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Some(SessionEvent::Exited {
                session_id: exited_id,
                reason,
            }) = event_rx.recv().await
            {
                assert_eq!(exited_id, session_id);
                break reason;
            }
        }
    })
    .await
    .expect("PTY session exits before deadline");
    assert_eq!(
        exit_reason.as_deref(),
        Some("session process exited with code 17")
    );
    jackin_diagnostics::flush_wire_test_export().expect("flush wire test export");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let records = loop {
        let records = testbed
            .log_records()
            .into_iter()
            .filter(|record| matches!(record.event_name.as_str(), "pty.spawn" | "pty.exit"))
            .collect::<Vec<_>>();
        if records.len() == 2 {
            break records;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "PTY spawn and exit wire events did not arrive exactly once"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    };
    let wire_text = format!("{records:?}");
    for expected in ["pty.spawn", "pty.exit", "nonzero_exit", "codex", "17"] {
        assert!(
            wire_text.contains(expected),
            "missing {expected}: {wire_text}"
        );
    }
    let prohibited = [
        "wire-private-pty-output",
        "wire-private-tab-label",
        "printf wire-private-pty-output",
        "/bin/sh",
    ];
    for value in prohibited {
        assert!(!wire_text.contains(value), "exported {value}");
    }
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
}

#[test]
fn terminate_marks_the_live_exit_as_cancelled() {
    let (input_tx, _input_rx) = mpsc::unbounded_channel();
    let session = Session::new_for_test(
        "test".to_owned(),
        None,
        None,
        (24, 80),
        0,
        input_tx,
        Arc::new(Mutex::new(Box::new(NullMasterPty))),
        Arc::new(Mutex::new(Box::new(NullChildKiller))),
    );
    session.terminate();
    assert!(
        session
            .termination_requested
            .load(std::sync::atomic::Ordering::Acquire)
    );
}

#[test]
fn diagnostic_tail_zero_rows_is_none() {
    let session = test_session_with_policy(OscPolicy::default());
    assert_eq!(session.diagnostic_tail(0), None);
}

#[test]
fn diagnostic_tail_blank_pane_is_none() {
    let session = test_session_with_policy(OscPolicy::default());
    assert_eq!(session.diagnostic_tail(12), None);
}

#[test]
fn diagnostic_tail_returns_last_nonblank_rows_oldest_first() {
    let mut session = test_session_with_policy(OscPolicy::default());
    session.feed_pty(b"alpha\r\nbravo\r\ncharlie\r\n");
    let tail = session
        .diagnostic_tail(2)
        .expect("rendered rows must yield a tail");
    assert_eq!(tail, "bravo\ncharlie");
}

#[tokio::test]
async fn writer_init_failure_emits_exited_with_reason() {
    let fault = FaultMasterPty {
        take_writer_err: Some(std::io::ErrorKind::PermissionDenied),
        ..Default::default()
    };
    let (_input_tx, mut event_rx) = start_fault_pty_tasks(fault);
    let ev = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
        .await
        .expect("timeout")
        .expect("channel closed");
    match ev {
        SessionEvent::Exited { reason, .. } => {
            assert_eq!(
                reason.as_deref(),
                Some("session PTY writer failed to initialize")
            );
        }
        other => panic!("expected Exited, got {other:?}"),
    }
}

#[tokio::test]
async fn reader_init_failure_emits_exited_with_reason() {
    let fault = FaultMasterPty {
        clone_reader_err: Some(std::io::ErrorKind::PermissionDenied),
        ..Default::default()
    };
    let (_input_tx, mut event_rx) = start_fault_pty_tasks(fault);
    let ev = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
        .await
        .expect("timeout")
        .expect("channel closed");
    match ev {
        SessionEvent::Exited { reason, .. } => {
            assert_eq!(
                reason.as_deref(),
                Some("session PTY reader failed to initialize")
            );
        }
        other => panic!("expected Exited, got {other:?}"),
    }
}

#[tokio::test]
async fn mid_stream_write_failure_emits_exited() {
    let fault = FaultMasterPty {
        writer_fails_after: Some(0),
        ..Default::default()
    };
    let (input_tx, mut event_rx) = start_fault_pty_tasks(fault);
    input_tx.send(b"x".to_vec()).expect("input channel open");
    let ev = tokio::time::timeout(std::time::Duration::from_secs(2), event_rx.recv())
        .await
        .expect("timeout")
        .expect("channel closed");
    match ev {
        SessionEvent::Exited { reason, .. } => {
            let r = reason.expect("reason");
            assert!(
                r.starts_with("session PTY write failed:"),
                "unexpected reason: {r}"
            );
        }
        other => panic!("expected Exited, got {other:?}"),
    }
}

#[tokio::test]
async fn read_error_breaks_without_exited_event() {
    let fault = FaultMasterPty {
        reader_yields: vec![Err(std::io::ErrorKind::BrokenPipe)],
        ..Default::default()
    };
    let (_input_tx, mut event_rx) = start_fault_pty_tasks(fault);
    // Reader should break without Exited; give tasks a moment then try_recv.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        event_rx.try_recv().is_err(),
        "read error must not emit Exited (reaper is authoritative)"
    );
}

#[test]
fn bare_claude_notification_payload_authors_authority() {
    use crate::agent_status::evidence::{AuthorityGrade, RawAgentState};
    let mut session = test_session_with_policy(OscPolicy::default());
    session.apply_runtime_event(
        "hook-claude-1",
        "claude",
        "Notification",
        Some(r#"{"notification_type":"permission_prompt"}"#),
        std::time::Instant::now(),
    );
    let a = session
        .authority
        .as_ref()
        .expect("authority set from payload subtype");
    assert_eq!(a.mapped_state, RawAgentState::Blocked);
    assert!(a.pending_permission);
    assert_eq!(a.grade, AuthorityGrade::Partial);
}

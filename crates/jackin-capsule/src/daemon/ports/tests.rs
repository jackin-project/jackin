//! Fake-port tests over the real daemon subsystem owners.

use super::*;
use crate::daemon::Dialog;
use chrono::TimeZone;
use std::sync::atomic::Ordering;

fn mux() -> crate::daemon::Multiplexer {
    crate::daemon::tests::single_pane_tab_mux()
}

#[test]
fn control_port_applies_unknown_event_and_always_acks() {
    let ports = FakeDaemonPorts::new();
    let mut mux = mux();
    let reply = ports.report_runtime_event(
        &mut mux.session_supervisor.sessions,
        RuntimeEvent {
            session_id: 404,
            source_id: "hook",
            runtime: "claude",
            event: "busy",
            payload: None,
            observed_at: Instant::now(),
        },
    );

    assert!(matches!(reply, ServerMsg::Ack));
    assert_eq!(*ports.runtime_events.lock().unwrap(), vec![404]);
}

#[tokio::test]
async fn fake_ports_drive_attach_displace_disconnect_and_reattach() {
    let ports = FakeDaemonPorts::new();
    let mut mux = mux();

    assert_eq!(
        ports.begin_attach(&mux.client_registry),
        AttachTransition::Attach
    );
    ports.record_attached();

    mux.client_registry.attached_task = Some(tokio::spawn(async {}));
    assert_eq!(
        ports.begin_attach(&mux.client_registry),
        AttachTransition::Displace
    );
    ports.record_detached();
    mux.client_registry.attached_task = None;

    assert_eq!(
        ports.begin_attach(&mux.client_registry),
        AttachTransition::Attach
    );
    ports.record_attached();

    assert_eq!(ports.attach_count.load(Ordering::SeqCst), 2);
    assert_eq!(ports.detach_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        *ports.transitions.lock().unwrap(),
        vec![
            AttachTransition::Attach,
            AttachTransition::Displace,
            AttachTransition::Attach
        ]
    );
}

#[test]
fn fake_port_injects_pty_spawn_failure_at_supervisor_boundary() {
    let ports = FakeDaemonPorts::new();
    let mux = mux();

    ports.fail_spawn.store(true, Ordering::SeqCst);
    let error = ports
        .prepare_session_spawn(&mux.session_supervisor)
        .expect_err("configured spawn failure must cross the port boundary");
    assert_eq!(error.to_string(), "injected PTY spawn failure");
}

#[test]
fn status_port_retires_codename_and_stamps_history() {
    let ports = FakeDaemonPorts::new();
    let mut mux = mux();
    let observed_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    mux.session_supervisor
        .codename_live
        .insert("test".to_owned());
    mux.session_supervisor
        .agent_history
        .push(crate::daemon::AgentRecord {
            session_id: 1,
            codename: "test".to_owned(),
            agent: None,
            provider: None,
            started_at: observed_at,
            exited_at: None,
        });

    ports.retire_codename(&mut mux.session_supervisor, "test", observed_at);

    assert!(!mux.session_supervisor.codename_live.contains("test"));
    assert!(mux.session_supervisor.codename_retired.contains("test"));
    assert_eq!(
        mux.session_supervisor.agent_history[0].exited_at,
        Some(observed_at)
    );
}

#[test]
fn persistence_port_reads_live_dialog_stack() {
    let ports = FakeDaemonPorts::new();
    let mut mux = mux();
    assert_eq!(
        ports.last_session_exit(&mux.control),
        ExitDisposition::Evaluate
    );

    mux.control.dialog_stack.push(Dialog::SpawnFailure(
        termrock::components::ErrorPopupState::new("Spawn failed", "test"),
    ));
    assert_eq!(
        ports.last_session_exit(&mux.control),
        ExitDisposition::Defer
    );
}

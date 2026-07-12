//! Tests for daemon port seams and session SM sim.
use super::*;

#[test]
fn persistence_port_defers_when_dialog_open() {
    let ports = DefaultDaemonPorts;
    assert!(ports.defer_last_session_exit(true));
    assert!(!ports.defer_last_session_exit(false));
}

#[test]
fn session_sm_last_exit_drains_then_exits() {
    let mut s = SessionSmState::Empty;
    s = session_sm_step(s, SessionSmEvent::SessionSpawned);
    assert_eq!(s, SessionSmState::Live { count: 1 });
    s = session_sm_step(s, SessionSmEvent::SessionExited);
    assert_eq!(s, SessionSmState::Draining { dialog_open: false });
    s = session_sm_step(s, SessionSmEvent::DrainCompleted);
    assert_eq!(s, SessionSmState::Exited);
}

#[test]
fn session_sm_defers_drain_while_dialog_open_inv_d19() {
    let mut s = SessionSmState::Live { count: 1 };
    s = session_sm_step(s, SessionSmEvent::SessionExited);
    s = session_sm_step(s, SessionSmEvent::DialogOpened);
    assert_eq!(s, SessionSmState::Draining { dialog_open: true });
    s = session_sm_step(s, SessionSmEvent::DrainCompleted);
    assert_eq!(s, SessionSmState::Draining { dialog_open: true });
    s = session_sm_step(s, SessionSmEvent::DialogClosed);
    s = session_sm_step(s, SessionSmEvent::DrainCompleted);
    assert_eq!(s, SessionSmState::Exited);
}

#[test]
fn attach_port_displaces_when_client_active() {
    let ports = DefaultDaemonPorts;
    assert!(ports.should_displace_on_hello(true));
    assert!(!ports.should_displace_on_hello(false));
}

#[test]
fn control_and_status_ports_exercise_seams() {
    let ports = DefaultDaemonPorts;
    assert!(ports.control_acks_unknown_session("sess-1"));
    assert!(ports.should_retire_codename_on_exit(0));
    assert!(ports.should_retire_codename_on_exit(2));
}

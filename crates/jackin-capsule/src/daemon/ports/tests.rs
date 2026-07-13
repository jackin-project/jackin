//! Port decision unit tests (pure). Integration with Multiplexer lives in
//! `daemon/tests.rs` (INV-D8 `remove_exited_session`, INV-D19 last-session exit).
use super::*;

#[test]
fn control_port_always_acks_unknown_session() {
    assert!(PORTS.should_ack_unknown_session_runtime_event(false));
    assert!(PORTS.should_ack_unknown_session_runtime_event(true));
}

#[test]
fn attach_port_displaces_only_when_client_active() {
    assert!(PORTS.should_displace_on_hello(true));
    assert!(!PORTS.should_displace_on_hello(false));
}

#[test]
fn status_port_always_retires_codename() {
    assert!(PORTS.should_retire_codename_on_exit(0));
    assert!(PORTS.should_retire_codename_on_exit(3));
}

#[test]
fn persistence_port_defers_when_dialog_open() {
    assert!(PORTS.defer_last_session_exit(true));
    assert!(!PORTS.defer_last_session_exit(false));
}

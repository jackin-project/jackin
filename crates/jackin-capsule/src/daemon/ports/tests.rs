//! Port decision unit tests + `FakeDaemonPorts` boundary harness (plan 017).
//!
//! Production INV defaults are pure; fake-port tests prove attach/displace/
//! reattach/PTY-failure/persistence observable behaviour at the port surface
//! (not helper predicates alone).

use super::*;
use std::sync::atomic::Ordering;

#[test]
fn control_port_always_acks_unknown_session() {
    assert!(PORTS.should_ack_unknown_session_runtime_event(1, false));
    assert!(PORTS.should_ack_unknown_session_runtime_event(1, true));
}

#[test]
fn attach_port_displaces_only_when_client_active() {
    assert!(PORTS.should_displace_on_hello(true));
    assert!(!PORTS.should_displace_on_hello(false));
}

#[test]
fn status_port_always_retires_codename() {
    assert!(PORTS.should_retire_codename_on_exit(7, 0));
    assert!(PORTS.should_retire_codename_on_exit(7, 3));
}

#[test]
fn persistence_port_defers_when_dialog_open() {
    assert!(PORTS.defer_last_session_exit(true));
    assert!(!PORTS.defer_last_session_exit(false));
}

#[test]
fn fake_ports_attach_displace_reattach_ledger() {
    let ports = FakeDaemonPorts::new();
    // First attach: no active client → no displace.
    assert!(!ports.should_displace_on_hello(false));
    ports.record_attach();
    assert_eq!(ports.attach_count.load(Ordering::SeqCst), 1);

    // Second Hello with active client → displace, then reattach.
    assert!(ports.should_displace_on_hello(true));
    ports.record_detach();
    ports.record_attach();
    assert_eq!(ports.detach_count.load(Ordering::SeqCst), 1);
    assert_eq!(ports.attach_count.load(Ordering::SeqCst), 2);

    let obs = ports.displace_observations.lock().unwrap();
    assert_eq!(obs.as_slice(), &[false, true]);
}

#[test]
fn fake_ports_pty_failure_path_is_observable() {
    let ports = FakeDaemonPorts::new();
    assert!(!ports.pty_failed());
    ports.mark_pty_failure();
    assert!(
        ports.pty_failed(),
        "PTY failure must stick for harness asserts"
    );
}

#[test]
fn fake_ports_persistence_round_trip_defer_policy() {
    let ports = FakeDaemonPorts::new();
    assert!(!ports.defer_last_session_exit(false));
    ports.force_defer_exit.store(true, Ordering::SeqCst);
    assert!(
        ports.defer_last_session_exit(false),
        "force_defer_exit must defer even with no dialog"
    );
}

#[test]
fn fake_ports_refuse_unknown_ack_and_skip_codename_retire() {
    let ports = FakeDaemonPorts::new();
    ports.refuse_unknown_ack.store(true, Ordering::SeqCst);
    assert!(!ports.should_ack_unknown_session_runtime_event(9, false));
    assert!(ports.should_ack_unknown_session_runtime_event(9, true));

    ports.skip_codename_retire.store(true, Ordering::SeqCst);
    assert!(!ports.should_retire_codename_on_exit(3, 1));
}

#[test]
fn fake_ports_force_displace_without_active_client() {
    let ports = FakeDaemonPorts::new();
    ports.force_displace.store(true, Ordering::SeqCst);
    assert!(
        ports.should_displace_on_hello(false),
        "force_displace simulates sticky displace policy"
    );
}

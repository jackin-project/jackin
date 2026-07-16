// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Barrier};

use super::*;

static TEST_SESSION_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn identity_values_are_uuid_unique_and_parseable() {
    let first = InvocationId::mint();
    let second = InvocationId::mint();
    assert_ne!(first, second);
    assert_eq!(InvocationId::parse(&first.to_string()).unwrap(), first);
}

#[test]
fn session_rejects_concurrent_owner_and_reattach_uses_last_ended() {
    let _serial = TEST_SESSION_LOCK.lock().unwrap();
    let first = SessionGuard::begin(SessionKind::Console).unwrap();
    let first_id = first.context().current;
    let barrier = Arc::new(Barrier::new(2));
    let worker_barrier = Arc::clone(&barrier);
    let worker = std::thread::spawn(move || {
        worker_barrier.wait();
        SessionGuard::begin(SessionKind::Attachment).unwrap_err()
    });
    barrier.wait();
    let conflict = worker.join().unwrap();
    assert_eq!(conflict.active.current, first_id);
    assert_eq!(conflict.active.kind, SessionKind::Console);
    assert_eq!(current_session(), Some(first.context()));
    drop(first);

    let reattach = SessionGuard::begin(SessionKind::Attachment).unwrap();
    assert_ne!(reattach.context().current, first_id);
    assert_eq!(reattach.context().previous, Some(first_id));
    assert_eq!(reattach.context().kind, SessionKind::Attachment);
    drop(reattach);
    assert_eq!(current_session(), None);
}

#[test]
fn interleaved_non_owner_drop_cannot_clear_active_session() {
    let _serial = TEST_SESSION_LOCK.lock().unwrap();
    let owner = SessionGuard::begin(SessionKind::Capsule).unwrap();
    let owner_context = owner.context();
    let conflict = SessionGuard::claim(SessionKind::Attachment).unwrap_err();
    assert_eq!(conflict.active, owner_context);
    assert_eq!(current_session(), Some(owner_context));
    drop(owner);
    assert_eq!(current_session(), None);
}

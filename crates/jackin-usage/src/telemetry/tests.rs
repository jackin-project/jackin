// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn inactive_failed_startup_releases_ambient_session() {
    let _serial = TEST_LOCK.lock().unwrap();
    let session = jackin_telemetry::identity::SessionGuard::claim(
        jackin_telemetry::identity::SessionKind::Capsule,
    )
    .unwrap();
    let id = session.context().current;
    let startup =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::APP_STARTUP, &[]).ok();
    let guard = FlushGuard {
        session: Some(session),
        startup,
        active: false,
    };
    assert_eq!(
        jackin_telemetry::identity::current_session().map(|value| value.current),
        Some(id)
    );
    drop(guard);
    assert_eq!(jackin_telemetry::identity::current_session(), None);
}

#[test]
fn listener_readiness_completes_bounded_startup_once() {
    let _serial = TEST_LOCK.lock().unwrap();
    let session = jackin_telemetry::identity::SessionGuard::claim(
        jackin_telemetry::identity::SessionKind::Capsule,
    )
    .unwrap();
    let startup =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::APP_STARTUP, &[]).ok();
    let mut guard = FlushGuard {
        session: Some(session),
        startup,
        active: false,
    };
    guard.listener_ready();
    assert!(guard.startup.is_none());
    guard.listener_ready();
    drop(guard);
    assert_eq!(jackin_telemetry::identity::current_session(), None);
}

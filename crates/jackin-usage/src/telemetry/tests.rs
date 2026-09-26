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

#[test]
fn daemon_failure_has_exactly_one_owner_across_readiness() {
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

    assert!(!guard.daemon_failure_needs_terminal_event());
    guard.listener_ready();
    assert!(guard.daemon_failure_needs_terminal_event());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capsule_panic_hook_shuts_down_active_telemetry() -> anyhow::Result<()> {
    const CHILD_ENV: &str = "JACKIN_PANIC_HOOK_TELEMETRY_CHILD";
    if std::env::var_os(CHILD_ENV).is_none() {
        let testbed = jackin_otlp_testbed::Testbed::start()?;
        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "telemetry::tests::capsule_panic_hook_shuts_down_active_telemetry",
                "--nocapture",
            ])
            .env(CHILD_ENV, "1")
            .env("OTEL_EXPORTER_OTLP_ENDPOINT", testbed.endpoint())
            .env("OTEL_EXPORTER_OTLP_PROTOCOL", "grpc")
            .status()?;
        anyhow::ensure!(status.success(), "panic-hook telemetry child failed");
        anyhow::ensure!(
            testbed
                .wait_for_all_signals(std::time::Duration::from_secs(2))
                .await,
            "panic-hook telemetry child did not flush all signals"
        );
        return Ok(());
    }

    let _guard = init().expect("active capsule telemetry");
    assert!(otlp_active());
    crate::logging::init();
    let panic_result = std::panic::catch_unwind(|| panic!("panic-hook shutdown proof"));
    assert!(panic_result.is_err(), "panic hook child did not panic");
    assert!(!otlp_active(), "panic hook did not shut down telemetry");
    Ok(())
}

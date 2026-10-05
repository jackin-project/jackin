// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn pr_context_recovery_export_is_bodyless() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, record_pr_context_recovery);

    export.force_flush();
    assert_eq!(export.event_count("operation.warn"), 1);
    assert!(export.contains_log_text("recovered_degradation"));
    for private in ["pull request", "bucket", "URL", "command", "raw error"] {
        assert!(!export.contains_log_text(private));
    }
}

#[cfg(unix)]
fn finite_pr_probe(script: &str) -> jackin_process::ExecRequest {
    jackin_process::ExecRequest::new("/bin/sh", ["-c", script])
}

#[cfg(unix)]
#[test]
fn finite_pr_probe_accepts_pending_status_and_healthy_output() {
    assert_eq!(
        run_command_capturing_output(
            &finite_pr_probe("printf ' healthy '; exit 8"),
            Duration::from_secs(2),
            &[0, 8],
        ),
        Ok(Some("healthy".to_owned())),
    );
}

#[cfg(unix)]
#[test]
fn finite_pr_probe_deadline_covers_descendant_held_stdout_and_stderr() {
    let started = std::time::Instant::now();
    assert_eq!(
        run_command_capturing_output(
            &finite_pr_probe("sleep 30 & printf healthy; printf private >&2"),
            Duration::from_millis(100),
            &[0],
        ),
        Err(LookupError::Timeout),
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[cfg(unix)]
#[test]
fn finite_pr_probe_rejects_oversized_stdout_and_stderr() {
    for script in [
        "dd if=/dev/zero bs=1024 count=65 2>/dev/null; sleep 30",
        "(dd if=/dev/zero bs=1024 count=5 2>/dev/null) >&2; sleep 30",
    ] {
        let started = std::time::Instant::now();
        assert_eq!(
            run_command_capturing_output(&finite_pr_probe(script), Duration::from_secs(10), &[0],),
            Err(LookupError::Io),
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}

#[cfg(unix)]
#[test]
fn finite_pr_probe_preserves_spawn_nonzero_empty_and_deadline_classes() {
    let missing = tempfile::tempdir().unwrap();
    assert_eq!(
        run_command_capturing_output(
            &jackin_process::ExecRequest::new(missing.path().join("missing"), None::<&str>),
            Duration::from_secs(2),
            &[0],
        ),
        Err(LookupError::Spawn),
    );
    assert_eq!(
        run_command_capturing_output(&finite_pr_probe("exit 1"), Duration::from_secs(2), &[0]),
        Err(LookupError::Nonzero),
    );
    assert_eq!(
        run_command_capturing_output(&finite_pr_probe("exit 0"), Duration::from_secs(2), &[0]),
        Ok(None),
    );
    let started = std::time::Instant::now();
    assert_eq!(
        run_command_capturing_output(
            &finite_pr_probe("sleep 30"),
            Duration::from_millis(100),
            &[0]
        ),
        Err(LookupError::Timeout),
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

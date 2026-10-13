// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn exports_auth_processes_without_credential_material() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    exec_sync(&ExecRequest::new(
        "gh",
        ["auth", "token", "credential-secret-argument"],
    ))
    .unwrap();
    let error = exec_sync(&ExecRequest::new(
        "/credential-secret/security",
        ["credential-secret-service"],
    ))
    .unwrap_err();
    assert_eq!(error.to_string(), "process spawn failed");

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 2);
    assert_eq!(export.error_span_count(), 2);
    for expected in ["gh", "other", "process_exit_nonzero", "process_spawn_error"] {
        assert!(export.contains_span_text(expected));
    }
    for secret in [
        "credential-secret-argument",
        "/credential-secret/security",
        "credential-secret-service",
    ] {
        assert!(!export.contains_span_text(secret));
    }
}

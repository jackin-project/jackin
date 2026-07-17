// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn exports_docker_context_process_without_context_material() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    exec_sync(&ExecRequest::new(
        "docker",
        ["context", "inspect", "context-secret-name"],
    ))
    .unwrap();
    let error = exec_sync(&ExecRequest::new(
        "/context-secret/missing-docker",
        ["context-secret-argument"],
    ))
    .unwrap_err();
    assert_eq!(error.to_string(), "process spawn failed");

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 2);
    assert_eq!(export.error_span_count(), 2);
    for expected in [
        "docker",
        "other",
        "process_exit_nonzero",
        "process_spawn_error",
    ] {
        assert!(export.contains_span_text(expected));
    }
    for secret in [
        "context-secret-name",
        "/context-secret/missing-docker",
        "context-secret-argument",
    ] {
        assert!(!export.contains_span_text(secret));
    }
}

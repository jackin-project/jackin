// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_process::{ExecRequest, ExecResult};
use jackin_telemetry::schema::enums::{ErrorType, OutcomeValue};

pub(crate) fn exec_sync(request: &ExecRequest) -> anyhow::Result<ExecResult> {
    let operation = jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::PROCESS_COMMAND,
        &[jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXECUTABLE_NAME,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::process::classify_executable(&request.program).as_str(),
            ),
        }],
    );
    let result = jackin_process::exec_sync(request);
    let completion = match &result {
        Ok(output) => {
            if let Some(code) = output.code {
                let _attribute = operation.set_attr(jackin_telemetry::Attr {
                    key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
                    value: jackin_telemetry::Value::I64(i64::from(code)),
                });
            }
            if output.timed_out {
                (OutcomeValue::Timeout, Some(ErrorType::Timeout))
            } else if output.success {
                (OutcomeValue::Success, None)
            } else {
                (OutcomeValue::Failure, Some(ErrorType::ProcessExitNonzero))
            }
        }
        Err(_) => (OutcomeValue::Failure, Some(ErrorType::ProcessSpawnError)),
    };
    operation.complete(completion.0, completion.1);
    result.map_err(|_| anyhow::anyhow!("process spawn failed"))
}

#[cfg(test)]
mod tests {
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
}

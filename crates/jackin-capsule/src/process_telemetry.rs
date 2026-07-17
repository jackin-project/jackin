// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_process::{ExecRequest, ExecResult};
use jackin_telemetry::schema::enums::{ErrorType, OutcomeValue, ProcessExecutableName};

fn operation(
    request: &ExecRequest,
    executable: Option<ProcessExecutableName>,
) -> jackin_telemetry::OperationGuard {
    let executable = executable
        .unwrap_or_else(|| jackin_telemetry::process::classify_executable(&request.program));
    jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::PROCESS_COMMAND,
        &[jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXECUTABLE_NAME,
            value: jackin_telemetry::Value::Str(executable.as_str()),
        }],
    )
}

fn complete(operation: jackin_telemetry::OperationGuard, result: &anyhow::Result<ExecResult>) {
    let completion = match result {
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
}

pub(crate) fn exec_sync(request: &ExecRequest) -> anyhow::Result<ExecResult> {
    let operation = operation(request, None);
    let result = jackin_process::exec_sync(request);
    complete(operation, &result);
    result.map_err(|_| anyhow::anyhow!("process spawn failed"))
}

pub(crate) async fn exec_async_as(
    request: &ExecRequest,
    executable: ProcessExecutableName,
) -> anyhow::Result<ExecResult> {
    let operation = operation(request, Some(executable));
    let result = jackin_process::exec_async(request).await;
    complete(operation, &result);
    result.map_err(|_| anyhow::anyhow!("process spawn failed"))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn exports_capsule_process_matrix_without_operator_material() {
        let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
        let _subscriber = tracing::subscriber::set_default(subscriber);

        let success = ExecRequest::new(
            "sh",
            [
                "-c",
                "printf operator-secret-stdout; printf operator-secret-stderr >&2",
            ],
        );
        exec_async_as(&success, ProcessExecutableName::ConfiguredCommand)
            .await
            .unwrap();

        let nonzero = ExecRequest::new("git", ["operator-secret-argument"]);
        exec_async_as(&nonzero, ProcessExecutableName::Git)
            .await
            .unwrap();

        let timeout = ExecRequest::new("sh", ["-c", "sleep 1"]).timeout(Duration::from_millis(5));
        exec_async_as(&timeout, ProcessExecutableName::ConfiguredCommand)
            .await
            .unwrap();

        let missing = ExecRequest::new(
            "/operator-secret/missing-command",
            ["operator-secret-spawn-argument"],
        );
        let error = exec_async_as(&missing, ProcessExecutableName::ConfiguredCommand)
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "process spawn failed");

        export.force_flush();
        assert_eq!(export.finished_spans().len(), 4);
        assert_eq!(export.error_span_count(), 3);
        assert!(export.contains_span_text("configured_command"));
        assert!(export.contains_span_text("git"));
        assert!(export.contains_span_text("process_exit_nonzero"));
        assert!(export.contains_span_text("process_spawn_error"));
        assert!(export.contains_span_text("timeout"));
        for secret in [
            "operator-secret-stdout",
            "operator-secret-stderr",
            "operator-secret-argument",
            "/operator-secret/missing-command",
            "operator-secret-spawn-argument",
        ] {
            assert!(!export.contains_span_text(secret));
        }
    }
}

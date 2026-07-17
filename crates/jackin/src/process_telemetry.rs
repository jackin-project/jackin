// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_process::{ExecRequest, ExecResult};
use jackin_telemetry::schema::enums::{ErrorType, OutcomeValue};

fn operation(request: &ExecRequest) -> jackin_telemetry::OperationGuard {
    jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::PROCESS_COMMAND,
        &[jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXECUTABLE_NAME,
            value: jackin_telemetry::Value::Str(
                jackin_telemetry::process::classify_executable(&request.program).as_str(),
            ),
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
    let operation = operation(request);
    let result = jackin_process::exec_sync(request);
    complete(operation, &result);
    result.map_err(|_| anyhow::anyhow!("process spawn failed"))
}

pub(crate) fn exec_sync_optional(request: &ExecRequest) -> anyhow::Result<Option<ExecResult>> {
    let operation = operation(request);
    let result = jackin_process::exec_sync(request);
    complete(operation, &result);
    match result {
        Ok(output) => Ok(Some(output)),
        Err(error)
            if error.chain().any(|cause| {
                cause
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
            }) =>
        {
            Ok(None)
        }
        Err(_) => Err(anyhow::anyhow!("process spawn failed")),
    }
}

pub(crate) async fn exec_async(request: &ExecRequest) -> anyhow::Result<ExecResult> {
    let operation = operation(request);
    let result = jackin_process::exec_async(request).await;
    complete(operation, &result);
    result.map_err(|_| anyhow::anyhow!("process spawn failed"))
}

pub(crate) struct SpawnOperation {
    operation: Option<jackin_telemetry::OperationGuard>,
}

impl SpawnOperation {
    fn finish(mut self, outcome: OutcomeValue, error_type: Option<ErrorType>) {
        if let Some(operation) = self.operation.take() {
            operation.complete(outcome, error_type);
        }
    }

    pub(crate) fn complete_ready(self) {
        self.finish(OutcomeValue::Success, None);
    }

    pub(crate) fn complete_io_failure(self) {
        self.finish(OutcomeValue::Failure, Some(ErrorType::IoError));
    }
}

impl Drop for SpawnOperation {
    fn drop(&mut self) {
        if let Some(operation) = self.operation.take() {
            operation.complete(
                OutcomeValue::Failure,
                Some(ErrorType::TelemetryInstrumentationFault),
            );
        }
    }
}

pub(crate) fn spawn_async(
    request: &ExecRequest,
) -> anyhow::Result<(SpawnOperation, tokio::process::Child)> {
    let operation = SpawnOperation {
        operation: Some(operation(request)),
    };
    let Ok(child) = jackin_process::spawn_async(request) else {
        operation.finish(OutcomeValue::Failure, Some(ErrorType::ProcessSpawnError));
        return Err(anyhow::anyhow!("process spawn failed"));
    };
    Ok((operation, child))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn exports_host_process_matrix_without_operator_material() {
        let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
        let _subscriber = tracing::subscriber::set_default(subscriber);

        exec_sync(&ExecRequest::new(
            "sh",
            [
                "-c",
                "printf operator-secret-stdout; printf operator-secret-stderr >&2",
            ],
        ))
        .unwrap();
        exec_async(&ExecRequest::new("docker", ["operator-secret-argument"]))
            .await
            .unwrap();
        exec_async(&ExecRequest::new("sh", ["-c", "sleep 1"]).timeout(Duration::from_millis(5)))
            .await
            .unwrap();
        let error = exec_sync(&ExecRequest::new(
            "/operator-secret/missing-command",
            ["operator-secret-spawn-argument"],
        ))
        .unwrap_err();
        assert_eq!(error.to_string(), "process spawn failed");

        assert!(
            exec_sync_optional(&ExecRequest::new(
                "/operator-secret/missing-viewer",
                ["operator-secret-viewer-argument"],
            ))
            .unwrap()
            .is_none()
        );
        {
            let request = ExecRequest::new("sh", ["-c", "exit 0"]);
            let (operation, mut child) = spawn_async(&request).unwrap();
            child.wait().await.unwrap();
            operation.complete_ready();
        }
        {
            let request = ExecRequest::new(
                "/operator-secret/missing-daemon",
                ["operator-secret-daemon-argument"],
            );
            let Err(error) = spawn_async(&request) else {
                panic!("missing daemon unexpectedly spawned");
            };
            assert_eq!(error.to_string(), "process spawn failed");
        }
        {
            let request = ExecRequest::new("sh", ["-c", "exit 0"]);
            let (_operation, mut child) = spawn_async(&request).unwrap();
            child.wait().await.unwrap();
        }

        export.force_flush();
        assert_eq!(export.finished_spans().len(), 8);
        assert_eq!(export.error_span_count(), 6);
        for expected in [
            "sh",
            "docker",
            "other",
            "process_exit_nonzero",
            "process_spawn_error",
            "telemetry_instrumentation_fault",
            "timeout",
        ] {
            assert!(export.contains_span_text(expected));
        }
        for secret in [
            "operator-secret-stdout",
            "operator-secret-stderr",
            "operator-secret-argument",
            "/operator-secret/missing-command",
            "operator-secret-spawn-argument",
            "/operator-secret/missing-viewer",
            "operator-secret-viewer-argument",
            "/operator-secret/missing-daemon",
            "operator-secret-daemon-argument",
        ] {
            assert!(!export.contains_span_text(secret));
        }
    }
}

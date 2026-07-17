// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_process::{ExecRequest, ExecResult};
use jackin_telemetry::schema::enums::{ErrorType, OutcomeValue, ProcessExecutableName};

pub(crate) struct ChildOperation {
    operation: Option<jackin_telemetry::operation::OperationGuard>,
}

impl ChildOperation {
    pub(crate) fn begin(executable: ProcessExecutableName) -> Self {
        Self {
            operation: Some(jackin_telemetry::operation_or_disabled(
                &jackin_telemetry::operation::PROCESS_COMMAND,
                &[jackin_telemetry::Attr {
                    key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXECUTABLE_NAME,
                    value: jackin_telemetry::Value::Str(executable.as_str()),
                }],
            )),
        }
    }

    pub(crate) fn complete_status(mut self, status: std::process::ExitStatus) {
        if let Some(code) = status.code()
            && let Some(operation) = self.operation.as_ref()
        {
            let _attribute = operation.set_attr(jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
                value: jackin_telemetry::Value::I64(i64::from(code)),
            });
        }
        self.complete(if status.success() {
            (OutcomeValue::Success, None)
        } else {
            (OutcomeValue::Failure, Some(ErrorType::ProcessExitNonzero))
        });
    }

    pub(crate) fn spawn_failed(mut self) {
        self.complete((OutcomeValue::Failure, Some(ErrorType::ProcessSpawnError)));
    }

    pub(crate) fn io_failed(mut self) {
        self.complete((OutcomeValue::Failure, Some(ErrorType::IoError)));
    }

    pub(crate) fn timed_out(mut self) {
        self.complete((OutcomeValue::Timeout, Some(ErrorType::Timeout)));
    }

    fn complete(&mut self, completion: (OutcomeValue, Option<ErrorType>)) {
        if let Some(operation) = self.operation.take() {
            operation.complete(completion.0, completion.1);
        }
    }
}

impl Drop for ChildOperation {
    fn drop(&mut self) {
        self.complete((
            OutcomeValue::Failure,
            Some(ErrorType::TelemetryInstrumentationFault),
        ));
    }
}

pub(crate) fn exec_sync_as(
    request: &ExecRequest,
    executable: ProcessExecutableName,
) -> anyhow::Result<ExecResult> {
    let operation = jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::PROCESS_COMMAND,
        &[jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXECUTABLE_NAME,
            value: jackin_telemetry::Value::Str(executable.as_str()),
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
    use std::time::Duration;

    use super::*;

    #[test]
    fn exports_claude_probe_matrix_without_host_material() {
        let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
        let _subscriber = tracing::subscriber::set_default(subscriber);

        exec_sync_as(
            &ExecRequest::new("sh", ["-c", "printf claude-secret-output"]),
            ProcessExecutableName::Claude,
        )
        .unwrap();
        exec_sync_as(
            &ExecRequest::new("sh", ["-c", "printf claude-secret-stderr >&2; exit 21"]),
            ProcessExecutableName::Claude,
        )
        .unwrap();
        exec_sync_as(
            &ExecRequest::new("sh", ["-c", "sleep 1"]).timeout(Duration::from_millis(5)),
            ProcessExecutableName::Claude,
        )
        .unwrap();
        let error = exec_sync_as(
            &ExecRequest::new("/claude-secret/missing-binary", ["claude-secret-argument"]),
            ProcessExecutableName::Claude,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "process spawn failed");

        export.force_flush();
        assert_eq!(export.finished_spans().len(), 4);
        assert_eq!(export.error_span_count(), 3);
        for expected in [
            "claude",
            "process_exit_nonzero",
            "process_spawn_error",
            "timeout",
        ] {
            assert!(export.contains_span_text(expected));
        }
        for secret in [
            "claude-secret-output",
            "claude-secret-stderr",
            "/claude-secret/missing-binary",
            "claude-secret-argument",
        ] {
            assert!(!export.contains_span_text(secret));
        }
    }
}

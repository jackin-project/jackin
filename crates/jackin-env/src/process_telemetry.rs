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

    pub(crate) fn complete_result(mut self, code: Option<i32>, success: bool) {
        if let Some(code) = code
            && let Some(operation) = self.operation.as_ref()
        {
            let _attribute = operation.set_attr(jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
                value: jackin_telemetry::Value::I64(i64::from(code)),
            });
        }
        self.complete(if success {
            (OutcomeValue::Success, None)
        } else {
            (OutcomeValue::Failure, Some(ErrorType::ProcessExitNonzero))
        });
    }

    pub(crate) fn complete_portable_status(mut self, status: &portable_pty::ExitStatus) {
        if let Some(operation) = self.operation.as_ref() {
            let _attribute = operation.set_attr(jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
                value: jackin_telemetry::Value::I64(i64::from(status.exit_code())),
            });
        }
        self.complete(if status.success() {
            (OutcomeValue::Success, None)
        } else {
            (OutcomeValue::Failure, Some(ErrorType::ProcessExitNonzero))
        });
    }

    pub(crate) fn succeeded(mut self) {
        self.complete((OutcomeValue::Success, None));
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

pub(crate) fn exec_sync_op_with_retry(
    request: &ExecRequest,
    attempts: usize,
) -> anyhow::Result<ExecResult> {
    assert!(attempts > 0, "attempt count must be nonzero");
    let operation = ChildOperation::begin(ProcessExecutableName::Op);
    for attempt in 0..attempts {
        match jackin_process::exec_sync(request) {
            Ok(output) => {
                if output.timed_out {
                    operation.timed_out();
                } else {
                    operation.complete_result(output.code, output.success);
                }
                return Ok(output);
            }
            Err(error)
                if attempt + 1 < attempts
                    && error.chain().any(|cause| {
                        cause
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|error| error.raw_os_error() == Some(26))
                    }) =>
            {
                #[expect(
                    clippy::disallowed_methods,
                    reason = "launch callers run 1Password spawn retries inside spawn_blocking"
                )]
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(_) => {
                operation.spawn_failed();
                return Err(anyhow::anyhow!("process spawn failed"));
            }
        }
    }
    unreachable!("attempt count is nonzero")
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

    #[test]
    fn exports_portable_pty_completion_statuses() {
        let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
        let _subscriber = tracing::subscriber::set_default(subscriber);

        ChildOperation::begin(ProcessExecutableName::Claude)
            .complete_portable_status(&portable_pty::ExitStatus::with_exit_code(0));
        ChildOperation::begin(ProcessExecutableName::Claude)
            .complete_portable_status(&portable_pty::ExitStatus::with_exit_code(19));

        export.force_flush();
        assert_eq!(export.finished_spans().len(), 2);
        assert_eq!(export.error_span_count(), 1);
        assert!(export.contains_span_text("claude"));
        assert!(export.contains_span_text("process_exit_nonzero"));
        assert!(export.contains_span_text("19"));
    }

    #[test]
    fn op_write_transport_bounds_stdin_execution_and_export() {
        let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
        let _subscriber = tracing::subscriber::set_default(subscriber);

        let mut success = ExecRequest::new("sh", ["-c", "cat"]);
        success.stdin = Some(b"op-write-secret-body".to_vec());
        success.timeout = Some(Duration::from_secs(1));
        let output = exec_sync_op_with_retry(&success, 1).unwrap();
        assert_eq!(output.stdout, b"op-write-secret-body");

        let mut timeout = ExecRequest::new("sh", ["-c", "sleep 1"]);
        timeout.stdin = Some(b"op-write-secret-timeout-body".to_vec());
        timeout.timeout = Some(Duration::from_millis(5));
        assert!(exec_sync_op_with_retry(&timeout, 1).unwrap().timed_out);

        export.force_flush();
        assert_eq!(export.finished_spans().len(), 2);
        assert_eq!(export.error_span_count(), 1);
        assert!(export.contains_span_text("op"));
        assert!(export.contains_span_text("timeout"));
        for secret in ["op-write-secret-body", "op-write-secret-timeout-body"] {
            assert!(!export.contains_span_text(secret));
        }
    }
}

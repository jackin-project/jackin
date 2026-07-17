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
mod tests;

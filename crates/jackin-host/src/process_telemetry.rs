// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_process::{ExecRequest, ExecResult};
use jackin_telemetry::schema::enums::{ErrorType, OutcomeValue};
use std::process::ExitStatus;

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

pub(crate) fn exec_sync(request: &ExecRequest) -> anyhow::Result<ExecResult> {
    let operation = operation(request);
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

pub(crate) struct ChildOperation {
    operation: Option<jackin_telemetry::OperationGuard>,
}

impl ChildOperation {
    fn finish(mut self, outcome: OutcomeValue, error_type: Option<ErrorType>) {
        if let Some(operation) = self.operation.take() {
            operation.complete(outcome, error_type);
        }
    }

    pub(crate) fn complete_status(self, status: ExitStatus) {
        if let Some(code) = status.code()
            && let Some(operation) = self.operation.as_ref()
        {
            let _attribute = operation.set_attr(jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
                value: jackin_telemetry::Value::I64(i64::from(code)),
            });
        }
        if status.success() {
            self.finish(OutcomeValue::Success, None);
        } else {
            self.finish(OutcomeValue::Failure, Some(ErrorType::ProcessExitNonzero));
        }
    }

    pub(crate) fn complete_io_failure(self) {
        self.finish(OutcomeValue::Failure, Some(ErrorType::IoError));
    }

    pub(crate) fn complete_cancelled(self) {
        self.finish(OutcomeValue::Cancellation, None);
    }
}

impl Drop for ChildOperation {
    fn drop(&mut self) {
        if let Some(operation) = self.operation.take() {
            operation.complete(
                OutcomeValue::Failure,
                Some(ErrorType::TelemetryInstrumentationFault),
            );
        }
    }
}

pub(crate) fn spawn_sync(
    request: &ExecRequest,
) -> anyhow::Result<(ChildOperation, std::process::Child)> {
    let operation = ChildOperation {
        operation: Some(operation(request)),
    };
    let Ok(child) = jackin_process::spawn_sync(request) else {
        operation.finish(OutcomeValue::Failure, Some(ErrorType::ProcessSpawnError));
        return Err(anyhow::anyhow!("process spawn failed"));
    };
    Ok((operation, child))
}

#[cfg(test)]
mod tests;

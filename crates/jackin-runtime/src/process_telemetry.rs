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

pub(crate) struct ChildOperation {
    operation: Option<jackin_telemetry::OperationGuard>,
}

impl ChildOperation {
    pub(crate) fn begin(request: &ExecRequest) -> Self {
        Self {
            operation: Some(operation(request)),
        }
    }

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

    pub(crate) fn complete_timeout(self) {
        self.finish(OutcomeValue::Timeout, Some(ErrorType::Timeout));
    }

    pub(crate) fn complete_failure(self, error_type: ErrorType) {
        self.finish(OutcomeValue::Failure, Some(error_type));
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
    let operation = ChildOperation::begin(request);
    let Ok(child) = jackin_process::spawn_sync(request) else {
        operation.complete_failure(ErrorType::ProcessSpawnError);
        return Err(anyhow::anyhow!("process spawn failed"));
    };
    Ok((operation, child))
}

pub(crate) fn spawn_async(
    request: &ExecRequest,
) -> anyhow::Result<(ChildOperation, tokio::process::Child)> {
    let operation = ChildOperation::begin(request);
    let Ok(child) = jackin_process::spawn_async(request) else {
        operation.complete_failure(ErrorType::ProcessSpawnError);
        return Err(anyhow::anyhow!("process spawn failed"));
    };
    Ok((operation, child))
}

pub(crate) fn exec_sync(request: &ExecRequest) -> anyhow::Result<ExecResult> {
    let operation = operation(request);
    let result = jackin_process::exec_sync(request);
    complete(operation, &result);
    result.map_err(|_| anyhow::anyhow!("process spawn failed"))
}

pub(crate) async fn exec_async(request: &ExecRequest) -> anyhow::Result<ExecResult> {
    let operation = operation(request);
    let result = jackin_process::exec_async(request).await;
    complete(operation, &result);
    result.map_err(|_| anyhow::anyhow!("process spawn failed"))
}

#[cfg(test)]
mod tests;

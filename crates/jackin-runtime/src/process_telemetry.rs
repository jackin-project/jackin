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
    result
}

pub(crate) async fn exec_async(request: &ExecRequest) -> anyhow::Result<ExecResult> {
    let operation = operation(request);
    let result = jackin_process::exec_async(request).await;
    complete(operation, &result);
    result
}

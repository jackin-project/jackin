// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_process::{ExecRequest, ExecResult};
use jackin_telemetry::schema::enums::{ErrorType, OutcomeValue, ProcessExecutableName};
#[cfg(target_os = "macos")]
use std::ops::{Deref, DerefMut};
use std::process::ExitStatus;

#[cfg(target_os = "macos")]
pub(crate) struct RegisteredStdChild {
    child: std::process::Child,
    registration: jackin_process::ChildRegistration,
}

#[cfg(target_os = "macos")]
impl RegisteredStdChild {
    fn new(child: std::process::Child, registration: jackin_process::ChildRegistration) -> Self {
        Self {
            child,
            registration,
        }
    }

    pub(crate) fn wait_with_output(self) -> std::io::Result<std::process::Output> {
        let Self {
            child,
            registration,
        } = self;
        let output = child.wait_with_output();
        drop(registration);
        output
    }
}

#[cfg(target_os = "macos")]
impl Deref for RegisteredStdChild {
    type Target = std::process::Child;

    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

#[cfg(target_os = "macos")]
impl DerefMut for RegisteredStdChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) type RegisteredStdChild = std::process::Child;

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

pub(crate) struct ChildOperation {
    operation: Option<jackin_telemetry::OperationGuard>,
}

impl ChildOperation {
    fn begin(request: &ExecRequest) -> Self {
        Self {
            operation: Some(operation(request, None)),
        }
    }

    fn finish(mut self, outcome: OutcomeValue, error_type: Option<ErrorType>) {
        if let Some(operation) = self.operation.take() {
            operation.complete(outcome, error_type);
        }
    }

    pub(crate) fn complete_status(self, status: ExitStatus, accepted: &[i32]) {
        if let Some(code) = status.code()
            && let Some(operation) = self.operation.as_ref()
        {
            let _attribute = operation.set_attr(jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
                value: jackin_telemetry::Value::I64(i64::from(code)),
            });
        }
        if status.code().is_some_and(|code| accepted.contains(&code)) {
            self.finish(OutcomeValue::Success, None);
        } else {
            self.finish(OutcomeValue::Failure, Some(ErrorType::ProcessExitNonzero));
        }
    }

    pub(crate) fn complete_reaped(self) {
        self.finish(OutcomeValue::Success, None);
    }

    pub(crate) fn complete_timeout(self) {
        self.finish(OutcomeValue::Timeout, Some(ErrorType::Timeout));
    }

    pub(crate) fn complete_io_failure(self) {
        self.finish(OutcomeValue::Failure, Some(ErrorType::IoError));
    }

    fn complete_spawn_failure(self) {
        self.finish(OutcomeValue::Failure, Some(ErrorType::ProcessSpawnError));
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
) -> anyhow::Result<(ChildOperation, RegisteredStdChild)> {
    let operation = ChildOperation::begin(request);
    #[cfg(target_os = "macos")]
    let spawn_result = jackin_process::coordinate_children(|registry| {
        let slot = registry
            .prepare_registration()
            .map_err(anyhow::Error::from)?;
        let child = jackin_process::spawn_sync(request)?;
        let registration = registry.register_prepared(child.id(), slot);
        Ok::<_, anyhow::Error>(RegisteredStdChild::new(child, registration))
    });
    #[cfg(not(target_os = "macos"))]
    let spawn_result = jackin_process::spawn_sync(request);
    let Ok(child) = spawn_result else {
        operation.complete_spawn_failure();
        return Err(anyhow::anyhow!("process spawn failed"));
    };
    Ok((operation, child))
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

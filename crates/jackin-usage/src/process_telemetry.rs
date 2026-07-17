// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_telemetry::schema::enums::{ErrorType, OutcomeValue, ProcessExecutableName};

pub(crate) struct ChildOperation {
    operation: Option<jackin_telemetry::operation::OperationGuard>,
}

impl ChildOperation {
    pub(crate) fn begin(command: &str) -> Self {
        let executable = match std::path::Path::new(command)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
        {
            Some("claude") => ProcessExecutableName::Claude,
            Some("amp") => ProcessExecutableName::Amp,
            Some("codex") => ProcessExecutableName::Codex,
            Some("grok") => ProcessExecutableName::Grok,
            _ => ProcessExecutableName::Other,
        };
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

    pub(crate) fn complete_status(mut self, code: Option<i32>, success: bool) {
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

    pub(crate) fn spawn_failed(mut self) {
        self.complete((OutcomeValue::Failure, Some(ErrorType::ProcessSpawnError)));
    }

    pub(crate) fn io_failed(mut self) {
        self.complete((OutcomeValue::Failure, Some(ErrorType::IoError)));
    }

    pub(crate) fn timed_out(mut self) {
        self.complete((OutcomeValue::Timeout, Some(ErrorType::Timeout)));
    }

    pub(crate) fn reap_managed(child: &mut std::process::Child) -> bool {
        let killed = child.kill().is_ok();
        let reaped = child.wait().is_ok();
        killed && reaped
    }

    pub(crate) fn finish_managed(mut self, succeeded: bool) {
        self.complete(if succeeded {
            (OutcomeValue::Success, None)
        } else {
            (OutcomeValue::Failure, Some(ErrorType::IoError))
        });
    }

    pub(crate) fn fail_managed_io(mut self, child: &mut std::process::Child) {
        drop(child.kill());
        drop(child.wait());
        self.complete((OutcomeValue::Failure, Some(ErrorType::IoError)));
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

pub(crate) fn external_rpc_operation(
    system: jackin_telemetry::schema::enums::RpcSystemName,
    method: &str,
) -> jackin_telemetry::OperationGuard {
    jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::RPC_CLIENT,
        &[
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
                value: jackin_telemetry::Value::Str(system.as_str()),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
                value: jackin_telemetry::Value::Str(method),
            },
        ],
    )
}

pub(crate) fn complete_external_rpc<T>(
    operation: jackin_telemetry::OperationGuard,
    result: &Result<T, String>,
    timed_out: bool,
) {
    operation.complete(
        if result.is_ok() {
            OutcomeValue::Success
        } else if timed_out {
            OutcomeValue::Timeout
        } else {
            OutcomeValue::Failure
        },
        if result.is_ok() {
            None
        } else if timed_out {
            Some(ErrorType::Timeout)
        } else {
            Some(ErrorType::RpcError)
        },
    );
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Product-binary invocation policy and bounded CLI lifecycle telemetry.

use std::time::Instant;

use jackin_telemetry::schema::enums::{AppMode, CliCommandName, ErrorType, OutcomeValue};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryKind {
    Host,
    Role,
    BuildCapsuleDeveloperTool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecyclePolicy {
    Product(AppMode),
    DeveloperExcluded,
}

#[must_use]
pub const fn lifecycle_policy(binary: BinaryKind) -> LifecyclePolicy {
    match binary {
        BinaryKind::Host => LifecyclePolicy::Product(AppMode::OneShot),
        BinaryKind::Role => LifecyclePolicy::Product(AppMode::OneShot),
        BinaryKind::BuildCapsuleDeveloperTool => LifecyclePolicy::DeveloperExcluded,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProductLifecycle {
    invocation_id: jackin_telemetry::identity::InvocationId,
    started_at: Instant,
}

impl ProductLifecycle {
    #[must_use]
    pub fn begin(binary: BinaryKind) -> Self {
        assert!(
            matches!(lifecycle_policy(binary), LifecyclePolicy::Product(_)),
            "developer binaries do not start product lifecycle telemetry"
        );
        let invocation_id = jackin_telemetry::identity::InvocationId::mint();
        let _set_result = jackin_telemetry::identity::set_current_invocation(invocation_id);
        jackin_diagnostics::install_host_panic_hook();
        Self {
            invocation_id,
            started_at: Instant::now(),
        }
    }

    #[must_use]
    pub const fn invocation_id(self) -> jackin_telemetry::identity::InvocationId {
        self.invocation_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResultClassification {
    pub exit_code: i64,
    pub outcome: OutcomeValue,
    pub error_type: Option<ErrorType>,
}

impl ResultClassification {
    pub const SUCCESS: Self = Self {
        exit_code: 0,
        outcome: OutcomeValue::Success,
        error_type: None,
    };
    pub const CANCELLATION: Self = Self {
        exit_code: 0,
        outcome: OutcomeValue::Cancellation,
        error_type: None,
    };

    #[must_use]
    pub const fn failed(self) -> bool {
        matches!(
            self.outcome,
            OutcomeValue::Failure | OutcomeValue::Error | OutcomeValue::Timeout
        )
    }
}

#[must_use]
pub(crate) fn classify_result(result: &anyhow::Result<()>) -> ResultClassification {
    let Err(error) = result else {
        return ResultClassification::SUCCESS;
    };
    classify_error(error)
}

#[must_use]
pub fn classify_error(error: &anyhow::Error) -> ResultClassification {
    if jackin_runtime::runtime::progress::LaunchCancelled::is_cancel(error) {
        return ResultClassification::CANCELLATION;
    }
    if let Some(jackin_error) = error.downcast_ref::<crate::error::JackinError>() {
        return ResultClassification {
            exit_code: 1,
            outcome: OutcomeValue::Failure,
            error_type: Some(jackin_error.user_message().code.telemetry_error()),
        };
    }
    if let Some(io_error) = error
        .chain()
        .find_map(|source| source.downcast_ref::<std::io::Error>())
    {
        return match io_error.kind() {
            std::io::ErrorKind::TimedOut => ResultClassification {
                exit_code: 1,
                outcome: OutcomeValue::Timeout,
                error_type: Some(ErrorType::Timeout),
            },
            std::io::ErrorKind::ConnectionRefused => ResultClassification {
                exit_code: 1,
                outcome: OutcomeValue::Failure,
                error_type: Some(ErrorType::ConnectionRefused),
            },
            _ => ResultClassification {
                exit_code: 1,
                outcome: OutcomeValue::Error,
                error_type: Some(ErrorType::LaunchFailed),
            },
        };
    }
    ResultClassification {
        exit_code: 1,
        outcome: OutcomeValue::Error,
        error_type: Some(ErrorType::LaunchFailed),
    }
}

#[must_use]
pub fn classify_parse_error(error: &clap::Error) -> ResultClassification {
    use clap::error::ErrorKind;
    match error.kind() {
        ErrorKind::DisplayHelp
        | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        | ErrorKind::DisplayVersion => ResultClassification::SUCCESS,
        _ => ResultClassification {
            exit_code: 2,
            outcome: OutcomeValue::Failure,
            error_type: Some(ErrorType::ConfigError),
        },
    }
}

#[derive(Debug)]
pub struct InvocationTelemetry {
    command: CliCommandName,
    started_at: Instant,
    roots: InvocationRoots,
}

#[derive(Debug)]
enum InvocationRoots {
    OneShot(Option<jackin_telemetry::operation::OperationGuard>),
    Interactive {
        attrs: RootAttrs,
        startup: Option<jackin_telemetry::operation::OperationGuard>,
        shutdown: Option<jackin_telemetry::operation::OperationGuard>,
    },
}

#[derive(Clone, Debug)]
struct RootAttrs {
    command: String,
    invocation: String,
}

impl RootAttrs {
    fn values(&self) -> [jackin_telemetry::Attr<'_>; 2] {
        [
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CLI_COMMAND_NAME,
                value: jackin_telemetry::Value::Str(&self.command),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CLI_INVOCATION_ID,
                value: jackin_telemetry::Value::Str(&self.invocation),
            },
        ]
    }
}

impl InvocationTelemetry {
    #[must_use]
    pub fn start(lifecycle: ProductLifecycle, command: CliCommandName, app_mode: AppMode) -> Self {
        let attrs = RootAttrs {
            command: command.as_str().to_owned(),
            invocation: lifecycle.invocation_id.to_string(),
        };
        let roots = if app_mode == AppMode::Interactive {
            InvocationRoots::Interactive {
                startup: jackin_telemetry::root_operation(
                    &jackin_telemetry::operation::APP_STARTUP,
                    &attrs.values(),
                )
                .ok(),
                shutdown: None,
                attrs,
            }
        } else {
            InvocationRoots::OneShot(
                jackin_telemetry::root_operation(
                    &jackin_telemetry::operation::CLI_COMMAND,
                    &attrs.values(),
                )
                .ok(),
            )
        };
        Self {
            command,
            started_at: lifecycle.started_at,
            roots,
        }
    }

    #[must_use]
    pub fn span(&self) -> tracing::Span {
        match &self.roots {
            InvocationRoots::OneShot(operation) => operation
                .as_ref()
                .map_or_else(tracing::Span::none, |operation| operation.span().clone()),
            InvocationRoots::Interactive { .. } => tracing::Span::none(),
        }
    }

    pub fn ready(&mut self) {
        if let InvocationRoots::Interactive { startup, .. } = &mut self.roots
            && let Some(startup) = startup.take()
        {
            startup.complete(OutcomeValue::Success, None);
        }
    }

    pub fn exit_requested(&mut self) {
        if let InvocationRoots::Interactive {
            attrs, shutdown, ..
        } = &mut self.roots
            && shutdown.is_none()
        {
            *shutdown = jackin_telemetry::root_operation(
                &jackin_telemetry::operation::APP_SHUTDOWN,
                &attrs.values(),
            )
            .ok();
        }
    }

    pub fn finish(mut self, result: &anyhow::Result<()>) -> ResultClassification {
        let classification = classify_result(result);
        match &mut self.roots {
            InvocationRoots::OneShot(operation) => {
                if let Some(operation) = operation.take() {
                    complete_root(operation, classification);
                }
            }
            InvocationRoots::Interactive {
                attrs,
                startup,
                shutdown,
            } => {
                if let Some(startup) = startup.take() {
                    complete_root(startup, classification);
                }
                if shutdown.is_none() {
                    *shutdown = jackin_telemetry::root_operation(
                        &jackin_telemetry::operation::APP_SHUTDOWN,
                        &attrs.values(),
                    )
                    .ok();
                }
                if let Some(shutdown) = shutdown.take() {
                    complete_root(shutdown, classification);
                }
            }
        }
        self.record_metrics(classification);
        classification
    }

    fn record_metrics(&self, classification: ResultClassification) {
        let command = self.command.as_str();
        let mut attrs = vec![
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CLI_COMMAND_NAME,
                value: jackin_telemetry::Value::Str(command),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::OUTCOME,
                value: jackin_telemetry::Value::Str(classification.outcome.as_str()),
            },
        ];
        if let Some(error_type) = classification.error_type {
            attrs.push(jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
                value: jackin_telemetry::Value::Str(error_type.as_str()),
            });
        }
        let _invocation =
            jackin_telemetry::counter(&jackin_telemetry::metric::CLI_INVOCATIONS).add(1, &attrs);
        let _duration = jackin_telemetry::histogram(&jackin_telemetry::metric::CLI_DURATION)
            .record(self.started_at.elapsed().as_secs_f64(), &attrs);
        if classification.failed() {
            let _failure =
                jackin_telemetry::counter(&jackin_telemetry::metric::CLI_FAILURES).add(1, &attrs);
        }
    }
}

fn complete_root(
    operation: jackin_telemetry::operation::OperationGuard,
    classification: ResultClassification,
) {
    let _exit_code = operation.set_attr(jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
        value: jackin_telemetry::Value::I64(classification.exit_code),
    });
    operation.complete(classification.outcome, classification.error_type);
}

#[cfg(test)]
mod tests;

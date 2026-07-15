// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use opentelemetry::trace::{SpanContext, Status};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

use crate::{
    event::{Attr, Rejection, Value},
    health, limits, privacy, schema,
};

#[derive(Clone, Copy, Debug)]
pub struct SpanDef {
    pub name: &'static str,
}

pub const CLI_COMMAND: SpanDef = SpanDef {
    name: schema::spans::CLI_COMMAND,
};
pub const APP_STARTUP: SpanDef = SpanDef {
    name: schema::spans::APP_STARTUP,
};
pub const APP_SHUTDOWN: SpanDef = SpanDef {
    name: schema::spans::APP_SHUTDOWN,
};
pub const UI_ACTION: SpanDef = SpanDef {
    name: schema::spans::UI_ACTION,
};
pub const UI_SCREEN_TRANSITION: SpanDef = SpanDef {
    name: schema::spans::UI_SCREEN_TRANSITION,
};
pub const UI_RENDER: SpanDef = SpanDef {
    name: schema::spans::UI_RENDER,
};
pub const BACKGROUND_CYCLE: SpanDef = SpanDef {
    name: schema::spans::BACKGROUND_CYCLE,
};
pub const CONNECTION_ATTEMPT: SpanDef = SpanDef {
    name: schema::spans::CONNECTION_ATTEMPT,
};
pub const PROCESS_COMMAND: SpanDef = SpanDef {
    name: schema::spans::PROCESS_COMMAND,
};

#[derive(Debug)]
pub struct OperationGuard {
    span: Span,
    completed: AtomicBool,
    links: AtomicUsize,
}

impl OperationGuard {
    #[must_use]
    pub fn span(&self) -> &Span {
        &self.span
    }

    pub fn set_attr(&self, attr: Attr<'_>) -> Result<(), Rejection> {
        if let Err(reason) =
            privacy::validate_key(attr.key).and_then(|()| limits::validate_value(&attr.value))
        {
            health::reject(reason);
            return Err(reason);
        }
        let value = match attr.value {
            Value::Str(value) => opentelemetry::Value::String(value.to_owned().into()),
            Value::Bool(value) => opentelemetry::Value::Bool(value),
            Value::I64(value) => opentelemetry::Value::I64(value),
            Value::U64(value) => {
                opentelemetry::Value::I64(i64::try_from(value).unwrap_or(i64::MAX))
            }
            Value::F64(value) => opentelemetry::Value::F64(value),
            Value::StrArray(values) => opentelemetry::Value::Array(opentelemetry::Array::String(
                values.iter().map(|v| (*v).to_owned().into()).collect(),
            )),
        };
        self.span.set_attribute(attr.key, value);
        Ok(())
    }

    pub fn link(&self, context: &SpanContext) -> Result<(), Rejection> {
        if self.links.fetch_add(1, Ordering::Relaxed) >= limits::MAX_SPAN_LINKS {
            self.links.fetch_sub(1, Ordering::Relaxed);
            health::reject(Rejection::SizeLimit);
            return Err(Rejection::SizeLimit);
        }
        self.span.add_link(context.clone());
        Ok(())
    }

    pub fn complete(self, outcome: schema::enums::OutcomeValue, error_type: Option<&'static str>) {
        self.record_completion(outcome, error_type);
        self.completed.store(true, Ordering::Release);
    }

    fn record_completion(
        &self,
        outcome: schema::enums::OutcomeValue,
        error_type: Option<&'static str>,
    ) {
        self.span
            .set_attribute(schema::attrs::OUTCOME, outcome.as_str());
        if let Some(error_type) = error_type {
            self.span
                .set_attribute(schema::attrs::std_attrs::ERROR_TYPE, error_type);
        }
        if matches!(
            outcome,
            schema::enums::OutcomeValue::Failure
                | schema::enums::OutcomeValue::Error
                | schema::enums::OutcomeValue::Timeout
        ) {
            self.span.set_status(Status::error(outcome.as_str()));
        }
    }
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        if !self.completed.load(Ordering::Acquire) {
            self.record_completion(schema::enums::OutcomeValue::Cancellation, None);
        }
    }
}

fn make_span(name: &str, root: bool) -> Option<Span> {
    if root {
        return Some(match name {
            schema::spans::CLI_COMMAND => {
                tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "cli.command")
            }
            schema::spans::APP_STARTUP => {
                tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "app.startup")
            }
            schema::spans::APP_SHUTDOWN => {
                tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "app.shutdown")
            }
            schema::spans::UI_ACTION => {
                tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "ui.action")
            }
            schema::spans::UI_SCREEN_TRANSITION => {
                tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "ui.screen.transition")
            }
            schema::spans::UI_RENDER => {
                tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "ui.render")
            }
            schema::spans::BACKGROUND_CYCLE => {
                tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "background.cycle")
            }
            schema::spans::CONNECTION_ATTEMPT => {
                tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "connection.attempt")
            }
            schema::spans::PROCESS_COMMAND => {
                tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "process.command")
            }
            _ => return None,
        });
    }
    Some(match name {
        schema::spans::CLI_COMMAND => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "cli.command")
        }
        schema::spans::APP_STARTUP => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "app.startup")
        }
        schema::spans::APP_SHUTDOWN => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "app.shutdown")
        }
        schema::spans::UI_ACTION => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "ui.action")
        }
        schema::spans::UI_SCREEN_TRANSITION => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "ui.screen.transition")
        }
        schema::spans::UI_RENDER => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "ui.render")
        }
        schema::spans::BACKGROUND_CYCLE => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "background.cycle")
        }
        schema::spans::CONNECTION_ATTEMPT => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "connection.attempt")
        }
        schema::spans::PROCESS_COMMAND => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "process.command")
        }
        _ => return None,
    })
}

#[must_use]
pub fn operation(def: &'static SpanDef, attrs: &[Attr<'_>]) -> Result<OperationGuard, Rejection> {
    if attrs.len() > limits::MAX_SPAN_ATTRIBUTES {
        health::reject(Rejection::SizeLimit);
        return Err(Rejection::SizeLimit);
    }
    operation_inner(def, attrs, false)
}

pub(crate) fn operation_root(
    def: &'static SpanDef,
    attrs: &[Attr<'_>],
) -> Result<OperationGuard, Rejection> {
    operation_inner(def, attrs, true)
}

fn operation_inner(
    def: &'static SpanDef,
    attrs: &[Attr<'_>],
    root: bool,
) -> Result<OperationGuard, Rejection> {
    if attrs.len() > limits::MAX_SPAN_ATTRIBUTES {
        health::reject(Rejection::SizeLimit);
        return Err(Rejection::SizeLimit);
    }
    let Some(span) = make_span(def.name, root) else {
        health::reject(Rejection::UnknownName);
        return Err(Rejection::UnknownName);
    };
    let guard = OperationGuard {
        span,
        completed: AtomicBool::new(false),
        links: AtomicUsize::new(0),
    };
    for attr in attrs {
        guard.set_attr(*attr)?;
    }
    Ok(guard)
}

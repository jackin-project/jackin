// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant,
};

use opentelemetry::trace::{SpanContext, Status};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

use crate::{
    event::{Attr, Rejection, Value},
    health, limits, schema, validation,
};

#[derive(Clone, Copy, Debug)]
pub struct SpanDef {
    pub(crate) name: &'static str,
    pub(crate) metadata: &'static schema::SpanMetadata,
}

macro_rules! span_def {
    ($constant:ident, $definition:ident) => {
        pub const $constant: SpanDef = SpanDef {
            name: schema::spans::$constant,
            metadata: &schema::spans::$definition,
        };
    };
}

span_def!(CLI_COMMAND, CLI_COMMAND_DEF);
/*
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
pub const PREWARM_SCHEDULE: SpanDef = SpanDef {
    name: schema::spans::PREWARM_SCHEDULE,
};
pub const PREWARM_ATTEMPT: SpanDef = SpanDef {
    name: schema::spans::PREWARM_ATTEMPT,
};
pub const CONNECTION_ATTEMPT: SpanDef = SpanDef {
    name: schema::spans::CONNECTION_ATTEMPT,
};
pub const PROCESS_COMMAND: SpanDef = SpanDef {
    name: schema::spans::PROCESS_COMMAND,
};
pub const LAUNCH: SpanDef = SpanDef {
    name: schema::spans::LAUNCH,
};
pub const LAUNCH_STAGE: SpanDef = SpanDef {
    name: schema::spans::LAUNCH_STAGE,
};
pub const HTTP_CLIENT: SpanDef = SpanDef {
    name: schema::spans::HTTP_CLIENT,
};
pub const DB_CLIENT: SpanDef = SpanDef {
    name: schema::spans::DB_CLIENT,
};
pub const RPC_CLIENT: SpanDef = SpanDef {
    name: schema::spans::RPC_CLIENT,
};
pub const RPC_SERVER: SpanDef = SpanDef {
    name: schema::spans::RPC_SERVER,
};
pub const TELEMETRY_VALIDATE: SpanDef = SpanDef {
    name: schema::spans::TELEMETRY_VALIDATE,
};
*/
span_def!(APP_STARTUP, APP_STARTUP_DEF);
span_def!(APP_SHUTDOWN, APP_SHUTDOWN_DEF);
span_def!(UI_ACTION, UI_ACTION_DEF);
span_def!(UI_SCREEN_TRANSITION, UI_SCREEN_TRANSITION_DEF);
span_def!(UI_RENDER, UI_RENDER_DEF);
span_def!(BACKGROUND_CYCLE, BACKGROUND_CYCLE_DEF);
span_def!(PREWARM_SCHEDULE, PREWARM_SCHEDULE_DEF);
span_def!(PREWARM_ATTEMPT, PREWARM_ATTEMPT_DEF);
span_def!(CONNECTION_ATTEMPT, CONNECTION_ATTEMPT_DEF);
span_def!(PROCESS_COMMAND, PROCESS_COMMAND_DEF);
span_def!(LAUNCH, LAUNCH_DEF);
span_def!(LAUNCH_STAGE, LAUNCH_STAGE_DEF);
span_def!(HTTP_CLIENT, HTTP_CLIENT_DEF);
span_def!(DB_CLIENT, DB_CLIENT_DEF);
span_def!(RPC_CLIENT, RPC_CLIENT_DEF);
span_def!(RPC_SERVER, RPC_SERVER_DEF);
span_def!(TELEMETRY_VALIDATE, TELEMETRY_VALIDATE_DEF);

#[derive(Debug)]
pub struct OperationGuard {
    definition: &'static SpanDef,
    span: Span,
    completed: AtomicBool,
    links: AtomicUsize,
    attributes: AtomicUsize,
    attribute_keys: Mutex<Vec<&'static str>>,
    rpc: Option<RpcLifecycle>,
    connection: Option<ConnectionLifecycle>,
}

#[derive(Debug)]
struct ConnectionLifecycle {
    peer: String,
    started_at: Instant,
}

impl ConnectionLifecycle {
    fn start(peer: &str) -> Self {
        let attrs = [Attr {
            key: schema::attrs::CONNECTION_PEER_TYPE,
            value: Value::Str(peer),
        }];
        let _active_result =
            crate::up_down_counter(&crate::metric::CONNECTION_ACTIVE).add(1, &attrs);
        Self {
            peer: peer.to_owned(),
            started_at: Instant::now(),
        }
    }

    fn finish(
        &self,
        outcome: schema::enums::OutcomeValue,
        error_type: Option<schema::enums::ErrorType>,
    ) {
        let active_attrs = [Attr {
            key: schema::attrs::CONNECTION_PEER_TYPE,
            value: Value::Str(&self.peer),
        }];
        let _active_result =
            crate::up_down_counter(&crate::metric::CONNECTION_ACTIVE).add(-1, &active_attrs);

        let mut attrs = vec![
            Attr {
                key: schema::attrs::CONNECTION_PEER_TYPE,
                value: Value::Str(&self.peer),
            },
            Attr {
                key: schema::attrs::OUTCOME,
                value: Value::Str(outcome.as_str()),
            },
        ];
        if let Some(error_type) = error_type {
            attrs.push(Attr {
                key: schema::attrs::std_attrs::ERROR_TYPE,
                value: Value::Str(error_type.as_str()),
            });
        }
        let _attempt_result = crate::counter(&crate::metric::CONNECTION_ATTEMPTS).add(1, &attrs);
        let _duration_result = crate::histogram(&crate::metric::CONNECTION_DURATION)
            .record(self.started_at.elapsed().as_secs_f64(), &attrs);
    }
}

#[derive(Debug)]
struct RpcLifecycle {
    method: String,
    started_at: Instant,
}

impl RpcLifecycle {
    fn start(method: &str) -> Self {
        let attrs = [Attr {
            key: schema::attrs::std_attrs::RPC_METHOD,
            value: Value::Str(method),
        }];
        let _active_result = crate::up_down_counter(&crate::metric::RPC_ACTIVE).add(1, &attrs);
        Self {
            method: method.to_owned(),
            started_at: Instant::now(),
        }
    }

    fn finish(
        &self,
        outcome: schema::enums::OutcomeValue,
        error_type: Option<schema::enums::ErrorType>,
    ) {
        let active_attrs = [Attr {
            key: schema::attrs::std_attrs::RPC_METHOD,
            value: Value::Str(&self.method),
        }];
        let _active_result =
            crate::up_down_counter(&crate::metric::RPC_ACTIVE).add(-1, &active_attrs);

        let mut attrs = vec![
            Attr {
                key: schema::attrs::std_attrs::RPC_METHOD,
                value: Value::Str(&self.method),
            },
            Attr {
                key: schema::attrs::OUTCOME,
                value: Value::Str(outcome.as_str()),
            },
        ];
        if let Some(error_type) = error_type {
            attrs.push(Attr {
                key: schema::attrs::std_attrs::ERROR_TYPE,
                value: Value::Str(error_type.as_str()),
            });
        }
        let _request_result = crate::counter(&crate::metric::RPC_REQUESTS).add(1, &attrs);
        let _duration_result = crate::histogram(&crate::metric::RPC_DURATION)
            .record(self.started_at.elapsed().as_secs_f64(), &attrs);
    }
}

impl OperationGuard {
    fn disabled() -> Self {
        Self {
            definition: &TELEMETRY_VALIDATE,
            span: Span::none(),
            completed: AtomicBool::new(false),
            links: AtomicUsize::new(0),
            attributes: AtomicUsize::new(0),
            attribute_keys: Mutex::new(Vec::new()),
            rpc: None,
            connection: None,
        }
    }

    #[must_use]
    pub fn span(&self) -> &Span {
        &self.span
    }

    pub fn set_attr(&self, attr: Attr<'_>) -> Result<(), Rejection> {
        if let Err(reason) = validation::attribute(self.definition.metadata.attributes, attr) {
            health::reject(health::Signal::Trace, reason);
            return Err(reason);
        }
        let mut keys = self
            .attribute_keys
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if keys.contains(&attr.key) {
            health::reject(health::Signal::Trace, Rejection::InvalidValue);
            return Err(Rejection::InvalidValue);
        }
        if self.attributes.load(Ordering::Relaxed) >= limits::MAX_SPAN_ATTRIBUTES - 2 {
            health::reject(health::Signal::Trace, Rejection::SizeLimit);
            return Err(Rejection::SizeLimit);
        }
        keys.push(attr.key);
        self.attributes.fetch_add(1, Ordering::Relaxed);
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
            health::reject(health::Signal::Trace, Rejection::SizeLimit);
            return Err(Rejection::SizeLimit);
        }
        self.span.add_link(context.clone());
        Ok(())
    }

    pub fn complete(
        self,
        outcome: schema::enums::OutcomeValue,
        error_type: Option<schema::enums::ErrorType>,
    ) {
        if !valid_completion(outcome, error_type) {
            health::reject(health::Signal::Trace, Rejection::InvalidValue);
            self.record_completion(
                schema::enums::OutcomeValue::Error,
                Some(schema::enums::ErrorType::TelemetryInstrumentationFault),
            );
            self.completed.store(true, Ordering::Release);
            return;
        }
        self.record_completion(outcome, error_type);
        self.completed.store(true, Ordering::Release);
    }

    fn record_completion(
        &self,
        outcome: schema::enums::OutcomeValue,
        error_type: Option<schema::enums::ErrorType>,
    ) {
        let failure = matches!(
            outcome,
            schema::enums::OutcomeValue::Failure
                | schema::enums::OutcomeValue::Error
                | schema::enums::OutcomeValue::Timeout
        );
        self.span
            .set_attribute(schema::attrs::OUTCOME, outcome.as_str());
        if let Some(error_type) = error_type {
            self.span
                .set_attribute(schema::attrs::std_attrs::ERROR_TYPE, error_type.as_str());
        }
        if failure {
            let description = limits::redact_and_clamp(
                error_type.map_or(outcome.as_str(), schema::enums::ErrorType::as_str),
            );
            self.span
                .set_status(Status::error(description.into_owned()));
        }
        if let Some(rpc) = &self.rpc {
            rpc.finish(outcome, error_type);
        }
        if let Some(connection) = &self.connection {
            connection.finish(outcome, error_type);
        }
        if error_type == Some(schema::enums::ErrorType::RecoveredDegradation) {
            let attrs = [Attr {
                key: schema::attrs::OUTCOME,
                value: Value::Str(outcome.as_str()),
            }];
            let _warning = crate::emit_event(
                &crate::event::OPERATION_WARN,
                crate::event::FieldSet::new(&attrs, None),
            );
        }
    }
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        if !self.completed.load(Ordering::Acquire) {
            self.record_completion(
                schema::enums::OutcomeValue::Error,
                Some(schema::enums::ErrorType::TelemetryInstrumentationFault),
            );
        }
    }
}

const fn valid_completion(
    outcome: schema::enums::OutcomeValue,
    error_type: Option<schema::enums::ErrorType>,
) -> bool {
    use schema::enums::{ErrorType, OutcomeValue};
    match (outcome, error_type) {
        (OutcomeValue::Failure | OutcomeValue::Error | OutcomeValue::Timeout, Some(error)) => {
            !matches!(error, ErrorType::RecoveredDegradation)
        }
        (OutcomeValue::Success, None | Some(ErrorType::RecoveredDegradation))
        | (OutcomeValue::Skip | OutcomeValue::Cancellation, None) => true,
        _ => false,
    }
}

fn make_span(name: &str, root: bool) -> Option<Span> {
    if root {
        return make_root_span(name);
    }
    make_child_span(name)
}

fn make_root_span(name: &str) -> Option<Span> {
    Some(match name {
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
        schema::spans::PREWARM_SCHEDULE => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "prewarm.schedule", otel.kind = "producer")
        }
        schema::spans::PREWARM_ATTEMPT => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "prewarm.attempt", otel.kind = "consumer")
        }
        schema::spans::CONNECTION_ATTEMPT => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "connection.attempt")
        }
        _ => return make_root_execution_span(name),
    })
}

fn make_root_execution_span(name: &str) -> Option<Span> {
    Some(match name {
        schema::spans::PROCESS_COMMAND => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "process.command", otel.kind = "client")
        }
        schema::spans::LAUNCH => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "launch")
        }
        schema::spans::LAUNCH_STAGE => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "launch.stage")
        }
        schema::spans::HTTP_CLIENT => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "http.client", otel.kind = "client")
        }
        schema::spans::DB_CLIENT => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "db.client", otel.kind = "client")
        }
        schema::spans::RPC_CLIENT => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "rpc.client", otel.kind = "client")
        }
        schema::spans::RPC_SERVER => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "rpc.server", otel.kind = "server")
        }
        schema::spans::TELEMETRY_VALIDATE => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, parent: None, "telemetry.validate")
        }
        _ => return None,
    })
}

fn make_child_span(name: &str) -> Option<Span> {
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
        schema::spans::PREWARM_SCHEDULE => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "prewarm.schedule", otel.kind = "producer")
        }
        schema::spans::PREWARM_ATTEMPT => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "prewarm.attempt", otel.kind = "consumer")
        }
        schema::spans::CONNECTION_ATTEMPT => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "connection.attempt", otel.kind = "client")
        }
        _ => return make_child_execution_span(name),
    })
}

fn make_child_execution_span(name: &str) -> Option<Span> {
    Some(match name {
        schema::spans::PROCESS_COMMAND => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "process.command", otel.kind = "client")
        }
        schema::spans::LAUNCH => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "launch")
        }
        schema::spans::LAUNCH_STAGE => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "launch.stage")
        }
        schema::spans::HTTP_CLIENT => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "http.client", otel.kind = "client")
        }
        schema::spans::DB_CLIENT => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "db.client", otel.kind = "client")
        }
        schema::spans::RPC_CLIENT => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "rpc.client", otel.kind = "client")
        }
        schema::spans::RPC_SERVER => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "rpc.server", otel.kind = "server")
        }
        schema::spans::TELEMETRY_VALIDATE => {
            tracing::info_span!(target: crate::TELEMETRY_TARGET, "telemetry.validate")
        }
        _ => return None,
    })
}

pub fn operation(def: &'static SpanDef, attrs: &[Attr<'_>]) -> Result<OperationGuard, Rejection> {
    operation_inner(def, attrs, false)
}

/// Start a governed operation, degrading to a disabled guard on rejection.
///
/// Product work must never fail because instrumentation was rejected. Tests
/// and schema validation can use [`operation`] when they need the rejection.
#[must_use]
pub fn operation_or_disabled(def: &'static SpanDef, attrs: &[Attr<'_>]) -> OperationGuard {
    operation(def, attrs).unwrap_or_else(|_| OperationGuard::disabled())
}

/// Start an operation parented by an extracted remote W3C span context.
pub fn operation_with_remote_parent(
    def: &'static SpanDef,
    attrs: &[Attr<'_>],
    parent: &SpanContext,
) -> Result<OperationGuard, Rejection> {
    use opentelemetry::trace::TraceContextExt as _;

    let guard = operation_inner(def, attrs, true)?;
    drop(
        guard
            .span
            .set_parent(opentelemetry::Context::new().with_remote_span_context(parent.clone())),
    );
    Ok(guard)
}

pub fn root_operation(
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
    let canonical = schema::spans::definition(def.name)
        .filter(|metadata| {
            metadata.name == def.metadata.name
                && metadata.kind == def.metadata.kind
                && metadata.description == def.metadata.description
        })
        .ok_or(Rejection::UnknownName);
    let metadata = match canonical {
        Ok(metadata) => metadata,
        Err(reason) => {
            health::reject(health::Signal::Trace, reason);
            return Err(reason);
        }
    };
    let invocation = crate::identity::current_invocation().map(|id| id.to_string());
    let session = crate::identity::current_session().map(|value| value.current.to_string());
    let mut ambient_attrs = attrs.to_vec();
    let supplied = |key| attrs.iter().any(|attr| attr.key == key);
    if let Some(invocation) = invocation.as_deref()
        && metadata
            .attributes
            .iter()
            .any(|requirement| requirement.name == schema::attrs::CLI_INVOCATION_ID)
        && !supplied(schema::attrs::CLI_INVOCATION_ID)
    {
        ambient_attrs.push(Attr {
            key: schema::attrs::CLI_INVOCATION_ID,
            value: Value::Str(invocation),
        });
    }
    if let Some(session) = session.as_deref()
        && metadata
            .attributes
            .iter()
            .any(|requirement| requirement.name == schema::attrs::std_attrs::SESSION_ID)
        && !supplied(schema::attrs::std_attrs::SESSION_ID)
    {
        ambient_attrs.push(Attr {
            key: schema::attrs::std_attrs::SESSION_ID,
            value: Value::Str(session),
        });
    }
    let attrs = ambient_attrs.as_slice();
    if attrs.iter().any(|attr| attr.key == schema::attrs::OUTCOME) {
        health::reject(health::Signal::Trace, Rejection::InvalidValue);
        return Err(Rejection::InvalidValue);
    }
    if let Err(reason) =
        validation::attributes(metadata.attributes, attrs, limits::MAX_SPAN_ATTRIBUTES - 2)
    {
        health::reject(health::Signal::Trace, reason);
        return Err(reason);
    }
    let Some(span) = make_span(def.name, root) else {
        health::reject(health::Signal::Trace, Rejection::UnknownName);
        return Err(Rejection::UnknownName);
    };
    let mut guard = OperationGuard {
        definition: def,
        span,
        completed: AtomicBool::new(false),
        links: AtomicUsize::new(0),
        attributes: AtomicUsize::new(0),
        attribute_keys: Mutex::new(Vec::with_capacity(attrs.len())),
        rpc: None,
        connection: None,
    };
    for attr in attrs {
        guard.set_attr(*attr)?;
    }
    let rpc = matches!(
        def.name,
        schema::spans::RPC_CLIENT | schema::spans::RPC_SERVER
    )
    .then(|| {
        attrs.iter().find_map(|attr| {
            (attr.key == schema::attrs::std_attrs::RPC_METHOD)
                .then_some(attr.value)
                .and_then(|value| match value {
                    Value::Str(method) => Some(RpcLifecycle::start(method)),
                    _ => None,
                })
        })
    })
    .flatten();
    guard.rpc = rpc;
    guard.connection = (def.name == schema::spans::CONNECTION_ATTEMPT)
        .then(|| {
            attrs.iter().find_map(|attr| {
                (attr.key == schema::attrs::CONNECTION_PEER_TYPE)
                    .then_some(attr.value)
                    .and_then(|value| match value {
                        Value::Str(peer) => Some(ConnectionLifecycle::start(peer)),
                        _ => None,
                    })
            })
        })
        .flatten();
    Ok(guard)
}

#[cfg(test)]
mod tests;

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;

use crate::{TELEMETRY_TARGET, health, limits, schema, validation};

#[doc(hidden)]
pub type PendingEventArrays = Vec<(&'static str, Vec<String>)>;

thread_local! {
    static PENDING_EVENT_ARRAYS: RefCell<Option<PendingEventArrays>> =
        const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Rejection {
    UnknownName,
    UnknownAttribute,
    InvalidValue,
    Privacy,
    Cardinality,
    SizeLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Copy, Debug)]
pub struct EventDef {
    pub(crate) name: &'static str,
    pub(crate) severity: Severity,
    pub(crate) metadata: &'static schema::EventMetadata,
}

impl EventDef {
    const fn generated(metadata: &'static schema::EventMetadata) -> Self {
        let severity = match metadata.severity {
            schema::EventSeverity::Trace => Severity::Trace,
            schema::EventSeverity::Debug => Severity::Debug,
            schema::EventSeverity::Info => Severity::Info,
            schema::EventSeverity::Warn => Severity::Warn,
            schema::EventSeverity::Error => Severity::Error,
        };
        Self {
            name: metadata.name,
            severity,
            metadata,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Value<'a> {
    Str(&'a str),
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    StrArray(&'a [&'a str]),
}

#[derive(Clone, Copy, Debug)]
pub struct Attr<'a> {
    pub key: &'static str,
    pub value: Value<'a>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FieldSet<'a> {
    pub attrs: &'a [Attr<'a>],
    pub body: Option<&'a str>,
}

impl<'a> FieldSet<'a> {
    #[must_use]
    pub const fn new(attrs: &'a [Attr<'a>], body: Option<&'a str>) -> Self {
        Self { attrs, body }
    }

    fn str(&self, key: &str) -> Option<&'a str> {
        self.attrs.iter().find_map(|attr| match attr {
            Attr {
                key: candidate,
                value: Value::Str(value),
            } if *candidate == key => Some(*value),
            _ => None,
        })
    }

    fn integer(&self, key: &str) -> Option<i64> {
        self.attrs.iter().find_map(|attr| match attr {
            Attr {
                key: candidate,
                value: Value::I64(value),
            } if *candidate == key => Some(*value),
            Attr {
                key: candidate,
                value: Value::U64(value),
            } if *candidate == key => i64::try_from(*value).ok(),
            _ => None,
        })
    }

    fn double(&self, key: &str) -> Option<f64> {
        self.attrs.iter().find_map(|attr| match attr {
            Attr {
                key: candidate,
                value: Value::F64(value),
            } if *candidate == key => Some(*value),
            _ => None,
        })
    }

    fn boolean(&self, key: &str) -> Option<bool> {
        self.attrs.iter().find_map(|attr| match attr {
            Attr {
                key: candidate,
                value: Value::Bool(value),
            } if *candidate == key => Some(*value),
            _ => None,
        })
    }
}

fn with_event_arrays(fields: &FieldSet<'_>, emit: impl FnOnce()) {
    let arrays = fields
        .attrs
        .iter()
        .filter_map(|attr| match attr.value {
            Value::StrArray(values) => Some((
                attr.key,
                values.iter().map(|value| (*value).to_owned()).collect(),
            )),
            _ => None,
        })
        .collect();
    PENDING_EVENT_ARRAYS.with(|slot| {
        let previous = slot.replace(Some(arrays));
        emit();
        slot.replace(previous);
    });
}

/// Supplies registered array attributes to the synchronous tracing log bridge.
///
/// The tracing value model has no array variant, so the standard appender would
/// otherwise flatten these values through `Debug`. The bridge consumes this
/// handoff while it is processing the same event on the emitting thread.
#[doc(hidden)]
pub fn take_pending_event_arrays() -> PendingEventArrays {
    PENDING_EVENT_ARRAYS.with(|slot| slot.borrow_mut().take().unwrap_or_default())
}

include!("event_defs.rs");

#[must_use]
pub fn definition(name: &str) -> Option<&'static EventDef> {
    ALL.iter().find(|definition| definition.name == name)
}

#[must_use]
pub fn canonical_severity(name: &str) -> Option<Severity> {
    definition(name).map(|definition| definition.severity)
}

fn validate(def: &'static EventDef, fields: &FieldSet<'_>) -> Result<(), Rejection> {
    let Some(canonical) = definition(def.name) else {
        return Err(Rejection::UnknownName);
    };
    if canonical.severity != def.severity || canonical.metadata.name != def.metadata.name {
        return Err(Rejection::UnknownName);
    }
    limits::validate_name(def.name)?;
    validation::attributes(
        def.metadata.attributes,
        fields.attrs,
        limits::MAX_LOG_ATTRIBUTES,
    )?;
    validate_config_schema_versions(fields)?;
    Ok(())
}

fn validate_config_schema_versions(fields: &FieldSet<'_>) -> Result<(), Rejection> {
    let from = fields.str(schema::attrs::CONFIG_SCHEMA_VERSION_FROM);
    let to = fields.str(schema::attrs::CONFIG_SCHEMA_VERSION_TO);
    if from.is_none() && to.is_none() {
        return Ok(());
    }
    let scope = fields
        .str(schema::attrs::CONFIG_SCOPE)
        .ok_or(Rejection::InvalidValue)?;
    if from.is_some_and(|value| {
        !schema::valid_config_schema_version(scope, schema::ConfigVersionDirection::From, value)
    }) || to.is_some_and(|value| {
        !schema::valid_config_schema_version(scope, schema::ConfigVersionDirection::To, value)
    }) {
        return Err(Rejection::InvalidValue);
    }
    Ok(())
}

fn emit_at_severity(
    severity: Severity,
    trace: impl FnOnce(),
    debug: impl FnOnce(),
    info: impl FnOnce(),
    warn: impl FnOnce(),
    error: impl FnOnce(),
) {
    match severity {
        Severity::Trace => trace(),
        Severity::Debug => debug(),
        Severity::Info => info(),
        Severity::Warn => warn(),
        Severity::Error => error(),
    }
}

macro_rules! emit_schema_event {
    ($name:literal, $severity:expr, $fields:expr, [$(($key:literal, $field:ident, $kind:ident)),* $(,)?]) => {{
        $(let $field = event_field_value!($fields, $key, $kind);)*
        match $fields.body {
            Some(body) => emit_at_severity(
                $severity,
                || tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::TRACE, $($key = $field,)* message = body),
                || tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::DEBUG, $($key = $field,)* message = body),
                || tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::INFO, $($key = $field,)* message = body),
                || tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::WARN, $($key = $field,)* message = body),
                || tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::ERROR, $($key = $field,)* message = body),
            ),
            None => emit_at_severity(
                $severity,
                || tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::TRACE, { $($key = $field,)* }),
                || tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::DEBUG, { $($key = $field,)* }),
                || tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::INFO, { $($key = $field,)* }),
                || tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::WARN, { $($key = $field,)* }),
                || tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::ERROR, { $($key = $field,)* }),
            ),
        }
    }};
}

include!("event_emit.rs");

fn event_enabled(def: &EventDef) -> bool {
    match def.severity {
        Severity::Trace => tracing::enabled!(target: TELEMETRY_TARGET, tracing::Level::TRACE),
        Severity::Debug => tracing::enabled!(target: TELEMETRY_TARGET, tracing::Level::DEBUG),
        Severity::Info => tracing::enabled!(target: TELEMETRY_TARGET, tracing::Level::INFO),
        Severity::Warn => tracing::enabled!(target: TELEMETRY_TARGET, tracing::Level::WARN),
        Severity::Error => tracing::enabled!(target: TELEMETRY_TARGET, tracing::Level::ERROR),
    }
}

/// Emit a governed event whose body is formatted only after its level is enabled.
pub fn emit_event_display(
    def: &'static EventDef,
    attrs: &[Attr<'_>],
    body: &impl std::fmt::Display,
) -> Result<(), Rejection> {
    if !event_enabled(def) {
        return Ok(());
    }
    let body = body.to_string();
    emit_event(def, FieldSet::new(attrs, Some(&body)))
}

pub fn emit_event(def: &'static EventDef, fields: FieldSet<'_>) -> Result<(), Rejection> {
    if !event_enabled(def) {
        return Ok(());
    }
    let invocation = crate::identity::current_invocation().map(|id| id.to_string());
    let session = crate::identity::current_session().map(|value| value.current.to_string());
    let mut ambient_fields = fields.attrs.to_vec();
    let accepts = |key| {
        def.metadata
            .attributes
            .iter()
            .any(|requirement| requirement.name == key)
    };
    let supplied = |key| fields.attrs.iter().any(|attr| attr.key == key);
    if let Some(invocation) = invocation.as_deref()
        && accepts(schema::attrs::CLI_INVOCATION_ID)
        && !supplied(schema::attrs::CLI_INVOCATION_ID)
    {
        ambient_fields.push(Attr {
            key: schema::attrs::CLI_INVOCATION_ID,
            value: Value::Str(invocation),
        });
    }
    if let Some(session) = session.as_deref()
        && accepts(schema::attrs::std_attrs::SESSION_ID)
        && !supplied(schema::attrs::std_attrs::SESSION_ID)
    {
        ambient_fields.push(Attr {
            key: schema::attrs::std_attrs::SESSION_ID,
            value: Value::Str(session),
        });
    }
    let fields = FieldSet::new(&ambient_fields, fields.body);
    if let Err(reason) = validate(def, &fields) {
        health::reject(health::Signal::Log, reason);
        return Err(reason);
    }
    let body = fields.body.map(limits::redact_and_clamp);
    let exception_values: Vec<_> = fields
        .attrs
        .iter()
        .map(|attr| match (attr.key, attr.value) {
            ("exception.message" | "exception.stacktrace", Value::Str(value)) => {
                Some(limits::redact_and_clamp(value))
            }
            _ => None,
        })
        .collect();
    let sanitized_attrs: Vec<_> = fields
        .attrs
        .iter()
        .zip(&exception_values)
        .map(|(attr, exception)| Attr {
            key: attr.key,
            value: exception
                .as_ref()
                .map_or(attr.value, |value| Value::Str(value.as_ref())),
        })
        .collect();
    let sanitized = FieldSet::new(&sanitized_attrs, body.as_deref());
    with_event_arrays(&sanitized, || emit_registered_event(def, sanitized));
    Ok(())
}

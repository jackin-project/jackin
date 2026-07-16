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

pub const SESSION_START: EventDef = EventDef {
    name: schema::events::SESSION_START,
    severity: Severity::Info,
    metadata: &schema::events::SESSION_START_DEF,
};
pub const SESSION_END: EventDef = EventDef {
    name: schema::events::SESSION_END,
    severity: Severity::Info,
    metadata: &schema::events::SESSION_END_DEF,
};
pub const UI_SCREEN_ENTERED: EventDef = EventDef {
    name: schema::events::UI_SCREEN_ENTERED,
    severity: Severity::Info,
    metadata: &schema::events::UI_SCREEN_ENTERED_DEF,
};
pub const UI_SCREEN_EXITED: EventDef = EventDef {
    name: schema::events::UI_SCREEN_EXITED,
    severity: Severity::Info,
    metadata: &schema::events::UI_SCREEN_EXITED_DEF,
};
pub const UI_WIDGET_FOCUSED: EventDef = EventDef {
    name: schema::events::UI_WIDGET_FOCUSED,
    severity: Severity::Debug,
    metadata: &schema::events::UI_WIDGET_FOCUSED_DEF,
};
pub const UI_WIDGET_UNFOCUSED: EventDef = EventDef {
    name: schema::events::UI_WIDGET_UNFOCUSED,
    severity: Severity::Debug,
    metadata: &schema::events::UI_WIDGET_UNFOCUSED_DEF,
};
pub const APP_JANK: EventDef = EventDef {
    name: schema::events::APP_JANK,
    severity: Severity::Warn,
    metadata: &schema::events::APP_JANK_DEF,
};
pub const APP_CRASH: EventDef = EventDef {
    name: schema::events::APP_CRASH,
    severity: Severity::Error,
    metadata: &schema::events::APP_CRASH_DEF,
};
pub const AGENT_STATE_CHANGED: EventDef = EventDef {
    name: schema::events::AGENT_STATE_CHANGED,
    severity: Severity::Info,
    metadata: &schema::events::AGENT_STATE_CHANGED_DEF,
};
pub const PTY_SPAWN: EventDef = EventDef {
    name: schema::events::PTY_SPAWN,
    severity: Severity::Info,
    metadata: &schema::events::PTY_SPAWN_DEF,
};
pub const PTY_EXIT: EventDef = EventDef {
    name: schema::events::PTY_EXIT,
    severity: Severity::Info,
    metadata: &schema::events::PTY_EXIT_DEF,
};
pub const TELEMETRY_VALIDATE: EventDef = EventDef {
    name: schema::events::TELEMETRY_VALIDATE,
    severity: Severity::Info,
    metadata: &schema::events::TELEMETRY_VALIDATE_DEF,
};

macro_rules! event_def {
    ($constant:ident, $name:ident, $definition:ident, $severity:ident) => {
        pub const $constant: EventDef = EventDef {
            name: schema::events::$name,
            severity: Severity::$severity,
            metadata: &schema::events::$definition,
        };
    };
}

event_def!(
    LAUNCH_STAGE_STARTED,
    LAUNCH_STAGE_STARTED,
    LAUNCH_STAGE_STARTED_DEF,
    Info
);
event_def!(
    LAUNCH_STAGE_DONE,
    LAUNCH_STAGE_DONE,
    LAUNCH_STAGE_DONE_DEF,
    Info
);
event_def!(
    LAUNCH_STAGE_FAILED,
    LAUNCH_STAGE_FAILED,
    LAUNCH_STAGE_FAILED_DEF,
    Error
);
event_def!(
    LAUNCH_STAGE_SKIPPED,
    LAUNCH_STAGE_SKIPPED,
    LAUNCH_STAGE_SKIPPED_DEF,
    Info
);
event_def!(TIMING_STARTED, TIMING_STARTED, TIMING_STARTED_DEF, Trace);
event_def!(TIMING_DONE, TIMING_DONE, TIMING_DONE_DEF, Info);
event_def!(DEBUG_LINE, DEBUG_LINE, DEBUG_LINE_DEF, Debug);
event_def!(
    PROCESS_SUBPROCESS_DONE,
    PROCESS_SUBPROCESS_DONE,
    PROCESS_SUBPROCESS_DONE_DEF,
    Info
);
event_def!(RUN_SUMMARY, RUN_SUMMARY, RUN_SUMMARY_DEF, Info);
event_def!(
    PERFORMANCE_SLOW_FOREGROUND_WAIT,
    PERFORMANCE_SLOW_FOREGROUND_WAIT,
    PERFORMANCE_SLOW_FOREGROUND_WAIT_DEF,
    Warn
);
event_def!(
    CAPSULE_SESSION_DETACH,
    CAPSULE_SESSION_DETACH,
    CAPSULE_SESSION_DETACH_DEF,
    Info
);
event_def!(
    CAPSULE_SESSION_CLEAN_SHUTDOWN,
    CAPSULE_SESSION_CLEAN_SHUTDOWN,
    CAPSULE_SESSION_CLEAN_SHUTDOWN_DEF,
    Info
);
event_def!(ERROR_TYPED, ERROR_TYPED, ERROR_TYPED_DEF, Error);
event_def!(
    CONFIG_OPERATION,
    CONFIG_OPERATION,
    CONFIG_OPERATION_DEF,
    Info
);
event_def!(TRUST_DECISION, TRUST_DECISION, TRUST_DECISION_DEF, Info);
event_def!(
    ISOLATION_DECISION,
    ISOLATION_DECISION,
    ISOLATION_DECISION_DEF,
    Info
);
event_def!(
    ISOLATION_FIREWALL_FAILED,
    ISOLATION_FIREWALL_FAILED,
    ISOLATION_FIREWALL_FAILED_DEF,
    Error
);
event_def!(CACHE_DECISION, CACHE_DECISION, CACHE_DECISION_DEF, Info);
event_def!(OPERATION_LOG, OPERATION_LOG, OPERATION_LOG_DEF, Info);
event_def!(OPERATION_WARN, OPERATION_WARN, OPERATION_WARN_DEF, Warn);

pub const ALL: &[EventDef] = &[
    SESSION_START,
    SESSION_END,
    UI_SCREEN_ENTERED,
    UI_SCREEN_EXITED,
    UI_WIDGET_FOCUSED,
    UI_WIDGET_UNFOCUSED,
    APP_JANK,
    APP_CRASH,
    AGENT_STATE_CHANGED,
    PTY_SPAWN,
    PTY_EXIT,
    TELEMETRY_VALIDATE,
    LAUNCH_STAGE_STARTED,
    LAUNCH_STAGE_DONE,
    LAUNCH_STAGE_FAILED,
    LAUNCH_STAGE_SKIPPED,
    TIMING_STARTED,
    TIMING_DONE,
    DEBUG_LINE,
    PROCESS_SUBPROCESS_DONE,
    RUN_SUMMARY,
    PERFORMANCE_SLOW_FOREGROUND_WAIT,
    CAPSULE_SESSION_DETACH,
    CAPSULE_SESSION_CLEAN_SHUTDOWN,
    ERROR_TYPED,
    CONFIG_OPERATION,
    TRUST_DECISION,
    ISOLATION_DECISION,
    ISOLATION_FIREWALL_FAILED,
    CACHE_DECISION,
    OPERATION_LOG,
    OPERATION_WARN,
];

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

pub fn emit_event(def: &'static EventDef, fields: FieldSet<'_>) -> Result<(), Rejection> {
    let enabled = match def.severity {
        Severity::Trace => tracing::enabled!(target: TELEMETRY_TARGET, tracing::Level::TRACE),
        Severity::Debug => tracing::enabled!(target: TELEMETRY_TARGET, tracing::Level::DEBUG),
        Severity::Info => tracing::enabled!(target: TELEMETRY_TARGET, tracing::Level::INFO),
        Severity::Warn => tracing::enabled!(target: TELEMETRY_TARGET, tracing::Level::WARN),
        Severity::Error => tracing::enabled!(target: TELEMETRY_TARGET, tracing::Level::ERROR),
    };
    if !enabled {
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

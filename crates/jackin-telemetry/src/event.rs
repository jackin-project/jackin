// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{TELEMETRY_TARGET, health, limits, privacy, schema};

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
    pub name: &'static str,
    pub severity: Severity,
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
}

pub const SESSION_START: EventDef = EventDef {
    name: schema::events::SESSION_START,
    severity: Severity::Info,
};
pub const SESSION_END: EventDef = EventDef {
    name: schema::events::SESSION_END,
    severity: Severity::Info,
};
pub const UI_SCREEN_ENTERED: EventDef = EventDef {
    name: schema::events::UI_SCREEN_ENTERED,
    severity: Severity::Info,
};
pub const UI_SCREEN_EXITED: EventDef = EventDef {
    name: schema::events::UI_SCREEN_EXITED,
    severity: Severity::Info,
};
pub const UI_WIDGET_FOCUSED: EventDef = EventDef {
    name: schema::events::UI_WIDGET_FOCUSED,
    severity: Severity::Debug,
};
pub const UI_WIDGET_UNFOCUSED: EventDef = EventDef {
    name: schema::events::UI_WIDGET_UNFOCUSED,
    severity: Severity::Debug,
};
pub const APP_JANK: EventDef = EventDef {
    name: schema::events::APP_JANK,
    severity: Severity::Warn,
};
pub const APP_CRASH: EventDef = EventDef {
    name: schema::events::APP_CRASH,
    severity: Severity::Error,
};

fn validate(def: &'static EventDef, fields: &FieldSet<'_>) -> Result<(), Rejection> {
    if !schema::events::ALL.contains(&def.name) {
        return Err(Rejection::UnknownName);
    }
    limits::validate_name(def.name)?;
    if fields.attrs.len() > limits::MAX_LOG_ATTRIBUTES {
        return Err(Rejection::SizeLimit);
    }
    for attr in fields.attrs {
        privacy::validate_key(attr.key)?;
        limits::validate_value(&attr.value)?;
    }
    Ok(())
}

macro_rules! emit_named {
    ($name:literal, $level:expr, $fields:expr) => {{
        let outcome = $fields.str(schema::attrs::OUTCOME);
        let session_id = $fields.str(schema::attrs::std_attrs::SESSION_ID);
        let screen_id = $fields.str(schema::attrs::std_attrs::APP_SCREEN_ID);
        let screen_name = $fields.str(schema::attrs::std_attrs::APP_SCREEN_NAME);
        let action_name = $fields.str(schema::attrs::UI_ACTION_NAME);
        let error_type = $fields.str(schema::attrs::std_attrs::ERROR_TYPE);
        let body = $fields.body.unwrap_or("");
        match $level {
            Severity::Trace => tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::TRACE, outcome, "session.id" = session_id, "app.screen.id" = screen_id, "app.screen.name" = screen_name, "ui.action.name" = action_name, "error.type" = error_type, message = body),
            Severity::Debug => tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::DEBUG, outcome, "session.id" = session_id, "app.screen.id" = screen_id, "app.screen.name" = screen_name, "ui.action.name" = action_name, "error.type" = error_type, message = body),
            Severity::Info => tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::INFO, outcome, "session.id" = session_id, "app.screen.id" = screen_id, "app.screen.name" = screen_name, "ui.action.name" = action_name, "error.type" = error_type, message = body),
            Severity::Warn => tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::WARN, outcome, "session.id" = session_id, "app.screen.id" = screen_id, "app.screen.name" = screen_name, "ui.action.name" = action_name, "error.type" = error_type, message = body),
            Severity::Error => tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::ERROR, outcome, "session.id" = session_id, "app.screen.id" = screen_id, "app.screen.name" = screen_name, "ui.action.name" = action_name, "error.type" = error_type, message = body),
        }
    }};
}

pub fn emit_event(def: &'static EventDef, fields: FieldSet<'_>) -> Result<(), Rejection> {
    if let Err(reason) = validate(def, &fields) {
        health::reject(reason);
        return Err(reason);
    }
    match def.name {
        schema::events::SESSION_START => emit_named!("session.start", def.severity, fields),
        schema::events::SESSION_END => emit_named!("session.end", def.severity, fields),
        schema::events::UI_SCREEN_ENTERED => emit_named!("ui.screen.entered", def.severity, fields),
        schema::events::UI_SCREEN_EXITED => emit_named!("ui.screen.exited", def.severity, fields),
        schema::events::UI_WIDGET_FOCUSED => emit_named!("ui.widget.focused", def.severity, fields),
        schema::events::UI_WIDGET_UNFOCUSED => {
            emit_named!("ui.widget.unfocused", def.severity, fields)
        }
        schema::events::APP_JANK => emit_named!("app.jank", def.severity, fields),
        schema::events::APP_CRASH => emit_named!("app.crash", def.severity, fields),
        _ => unreachable!("validated closed event registry"),
    }
    Ok(())
}

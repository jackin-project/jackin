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
pub const AGENT_STATE_CHANGED: EventDef = EventDef {
    name: schema::events::AGENT_STATE_CHANGED,
    severity: Severity::Info,
};
pub const PTY_SPAWN: EventDef = EventDef {
    name: schema::events::PTY_SPAWN,
    severity: Severity::Info,
};
pub const PTY_EXIT: EventDef = EventDef {
    name: schema::events::PTY_EXIT,
    severity: Severity::Info,
};
pub const TELEMETRY_VALIDATE: EventDef = EventDef {
    name: schema::events::TELEMETRY_VALIDATE,
    severity: Severity::Info,
};

macro_rules! event_def {
    ($constant:ident, $name:ident, $severity:ident) => {
        pub const $constant: EventDef = EventDef {
            name: schema::events::$name,
            severity: Severity::$severity,
        };
    };
}

event_def!(LAUNCH_STAGE_STARTED, LAUNCH_STAGE_STARTED, Info);
event_def!(LAUNCH_STAGE_DONE, LAUNCH_STAGE_DONE, Info);
event_def!(LAUNCH_STAGE_FAILED, LAUNCH_STAGE_FAILED, Error);
event_def!(LAUNCH_STAGE_SKIPPED, LAUNCH_STAGE_SKIPPED, Info);
event_def!(TIMING_STARTED, TIMING_STARTED, Info);
event_def!(TIMING_DONE, TIMING_DONE, Info);
event_def!(DEBUG_LINE, DEBUG_LINE, Debug);
event_def!(PROCESS_SUBPROCESS_DONE, PROCESS_SUBPROCESS_DONE, Info);
event_def!(RUN_SUMMARY, RUN_SUMMARY, Info);
event_def!(
    PERFORMANCE_SLOW_FOREGROUND_WAIT,
    PERFORMANCE_SLOW_FOREGROUND_WAIT,
    Warn
);
event_def!(CAPSULE_SESSION_DETACH, CAPSULE_SESSION_DETACH, Info);
event_def!(
    CAPSULE_SESSION_CLEAN_SHUTDOWN,
    CAPSULE_SESSION_CLEAN_SHUTDOWN,
    Info
);
event_def!(ERROR_TYPED, ERROR_TYPED, Error);
event_def!(OPERATION_LOG, OPERATION_LOG, Info);
event_def!(OPERATION_WARN, OPERATION_WARN, Warn);

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
        let visit_id = $fields.str(schema::attrs::UI_SCREEN_VISIT_ID);
        let transition_reason = $fields.str(schema::attrs::UI_TRANSITION_REASON);
        let widget_id = $fields.str(schema::attrs::std_attrs::APP_WIDGET_ID);
        let agent_name = $fields.str(schema::attrs::std_attrs::GEN_AI_AGENT_NAME);
        let conversation_id = $fields.str(schema::attrs::std_attrs::GEN_AI_CONVERSATION_ID);
        let pty_exit_reason = $fields.str(schema::attrs::PTY_EXIT_REASON);
        let process_exit_code = $fields.attrs.iter().find_map(|attr| match attr {
            Attr { key: schema::attrs::std_attrs::PROCESS_EXIT_CODE, value: Value::I64(value) } => Some(*value),
            _ => None,
        });
        let jank_frame_count = $fields.attrs.iter().find_map(|attr| match attr {
            Attr { key: schema::attrs::APP_JANK_FRAME_COUNT, value: Value::U64(value) } => Some(*value),
            _ => None,
        });
        let jank_period = $fields.str(schema::attrs::APP_JANK_PERIOD);
        let jank_threshold = $fields.attrs.iter().find_map(|attr| match attr {
            Attr { key: schema::attrs::APP_JANK_THRESHOLD, value: Value::F64(value) } => Some(*value),
            _ => None,
        });
        let agent_state = $fields.str(schema::attrs::AGENT_STATE);
        let agent_status_source = $fields.str(schema::attrs::AGENT_STATUS_SOURCE);
        let agent_status_confidence = $fields.str(schema::attrs::AGENT_STATUS_CONFIDENCE);
        let agent_status_stuck = $fields.attrs.iter().find_map(|attr| match attr {
            Attr { key: schema::attrs::AGENT_STATUS_STUCK, value: Value::Bool(value) } => Some(*value),
            _ => None,
        });
        let navigation_sequence = $fields.attrs.iter().find_map(|attr| match attr {
            Attr { key: schema::attrs::UI_NAVIGATION_SEQUENCE, value: Value::U64(value) } => Some(*value),
            _ => None,
        });
        let error_type = $fields.str(schema::attrs::std_attrs::ERROR_TYPE);
        let body = $fields.body.unwrap_or("");
        match $level {
            Severity::Trace => tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::TRACE, outcome, "session.id" = session_id, "app.screen.id" = screen_id, "app.screen.name" = screen_name, "ui.action.name" = action_name, "error.type" = error_type, message = body),
            Severity::Debug => tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::DEBUG, outcome, "session.id" = session_id, "app.screen.id" = screen_id, "app.screen.name" = screen_name, "app.widget.id" = widget_id, "ui.action.name" = action_name, "ui.screen.visit.id" = visit_id, "ui.navigation.sequence" = navigation_sequence, "ui.transition.reason" = transition_reason, "error.type" = error_type, message = body),
            Severity::Info => tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::INFO, outcome, "session.id" = session_id, "app.screen.id" = screen_id, "app.screen.name" = screen_name, "app.widget.id" = widget_id, "ui.action.name" = action_name, "ui.screen.visit.id" = visit_id, "ui.navigation.sequence" = navigation_sequence, "ui.transition.reason" = transition_reason, "gen_ai.agent.name" = agent_name, "gen_ai.conversation.id" = conversation_id, "agent.state" = agent_state, "agent.status.source" = agent_status_source, "agent.status.confidence" = agent_status_confidence, "agent.status.stuck" = agent_status_stuck, "pty.exit.reason" = pty_exit_reason, "process.exit.code" = process_exit_code, "error.type" = error_type, message = body),
            Severity::Warn => tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::WARN, outcome, "session.id" = session_id, "app.screen.id" = screen_id, "app.screen.name" = screen_name, "ui.action.name" = action_name, "app.jank.frame_count" = jank_frame_count, "app.jank.period" = jank_period, "app.jank.threshold" = jank_threshold, "error.type" = error_type, message = body),
            Severity::Error => tracing::event!(name: $name, target: TELEMETRY_TARGET, tracing::Level::ERROR, outcome, "session.id" = session_id, "app.screen.id" = screen_id, "app.screen.name" = screen_name, "ui.action.name" = action_name, "error.type" = error_type, message = body),
        }
    }};
}

#[expect(
    clippy::cognitive_complexity,
    reason = "the closed EventName dispatch is intentionally exhaustive in one authority"
)]
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
            emit_named!("ui.widget.unfocused", def.severity, fields);
        }
        schema::events::APP_JANK => emit_named!("app.jank", def.severity, fields),
        schema::events::APP_CRASH => emit_named!("app.crash", def.severity, fields),
        schema::events::AGENT_STATE_CHANGED => {
            emit_named!("agent.state.changed", def.severity, fields);
        }
        schema::events::PTY_SPAWN => emit_named!("pty.spawn", def.severity, fields),
        schema::events::PTY_EXIT => emit_named!("pty.exit", def.severity, fields),
        schema::events::TELEMETRY_VALIDATE => {
            emit_named!("telemetry.validate", def.severity, fields);
        }
        schema::events::LAUNCH_STAGE_STARTED => {
            emit_named!("launch.stage.started", def.severity, fields);
        }
        schema::events::LAUNCH_STAGE_DONE => emit_named!("launch.stage.done", def.severity, fields),
        schema::events::LAUNCH_STAGE_FAILED => {
            emit_named!("launch.stage.failed", def.severity, fields);
        }
        schema::events::LAUNCH_STAGE_SKIPPED => {
            emit_named!("launch.stage.skipped", def.severity, fields);
        }
        schema::events::TIMING_STARTED => emit_named!("timing.started", def.severity, fields),
        schema::events::TIMING_DONE => emit_named!("timing.done", def.severity, fields),
        schema::events::DEBUG_LINE => emit_named!("debug.line", def.severity, fields),
        schema::events::PROCESS_SUBPROCESS_DONE => {
            emit_named!("process.subprocess.done", def.severity, fields);
        }
        schema::events::RUN_SUMMARY => emit_named!("run.summary", def.severity, fields),
        schema::events::PERFORMANCE_SLOW_FOREGROUND_WAIT => {
            emit_named!("performance.slow.foreground.wait", def.severity, fields);
        }
        schema::events::CAPSULE_SESSION_DETACH => {
            emit_named!("capsule.session.detach", def.severity, fields);
        }
        schema::events::CAPSULE_SESSION_CLEAN_SHUTDOWN => {
            emit_named!("capsule.session.clean.shutdown", def.severity, fields);
        }
        schema::events::ERROR_TYPED => emit_named!("error.typed", def.severity, fields),
        schema::events::OPERATION_LOG => emit_named!("operation.log", def.severity, fields),
        schema::events::OPERATION_WARN => emit_named!("operation.warn", def.severity, fields),
        _ => unreachable!("validated closed event registry"),
    }
    Ok(())
}

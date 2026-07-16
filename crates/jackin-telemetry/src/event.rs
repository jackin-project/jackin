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
    if matches!(
        def.name,
        schema::events::SESSION_START
            | schema::events::SESSION_END
            | schema::events::UI_SCREEN_ENTERED
            | schema::events::UI_SCREEN_EXITED
            | schema::events::UI_WIDGET_FOCUSED
            | schema::events::UI_WIDGET_UNFOCUSED
            | schema::events::APP_JANK
            | schema::events::APP_CRASH
            | schema::events::AGENT_STATE_CHANGED
            | schema::events::PTY_SPAWN
    ) {
        emit_lifecycle_event(def, fields);
    } else if matches!(
        def.name,
        schema::events::PTY_EXIT
            | schema::events::TELEMETRY_VALIDATE
            | schema::events::LAUNCH_STAGE_STARTED
            | schema::events::LAUNCH_STAGE_DONE
            | schema::events::LAUNCH_STAGE_FAILED
            | schema::events::LAUNCH_STAGE_SKIPPED
            | schema::events::TIMING_STARTED
            | schema::events::TIMING_DONE
            | schema::events::DEBUG_LINE
            | schema::events::PROCESS_SUBPROCESS_DONE
    ) {
        emit_progress_event(def, fields);
    } else {
        emit_outcome_event(def, fields);
    }
    Ok(())
}

fn emit_lifecycle_event(def: &EventDef, fields: FieldSet<'_>) {
    match def.name {
        schema::events::SESSION_START => emit_session_start(def.severity, fields),
        schema::events::SESSION_END => emit_session_end(def.severity, fields),
        schema::events::UI_SCREEN_ENTERED => emit_ui_screen_entered(def.severity, fields),
        schema::events::UI_SCREEN_EXITED => emit_ui_screen_exited(def.severity, fields),
        schema::events::UI_WIDGET_FOCUSED => emit_ui_widget_focused(def.severity, fields),
        schema::events::UI_WIDGET_UNFOCUSED => emit_ui_widget_unfocused(def.severity, fields),
        schema::events::APP_JANK => emit_app_jank(def.severity, fields),
        schema::events::APP_CRASH => emit_app_crash(def.severity, fields),
        schema::events::AGENT_STATE_CHANGED => emit_agent_state_changed(def.severity, fields),
        schema::events::PTY_SPAWN => emit_pty_spawn(def.severity, fields),
        _ => unreachable!("validated lifecycle event registry"),
    }
}

fn emit_progress_event(def: &EventDef, fields: FieldSet<'_>) {
    match def.name {
        schema::events::PTY_EXIT => emit_pty_exit(def.severity, fields),
        schema::events::TELEMETRY_VALIDATE => emit_telemetry_validate(def.severity, fields),
        schema::events::LAUNCH_STAGE_STARTED => emit_launch_stage_started(def.severity, fields),
        schema::events::LAUNCH_STAGE_DONE => emit_launch_stage_done(def.severity, fields),
        schema::events::LAUNCH_STAGE_FAILED => emit_launch_stage_failed(def.severity, fields),
        schema::events::LAUNCH_STAGE_SKIPPED => emit_launch_stage_skipped(def.severity, fields),
        schema::events::TIMING_STARTED => emit_timing_started(def.severity, fields),
        schema::events::TIMING_DONE => emit_timing_done(def.severity, fields),
        schema::events::DEBUG_LINE => emit_debug_line(def.severity, fields),
        schema::events::PROCESS_SUBPROCESS_DONE => emit_process_done(def.severity, fields),
        _ => unreachable!("validated progress event registry"),
    }
}

fn emit_outcome_event(def: &EventDef, fields: FieldSet<'_>) {
    match def.name {
        schema::events::RUN_SUMMARY => emit_run_summary(def.severity, fields),
        schema::events::PERFORMANCE_SLOW_FOREGROUND_WAIT => emit_slow_wait(def.severity, fields),
        schema::events::CAPSULE_SESSION_DETACH => emit_session_detach(def.severity, fields),
        schema::events::CAPSULE_SESSION_CLEAN_SHUTDOWN => {
            emit_session_clean_shutdown(def.severity, fields);
        }
        schema::events::ERROR_TYPED => emit_error_typed(def.severity, fields),
        schema::events::OPERATION_LOG => emit_operation_log(def.severity, fields),
        schema::events::OPERATION_WARN => emit_operation_warn(def.severity, fields),
        _ => unreachable!("validated outcome event registry"),
    }
}

macro_rules! define_event_emitter {
    ($function:ident, $name:literal) => {
        fn $function(severity: Severity, fields: FieldSet<'_>) {
            emit_named!($name, severity, fields);
        }
    };
}

define_event_emitter!(emit_session_start, "session.start");
define_event_emitter!(emit_session_end, "session.end");
define_event_emitter!(emit_ui_screen_entered, "ui.screen.entered");
define_event_emitter!(emit_ui_screen_exited, "ui.screen.exited");
define_event_emitter!(emit_ui_widget_focused, "ui.widget.focused");
define_event_emitter!(emit_ui_widget_unfocused, "ui.widget.unfocused");
define_event_emitter!(emit_app_jank, "app.jank");
define_event_emitter!(emit_app_crash, "app.crash");
define_event_emitter!(emit_agent_state_changed, "agent.state.changed");
define_event_emitter!(emit_pty_spawn, "pty.spawn");
define_event_emitter!(emit_pty_exit, "pty.exit");
define_event_emitter!(emit_telemetry_validate, "telemetry.validate");
define_event_emitter!(emit_launch_stage_started, "launch.stage.started");
define_event_emitter!(emit_launch_stage_done, "launch.stage.done");
define_event_emitter!(emit_launch_stage_failed, "launch.stage.failed");
define_event_emitter!(emit_launch_stage_skipped, "launch.stage.skipped");
define_event_emitter!(emit_timing_started, "timing.started");
define_event_emitter!(emit_timing_done, "timing.done");
define_event_emitter!(emit_debug_line, "debug.line");
define_event_emitter!(emit_process_done, "process.subprocess.done");
define_event_emitter!(emit_run_summary, "run.summary");
define_event_emitter!(emit_slow_wait, "performance.slow.foreground.wait");
define_event_emitter!(emit_session_detach, "capsule.session.detach");
define_event_emitter!(
    emit_session_clean_shutdown,
    "capsule.session.clean.shutdown"
);
define_event_emitter!(emit_error_typed, "error.typed");
define_event_emitter!(emit_operation_log, "operation.log");
define_event_emitter!(emit_operation_warn, "operation.warn");

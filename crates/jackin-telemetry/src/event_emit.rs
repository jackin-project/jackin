// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

macro_rules! event_field_value {
    ($fields:expr, $key:literal, Boolean) => {
        $fields.boolean($key)
    };
    ($fields:expr, $key:literal, Double) => {
        $fields.double($key)
    };
    ($fields:expr, $key:literal, Integer) => {
        $fields.integer($key)
    };
    ($fields:expr, $key:literal, String) => {
        $fields.str($key)
    };
    ($fields:expr, $key:literal, StringArray) => {
        tracing::field::Empty
    };
}

fn emit_registered_event(def: &'static EventDef, fields: FieldSet<'_>) {
    if def.name <= "operation.log" {
        emit_registered_event_0(def, fields);
    } else if def.name <= "ui.widget.unfocused" {
        emit_registered_event_1(def, fields);
    } else {
        unreachable!("validated event registry");
    }
}

fn emit_registered_event_0(def: &'static EventDef, fields: FieldSet<'_>) {
    match def.name {
        schema::events::AGENT_STATE_CHANGED => {
            emit_agent_state_changed(def, fields);
        }
        schema::events::APP_CRASH => {
            emit_app_crash(def, fields);
        }
        schema::events::APP_JANK => {
            emit_app_jank(def, fields);
        }
        schema::events::CACHE_DECISION => {
            emit_cache_decision(def, fields);
        }
        schema::events::CAPSULE_SESSION_CLEAN_SHUTDOWN => {
            emit_capsule_session_clean_shutdown(def, fields);
        }
        schema::events::CAPSULE_SESSION_DETACH => {
            emit_capsule_session_detach(def, fields);
        }
        schema::events::CONFIG_OPERATION => {
            emit_config_operation(def, fields);
        }
        schema::events::DEBUG_LINE => {
            emit_debug_line(def, fields);
        }
        schema::events::ERROR_TYPED => {
            emit_error_typed(def, fields);
        }
        schema::events::ISOLATION_DECISION => {
            emit_isolation_decision(def, fields);
        }
        schema::events::ISOLATION_FIREWALL_FAILED => {
            emit_isolation_firewall_failed(def, fields);
        }
        schema::events::LAUNCH_STAGE_DONE => {
            emit_launch_stage_done(def, fields);
        }
        schema::events::LAUNCH_STAGE_FAILED => {
            emit_launch_stage_failed(def, fields);
        }
        schema::events::LAUNCH_STAGE_SKIPPED => {
            emit_launch_stage_skipped(def, fields);
        }
        schema::events::LAUNCH_STAGE_STARTED => {
            emit_launch_stage_started(def, fields);
        }
        schema::events::OPERATION_LOG => {
            emit_operation_log(def, fields);
        }
        _ => unreachable!("validated event registry chunk"),
    }
}

fn emit_registered_event_1(def: &'static EventDef, fields: FieldSet<'_>) {
    match def.name {
        schema::events::OPERATION_WARN => {
            emit_operation_warn(def, fields);
        }
        schema::events::PERFORMANCE_SLOW_FOREGROUND_WAIT => {
            emit_performance_slow_foreground_wait(def, fields);
        }
        schema::events::PROCESS_SUBPROCESS_DONE => {
            emit_process_subprocess_done(def, fields);
        }
        schema::events::PTY_EXIT => {
            emit_pty_exit(def, fields);
        }
        schema::events::PTY_SPAWN => {
            emit_pty_spawn(def, fields);
        }
        schema::events::RUN_SUMMARY => {
            emit_run_summary(def, fields);
        }
        schema::events::SESSION_END => {
            emit_session_end(def, fields);
        }
        schema::events::SESSION_START => {
            emit_session_start(def, fields);
        }
        schema::events::TELEMETRY_VALIDATE => {
            emit_telemetry_validate(def, fields);
        }
        schema::events::TIMING_DONE => {
            emit_timing_done(def, fields);
        }
        schema::events::TIMING_STARTED => {
            emit_timing_started(def, fields);
        }
        schema::events::TRUST_DECISION => {
            emit_trust_decision(def, fields);
        }
        schema::events::UI_SCREEN_ENTERED => {
            emit_ui_screen_entered(def, fields);
        }
        schema::events::UI_SCREEN_EXITED => {
            emit_ui_screen_exited(def, fields);
        }
        schema::events::UI_WIDGET_FOCUSED => {
            emit_ui_widget_focused(def, fields);
        }
        schema::events::UI_WIDGET_UNFOCUSED => {
            emit_ui_widget_unfocused(def, fields);
        }
        _ => unreachable!("validated event registry chunk"),
    }
}

fn emit_agent_state_changed(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "agent.state.changed",
        def.severity,
        fields,
        [
            ("agent.state", field_agent_state, String),
            (
                "agent.status.confidence",
                field_agent_status_confidence,
                String
            ),
            ("agent.status.source", field_agent_status_source, String),
            ("agent.status.stuck", field_agent_status_stuck, Boolean),
        ]
    );
}

fn emit_app_crash(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "app.crash",
        def.severity,
        fields,
        [
            ("app.build_id", field_app_build_id, String),
            ("app.crash.id", field_app_crash_id, String),
            ("exception.message", field_exception_message, String),
            ("exception.stacktrace", field_exception_stacktrace, String),
            ("exception.type", field_exception_type, String),
            ("os.name", field_os_name, String),
            ("os.version", field_os_version, String),
            ("service.version", field_service_version, String),
            ("session.id", field_session_id, String),
        ]
    );
}

fn emit_app_jank(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "app.jank",
        def.severity,
        fields,
        [
            ("app.jank.frame_count", field_app_jank_frame_count, Integer),
            ("app.jank.period", field_app_jank_period, Double),
            ("app.jank.threshold", field_app_jank_threshold, Double),
        ]
    );
}

fn emit_cache_decision(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "cache.decision",
        def.severity,
        fields,
        [
            ("cache.name", field_cache_name, String),
            ("cache.result", field_cache_result, String),
        ]
    );
}

fn emit_capsule_session_clean_shutdown(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "capsule.session.clean.shutdown",
        def.severity,
        fields,
        [
            ("outcome", field_outcome, String),
            ("session.id", field_session_id, String),
        ]
    );
}

fn emit_capsule_session_detach(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "capsule.session.detach",
        def.severity,
        fields,
        [
            ("outcome", field_outcome, String),
            ("session.id", field_session_id, String),
        ]
    );
}

fn emit_config_operation(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "config.operation",
        def.severity,
        fields,
        [
            (
                "config.migration.step_count",
                field_config_migration_step_count,
                Integer
            ),
            ("config.operation", field_config_operation, String),
            (
                "config.schema.version.from",
                field_config_schema_version_from,
                String
            ),
            (
                "config.schema.version.to",
                field_config_schema_version_to,
                String
            ),
            ("config.scope", field_config_scope, String),
            ("error.type", field_error_type, String),
            ("outcome", field_outcome, String),
        ]
    );
}

fn emit_debug_line(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "debug.line",
        def.severity,
        fields,
        [("outcome", field_outcome, String),]
    );
}

fn emit_error_typed(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "error.typed",
        def.severity,
        fields,
        [
            ("error.type", field_error_type, String),
            ("outcome", field_outcome, String),
        ]
    );
}

fn emit_isolation_decision(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "isolation.decision",
        def.severity,
        fields,
        [
            ("dind.mode", field_dind_mode, String),
            ("network.mode", field_network_mode, String),
            ("outcome", field_outcome, String),
            (
                "workspace.isolation.mode",
                field_workspace_isolation_mode,
                String
            ),
        ]
    );
}

fn emit_isolation_firewall_failed(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "isolation.firewall.failed",
        def.severity,
        fields,
        [
            ("error.type", field_error_type, String),
            ("network.mode", field_network_mode, String),
            ("outcome", field_outcome, String),
        ]
    );
}

fn emit_launch_stage_done(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "launch.stage.done",
        def.severity,
        fields,
        [("outcome", field_outcome, String),]
    );
}

fn emit_launch_stage_failed(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "launch.stage.failed",
        def.severity,
        fields,
        [
            ("error.type", field_error_type, String),
            ("outcome", field_outcome, String),
        ]
    );
}

fn emit_launch_stage_skipped(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "launch.stage.skipped",
        def.severity,
        fields,
        [("outcome", field_outcome, String),]
    );
}

fn emit_launch_stage_started(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "launch.stage.started",
        def.severity,
        fields,
        [("outcome", field_outcome, String),]
    );
}

fn emit_operation_log(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "operation.log",
        def.severity,
        fields,
        [
            ("outcome", field_outcome, String),
            ("session.id", field_session_id, String),
        ]
    );
}

fn emit_operation_warn(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "operation.warn",
        def.severity,
        fields,
        [
            ("outcome", field_outcome, String),
            ("session.id", field_session_id, String),
        ]
    );
}

fn emit_performance_slow_foreground_wait(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "performance.slow.foreground.wait",
        def.severity,
        fields,
        [("outcome", field_outcome, String),]
    );
}

fn emit_process_subprocess_done(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "process.subprocess.done",
        def.severity,
        fields,
        [
            ("error.type", field_error_type, String),
            ("outcome", field_outcome, String),
        ]
    );
}

fn emit_pty_exit(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!("pty.exit", def.severity, fields, []);
}

fn emit_pty_spawn(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!("pty.spawn", def.severity, fields, []);
}

fn emit_run_summary(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "run.summary",
        def.severity,
        fields,
        [("outcome", field_outcome, String),]
    );
}

fn emit_session_end(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!("session.end", def.severity, fields, []);
}

fn emit_session_start(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!("session.start", def.severity, fields, []);
}

fn emit_telemetry_validate(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "telemetry.validate",
        def.severity,
        fields,
        [(
            "telemetry.validation.values",
            field_telemetry_validation_values,
            StringArray
        ),]
    );
}

fn emit_timing_done(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "timing.done",
        def.severity,
        fields,
        [("outcome", field_outcome, String),]
    );
}

fn emit_timing_started(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "timing.started",
        def.severity,
        fields,
        [("outcome", field_outcome, String),]
    );
}

fn emit_trust_decision(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!(
        "trust.decision",
        def.severity,
        fields,
        [
            ("error.type", field_error_type, String),
            ("outcome", field_outcome, String),
            ("trust.decision", field_trust_decision, String),
            ("trust.source.type", field_trust_source_type, String),
        ]
    );
}

fn emit_ui_screen_entered(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!("ui.screen.entered", def.severity, fields, []);
}

fn emit_ui_screen_exited(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!("ui.screen.exited", def.severity, fields, []);
}

fn emit_ui_widget_focused(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!("ui.widget.focused", def.severity, fields, []);
}

fn emit_ui_widget_unfocused(def: &'static EventDef, fields: FieldSet<'_>) {
    emit_schema_event!("ui.widget.unfocused", def.severity, fields, []);
}

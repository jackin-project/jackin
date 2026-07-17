// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

pub const AGENT_STATE_CHANGED: EventDef =
    EventDef::generated(&schema::events::AGENT_STATE_CHANGED_DEF);
pub const AUTH_PROVISION: EventDef = EventDef::generated(&schema::events::AUTH_PROVISION_DEF);
pub const CACHE_DECISION: EventDef = EventDef::generated(&schema::events::CACHE_DECISION_DEF);
pub const CAPSULE_SESSION_CLEAN_SHUTDOWN: EventDef =
    EventDef::generated(&schema::events::CAPSULE_SESSION_CLEAN_SHUTDOWN_DEF);
pub const CAPSULE_SESSION_DETACH: EventDef =
    EventDef::generated(&schema::events::CAPSULE_SESSION_DETACH_DEF);
pub const CONFIG_OPERATION: EventDef = EventDef::generated(&schema::events::CONFIG_OPERATION_DEF);
pub const DEBUG_LINE: EventDef = EventDef::generated(&schema::events::DEBUG_LINE_DEF);
pub const ERROR_TYPED: EventDef = EventDef::generated(&schema::events::ERROR_TYPED_DEF);
pub const ISOLATION_DECISION: EventDef =
    EventDef::generated(&schema::events::ISOLATION_DECISION_DEF);
pub const ISOLATION_FIREWALL_FAILED: EventDef =
    EventDef::generated(&schema::events::ISOLATION_FIREWALL_FAILED_DEF);
pub const APP_CRASH: EventDef = EventDef::generated(&schema::events::APP_CRASH_DEF);
pub const APP_JANK: EventDef = EventDef::generated(&schema::events::APP_JANK_DEF);
pub const LAUNCH_STAGE_DONE: EventDef = EventDef::generated(&schema::events::LAUNCH_STAGE_DONE_DEF);
pub const LAUNCH_STAGE_FAILED: EventDef =
    EventDef::generated(&schema::events::LAUNCH_STAGE_FAILED_DEF);
pub const LAUNCH_STAGE_SKIPPED: EventDef =
    EventDef::generated(&schema::events::LAUNCH_STAGE_SKIPPED_DEF);
pub const LAUNCH_STAGE_STARTED: EventDef =
    EventDef::generated(&schema::events::LAUNCH_STAGE_STARTED_DEF);
pub const OPERATION_LOG: EventDef = EventDef::generated(&schema::events::OPERATION_LOG_DEF);
pub const OPERATION_WARN: EventDef = EventDef::generated(&schema::events::OPERATION_WARN_DEF);
pub const PERFORMANCE_SLOW_FOREGROUND_WAIT: EventDef =
    EventDef::generated(&schema::events::PERFORMANCE_SLOW_FOREGROUND_WAIT_DEF);
pub const PROCESS_SUBPROCESS_DONE: EventDef =
    EventDef::generated(&schema::events::PROCESS_SUBPROCESS_DONE_DEF);
pub const PTY_EXIT: EventDef = EventDef::generated(&schema::events::PTY_EXIT_DEF);
pub const PTY_SPAWN: EventDef = EventDef::generated(&schema::events::PTY_SPAWN_DEF);
pub const RUN_SUMMARY: EventDef = EventDef::generated(&schema::events::RUN_SUMMARY_DEF);
pub const SESSION_END: EventDef = EventDef::generated(&schema::events::SESSION_END_DEF);
pub const SESSION_START: EventDef = EventDef::generated(&schema::events::SESSION_START_DEF);
pub const TELEMETRY_VALIDATE: EventDef =
    EventDef::generated(&schema::events::TELEMETRY_VALIDATE_DEF);
pub const TIMING_DONE: EventDef = EventDef::generated(&schema::events::TIMING_DONE_DEF);
pub const TIMING_STARTED: EventDef = EventDef::generated(&schema::events::TIMING_STARTED_DEF);
pub const TRUST_DECISION: EventDef = EventDef::generated(&schema::events::TRUST_DECISION_DEF);
pub const UI_SCREEN_ENTERED: EventDef = EventDef::generated(&schema::events::UI_SCREEN_ENTERED_DEF);
pub const UI_SCREEN_EXITED: EventDef = EventDef::generated(&schema::events::UI_SCREEN_EXITED_DEF);
pub const UI_WIDGET_FOCUSED: EventDef = EventDef::generated(&schema::events::UI_WIDGET_FOCUSED_DEF);
pub const UI_WIDGET_UNFOCUSED: EventDef =
    EventDef::generated(&schema::events::UI_WIDGET_UNFOCUSED_DEF);

pub const ALL: &[EventDef] = &[
    AGENT_STATE_CHANGED,
    AUTH_PROVISION,
    CACHE_DECISION,
    CAPSULE_SESSION_CLEAN_SHUTDOWN,
    CAPSULE_SESSION_DETACH,
    CONFIG_OPERATION,
    DEBUG_LINE,
    ERROR_TYPED,
    ISOLATION_DECISION,
    ISOLATION_FIREWALL_FAILED,
    APP_CRASH,
    APP_JANK,
    LAUNCH_STAGE_DONE,
    LAUNCH_STAGE_FAILED,
    LAUNCH_STAGE_SKIPPED,
    LAUNCH_STAGE_STARTED,
    OPERATION_LOG,
    OPERATION_WARN,
    PERFORMANCE_SLOW_FOREGROUND_WAIT,
    PROCESS_SUBPROCESS_DONE,
    PTY_EXIT,
    PTY_SPAWN,
    RUN_SUMMARY,
    SESSION_END,
    SESSION_START,
    TELEMETRY_VALIDATE,
    TIMING_DONE,
    TIMING_STARTED,
    TRUST_DECISION,
    UI_SCREEN_ENTERED,
    UI_SCREEN_EXITED,
    UI_WIDGET_FOCUSED,
    UI_WIDGET_UNFOCUSED,
];

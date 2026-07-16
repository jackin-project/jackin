// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

pub const SESSION_START: &str = "session.start";
pub const SESSION_END: &str = "session.end";
pub const UI_SCREEN_ENTERED: &str = "ui.screen.entered";
pub const UI_SCREEN_EXITED: &str = "ui.screen.exited";
pub const UI_WIDGET_FOCUSED: &str = "ui.widget.focused";
pub const UI_WIDGET_UNFOCUSED: &str = "ui.widget.unfocused";
pub const APP_JANK: &str = "app.jank";
pub const APP_CRASH: &str = "app.crash";
pub const AGENT_STATE_CHANGED: &str = "agent.state.changed";
pub const PTY_SPAWN: &str = "pty.spawn";
pub const PTY_EXIT: &str = "pty.exit";
pub const TELEMETRY_VALIDATE: &str = "telemetry.validate";
pub const LAUNCH_STAGE_STARTED: &str = "launch.stage.started";
pub const LAUNCH_STAGE_DONE: &str = "launch.stage.done";
pub const LAUNCH_STAGE_FAILED: &str = "launch.stage.failed";
pub const LAUNCH_STAGE_SKIPPED: &str = "launch.stage.skipped";
pub const TIMING_STARTED: &str = "timing.started";
pub const TIMING_DONE: &str = "timing.done";
pub const DEBUG_LINE: &str = "debug.line";
pub const PROCESS_SUBPROCESS_DONE: &str = "process.subprocess.done";
pub const RUN_SUMMARY: &str = "run.summary";
pub const PERFORMANCE_SLOW_FOREGROUND_WAIT: &str = "performance.slow.foreground.wait";
pub const CAPSULE_SESSION_DETACH: &str = "capsule.session.detach";
pub const CAPSULE_SESSION_CLEAN_SHUTDOWN: &str = "capsule.session.clean.shutdown";
pub const ERROR_TYPED: &str = "error.typed";
pub const OPERATION_LOG: &str = "operation.log";
pub const OPERATION_WARN: &str = "operation.warn";

pub const ALL: &[&str] = &[
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
    OPERATION_LOG,
    OPERATION_WARN,
];

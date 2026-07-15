// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Diagnostics event `kind` names referenced by the telemetry taxonomy.

pub const STAGE_STARTED: &str = "stage_started";
pub const STAGE_DONE: &str = "stage_done";
pub const STAGE_FAILED: &str = "stage_failed";
pub const STAGE_SKIPPED: &str = "stage_skipped";
pub const TIMING_STARTED: &str = "timing_started";
pub const TIMING_DONE: &str = "timing_done";
pub const DEBUG: &str = "debug";
pub const SUBPROCESS_DONE: &str = "subprocess_done";
pub const OTLP_INTERNAL: &str = "otlp_internal";
pub const RUN_SUMMARY: &str = "run_summary";
pub const SLOW_FOREGROUND_WAIT: &str = "slow_foreground_wait";
pub const SESSION_DETACH: &str = "session_detach";
pub const CLEAN_SHUTDOWN: &str = "clean_shutdown";
/// Host subprocess span name (`ShellRunner` choke point, plan 041).
pub const PROCESS_EXECUTE: &str = "process.execute";

pub const ALL: &[&str] = &[
    STAGE_STARTED,
    STAGE_DONE,
    STAGE_FAILED,
    STAGE_SKIPPED,
    TIMING_STARTED,
    TIMING_DONE,
    DEBUG,
    SUBPROCESS_DONE,
    OTLP_INTERNAL,
    RUN_SUMMARY,
    SLOW_FOREGROUND_WAIT,
    SESSION_DETACH,
    CLEAN_SHUTDOWN,
    PROCESS_EXECUTE,
];

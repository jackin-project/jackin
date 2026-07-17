// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

pub const APP_SHUTDOWN: SpanDef = SpanDef::generated(&schema::spans::APP_SHUTDOWN_DEF);
pub const APP_STARTUP: SpanDef = SpanDef::generated(&schema::spans::APP_STARTUP_DEF);
pub const BACKGROUND_CYCLE: SpanDef = SpanDef::generated(&schema::spans::BACKGROUND_CYCLE_DEF);
pub const CLI_COMMAND: SpanDef = SpanDef::generated(&schema::spans::CLI_COMMAND_DEF);
pub const CONNECTION_ATTEMPT: SpanDef = SpanDef::generated(&schema::spans::CONNECTION_ATTEMPT_DEF);
pub const DB_CLIENT: SpanDef = SpanDef::generated(&schema::spans::DB_CLIENT_DEF);
pub const HTTP_CLIENT: SpanDef = SpanDef::generated(&schema::spans::HTTP_CLIENT_DEF);
pub const LAUNCH: SpanDef = SpanDef::generated(&schema::spans::LAUNCH_DEF);
pub const LAUNCH_STAGE: SpanDef = SpanDef::generated(&schema::spans::LAUNCH_STAGE_DEF);
pub const PREWARM_ATTEMPT: SpanDef = SpanDef::generated(&schema::spans::PREWARM_ATTEMPT_DEF);
pub const PREWARM_SCHEDULE: SpanDef = SpanDef::generated(&schema::spans::PREWARM_SCHEDULE_DEF);
pub const PROCESS_COMMAND: SpanDef = SpanDef::generated(&schema::spans::PROCESS_COMMAND_DEF);
pub const RPC_CLIENT: SpanDef = SpanDef::generated(&schema::spans::RPC_CLIENT_DEF);
pub const RPC_SERVER: SpanDef = SpanDef::generated(&schema::spans::RPC_SERVER_DEF);
pub const TELEMETRY_VALIDATE: SpanDef = SpanDef::generated(&schema::spans::TELEMETRY_VALIDATE_DEF);
pub const UI_ACTION: SpanDef = SpanDef::generated(&schema::spans::UI_ACTION_DEF);
pub const UI_RENDER: SpanDef = SpanDef::generated(&schema::spans::UI_RENDER_DEF);
pub const UI_SCREEN_TRANSITION: SpanDef =
    SpanDef::generated(&schema::spans::UI_SCREEN_TRANSITION_DEF);

pub const ALL: &[SpanDef] = &[
    APP_SHUTDOWN,
    APP_STARTUP,
    BACKGROUND_CYCLE,
    CLI_COMMAND,
    CONNECTION_ATTEMPT,
    DB_CLIENT,
    HTTP_CLIENT,
    LAUNCH,
    LAUNCH_STAGE,
    PREWARM_ATTEMPT,
    PREWARM_SCHEDULE,
    PROCESS_COMMAND,
    RPC_CLIENT,
    RPC_SERVER,
    TELEMETRY_VALIDATE,
    UI_ACTION,
    UI_RENDER,
    UI_SCREEN_TRANSITION,
];

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

pub const AGENT_STATE_FLAPS: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::AGENT_STATE_FLAPS_DEF);
pub const AGENT_STATE_STUCK: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::AGENT_STATE_STUCK_DEF);
pub const AGENT_STATE_TRANSITIONS: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::AGENT_STATE_TRANSITIONS_DEF);
pub const BACKGROUND_CYCLE_DURATION: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::BACKGROUND_CYCLE_DURATION_DEF);
pub const BACKGROUND_CYCLES: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::BACKGROUND_CYCLES_DEF);
pub const CACHE_DECISIONS: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::CACHE_DECISIONS_DEF);
pub const CLI_DURATION: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::CLI_DURATION_DEF);
pub const CLI_FAILURES: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::CLI_FAILURES_DEF);
pub const CLI_INVOCATIONS: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::CLI_INVOCATIONS_DEF);
pub const CONNECTION_ACTIVE: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::CONNECTION_ACTIVE_DEF);
pub const CONNECTION_ATTEMPTS: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::CONNECTION_ATTEMPTS_DEF);
pub const CONNECTION_DURATION: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::CONNECTION_DURATION_DEF);
pub const DB_CLIENT_OPERATION_DURATION: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::DB_CLIENT_OPERATION_DURATION_DEF);
pub const GEN_AI_CLIENT_TOKEN_USAGE: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::GEN_AI_CLIENT_TOKEN_USAGE_DEF);
pub const LAUNCH_STAGE_ACTIVE: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::LAUNCH_STAGE_ACTIVE_DEF);
pub const LAUNCH_STAGE_DURATION: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::LAUNCH_STAGE_DURATION_DEF);
pub const LAUNCH_STAGE_EXECUTIONS: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::LAUNCH_STAGE_EXECUTIONS_DEF);
pub const PREWARM_ACTIVE: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::PREWARM_ACTIVE_DEF);
pub const PREWARM_DURATION: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::PREWARM_DURATION_DEF);
pub const PREWARM_JOBS: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::PREWARM_JOBS_DEF);
pub const PROCESS_CPU_TIME: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::PROCESS_CPU_TIME_DEF);
pub const PROCESS_MEMORY_USAGE: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::PROCESS_MEMORY_USAGE_DEF);
pub const PROCESS_UPTIME: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::PROCESS_UPTIME_DEF);
pub const RPC_ACTIVE: InstrumentDef = InstrumentDef::generated(&schema::metrics::RPC_ACTIVE_DEF);
pub const RPC_DURATION: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::RPC_DURATION_DEF);
pub const RPC_REQUESTS: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::RPC_REQUESTS_DEF);
pub const TELEMETRY_REJECTIONS: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::TELEMETRY_REJECTIONS_DEF);
pub const TELEMETRY_VALIDATE: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::TELEMETRY_VALIDATE_DEF);
pub const TERMINAL_CURSOR_MOVES: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::TERMINAL_CURSOR_MOVES_DEF);
pub const TERMINAL_INPUT_MOUSE: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::TERMINAL_INPUT_MOUSE_DEF);
pub const TERMINAL_BYTES: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::TERMINAL_IO_BYTES_DEF);
pub const TERMINAL_RENDER_CELLS: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::TERMINAL_RENDER_CELLS_DEF);
pub const TERMINAL_RENDER_DURATION: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::TERMINAL_RENDER_DURATION_DEF);
pub const TERMINAL_RENDER_FRAMES: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::TERMINAL_RENDER_FRAMES_DEF);
pub const UI_ACTIONS: InstrumentDef = InstrumentDef::generated(&schema::metrics::UI_ACTIONS_DEF);
pub const UI_FOCUS_DURATION: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::UI_FOCUS_DURATION_DEF);
pub const UI_JANK: InstrumentDef = InstrumentDef::generated(&schema::metrics::UI_JANK_DEF);
pub const UI_RENDER_DURATION: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::UI_RENDER_DURATION_DEF);
pub const UI_DWELL: InstrumentDef = InstrumentDef::generated(&schema::metrics::UI_SCREEN_DWELL_DEF);
pub const UI_TRANSITIONS: InstrumentDef =
    InstrumentDef::generated(&schema::metrics::UI_TRANSITIONS_DEF);

pub const ALL: &[InstrumentDef] = &[
    AGENT_STATE_FLAPS,
    AGENT_STATE_STUCK,
    AGENT_STATE_TRANSITIONS,
    BACKGROUND_CYCLE_DURATION,
    BACKGROUND_CYCLES,
    CACHE_DECISIONS,
    CLI_DURATION,
    CLI_FAILURES,
    CLI_INVOCATIONS,
    CONNECTION_ACTIVE,
    CONNECTION_ATTEMPTS,
    CONNECTION_DURATION,
    DB_CLIENT_OPERATION_DURATION,
    GEN_AI_CLIENT_TOKEN_USAGE,
    LAUNCH_STAGE_ACTIVE,
    LAUNCH_STAGE_DURATION,
    LAUNCH_STAGE_EXECUTIONS,
    PREWARM_ACTIVE,
    PREWARM_DURATION,
    PREWARM_JOBS,
    PROCESS_CPU_TIME,
    PROCESS_MEMORY_USAGE,
    PROCESS_UPTIME,
    RPC_ACTIVE,
    RPC_DURATION,
    RPC_REQUESTS,
    TELEMETRY_REJECTIONS,
    TELEMETRY_VALIDATE,
    TERMINAL_CURSOR_MOVES,
    TERMINAL_INPUT_MOUSE,
    TERMINAL_BYTES,
    TERMINAL_RENDER_CELLS,
    TERMINAL_RENDER_DURATION,
    TERMINAL_RENDER_FRAMES,
    UI_ACTIONS,
    UI_FOCUS_DURATION,
    UI_JANK,
    UI_RENDER_DURATION,
    UI_DWELL,
    UI_TRANSITIONS,
];

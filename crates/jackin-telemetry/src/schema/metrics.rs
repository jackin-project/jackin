// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub const CLI_INVOCATIONS: &str = "cli.invocations";
pub const CLI_DURATION: &str = "cli.duration";
pub const CLI_FAILURES: &str = "cli.failures";
pub const UI_TRANSITIONS: &str = "ui.transitions";
pub const UI_ACTIONS: &str = "ui.actions";
pub const UI_SCREEN_DWELL: &str = "ui.screen.dwell";
pub const UI_FOCUS_DURATION: &str = "ui.focus.duration";
pub const UI_RENDER_DURATION: &str = "ui.render.duration";
pub const LAUNCH_STAGE_DURATION: &str = "launch.stage.duration";
pub const LAUNCH_CACHE_REUSE: &str = "launch.cache.reuse";
pub const PREWARM_JOBS: &str = "prewarm.jobs";
pub const PREWARM_ACTIVE: &str = "prewarm.active";
pub const PREWARM_DURATION: &str = "prewarm.duration";
pub const BACKGROUND_CYCLES: &str = "background.cycles";
pub const BACKGROUND_CYCLE_DURATION: &str = "background.cycle.duration";
pub const CONNECTION_ATTEMPTS: &str = "connection.attempts";
pub const CONNECTION_ACTIVE: &str = "connection.active";
pub const CONNECTION_DURATION: &str = "connection.duration";
pub const RPC_REQUESTS: &str = "rpc.requests";
pub const RPC_ACTIVE: &str = "rpc.active";
pub const RPC_DURATION: &str = "rpc.duration";
pub const AGENT_STATE_TRANSITIONS: &str = "agent.state.transitions";
pub const AGENT_STATE_STUCK: &str = "agent.state.stuck";
pub const AGENT_STATE_FLAPS: &str = "agent.state.flaps";
pub const TERMINAL_IO_BYTES: &str = "terminal.io.bytes";
pub const TERMINAL_CURSOR_MOVES: &str = "terminal.cursor.moves";
pub const TERMINAL_RENDER_CELLS: &str = "terminal.render.cells";
pub const TERMINAL_RENDER_DURATION: &str = "terminal.render.duration";
pub const TERMINAL_RENDER_FRAMES: &str = "terminal.render.frames";
pub const TERMINAL_INPUT_MOUSE: &str = "terminal.input.mouse";
pub const TELEMETRY_REJECTIONS: &str = "telemetry.rejections";
pub const TELEMETRY_VALIDATE: &str = "telemetry.validate";

pub const ALL: &[&str] = &[
    CLI_INVOCATIONS,
    CLI_DURATION,
    CLI_FAILURES,
    UI_TRANSITIONS,
    UI_ACTIONS,
    UI_SCREEN_DWELL,
    UI_FOCUS_DURATION,
    UI_RENDER_DURATION,
    LAUNCH_STAGE_DURATION,
    LAUNCH_CACHE_REUSE,
    PREWARM_JOBS,
    PREWARM_ACTIVE,
    PREWARM_DURATION,
    BACKGROUND_CYCLES,
    BACKGROUND_CYCLE_DURATION,
    CONNECTION_ATTEMPTS,
    CONNECTION_ACTIVE,
    CONNECTION_DURATION,
    RPC_REQUESTS,
    RPC_ACTIVE,
    RPC_DURATION,
    AGENT_STATE_TRANSITIONS,
    AGENT_STATE_STUCK,
    AGENT_STATE_FLAPS,
    TERMINAL_IO_BYTES,
    TERMINAL_CURSOR_MOVES,
    TERMINAL_RENDER_CELLS,
    TERMINAL_RENDER_DURATION,
    TERMINAL_RENDER_FRAMES,
    TERMINAL_INPUT_MOUSE,
    TELEMETRY_REJECTIONS,
    TELEMETRY_VALIDATE,
];

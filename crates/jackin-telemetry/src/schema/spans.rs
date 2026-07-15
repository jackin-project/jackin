// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

pub const CLI_COMMAND: &str = "cli.command";
pub const APP_STARTUP: &str = "app.startup";
pub const APP_SHUTDOWN: &str = "app.shutdown";
pub const UI_ACTION: &str = "ui.action";
pub const UI_SCREEN_TRANSITION: &str = "ui.screen.transition";
pub const UI_RENDER: &str = "ui.render";
pub const BACKGROUND_CYCLE: &str = "background.cycle";
pub const CONNECTION_ATTEMPT: &str = "connection.attempt";
pub const PROCESS_COMMAND: &str = "process.command";
pub const RPC_CLIENT: &str = "rpc.client";
pub const RPC_SERVER: &str = "rpc.server";
pub const TELEMETRY_VALIDATE: &str = "telemetry.validate";

pub const ALL: &[&str] = &[
    CLI_COMMAND,
    APP_STARTUP,
    APP_SHUTDOWN,
    UI_ACTION,
    UI_SCREEN_TRANSITION,
    UI_RENDER,
    BACKGROUND_CYCLE,
    CONNECTION_ATTEMPT,
    PROCESS_COMMAND,
    RPC_CLIENT,
    RPC_SERVER,
    TELEMETRY_VALIDATE,
];

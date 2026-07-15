// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

pub const APP_MODE: &str = "app.mode";
pub const CLI_INVOCATION_ID: &str = "cli.invocation.id";
pub const CLI_COMMAND_NAME: &str = "cli.command.name";
pub const UI_ACTION_NAME: &str = "ui.action.name";
pub const UI_SCREEN_VISIT_ID: &str = "ui.screen.visit.id";
pub const UI_NAVIGATION_SEQUENCE: &str = "ui.navigation.sequence";
pub const UI_TRANSITION_REASON: &str = "ui.transition.reason";
pub const JOB_ID: &str = "job.id";
pub const JOB_TYPE: &str = "job.type";
pub const OUTCOME: &str = "outcome";
pub const LAUNCH_STAGE_NAME: &str = "launch.stage.name";
pub const LAUNCH_TARGET_KIND: &str = "launch.target.kind";
pub const BACKGROUND_CYCLE_NAME: &str = "background.cycle.name";
pub const CONNECTION_PEER_TYPE: &str = "connection.peer.type";
pub const AGENT_STATE: &str = "agent.state";
pub const AGENT_STATUS_SOURCE: &str = "agent.status.source";
pub const AGENT_STATUS_CONFIDENCE: &str = "agent.status.confidence";
pub const AGENT_STATUS_STUCK: &str = "agent.status.stuck";
pub const AUTH_MODE: &str = "auth.mode";
pub const CREDENTIAL_SOURCE_TYPE: &str = "credential.source.type";
pub const WORKSPACE_ISOLATION_MODE: &str = "workspace.isolation.mode";
pub const NETWORK_MODE: &str = "network.mode";
pub const DIND_MODE: &str = "dind.mode";
pub const CONFIG_SCOPE: &str = "config.scope";
pub const CONFIG_OPERATION: &str = "config.operation";
pub const CONFIG_SCHEMA_VERSION_FROM: &str = "config.schema.version.from";
pub const CONFIG_SCHEMA_VERSION_TO: &str = "config.schema.version.to";
pub const CONFIG_MIGRATION_STEP_COUNT: &str = "config.migration.step_count";
pub const TRUST_DECISION: &str = "trust.decision";
pub const TRUST_SOURCE_TYPE: &str = "trust.source.type";
pub const CACHE_NAME: &str = "cache.name";
pub const CACHE_RESULT: &str = "cache.result";
pub const PTY_EXIT_REASON: &str = "pty.exit.reason";
pub const STREAM_DIRECTION: &str = "stream.direction";
pub const TELEMETRY_SIGNAL: &str = "telemetry.signal";
pub const TELEMETRY_REJECTION_REASON: &str = "telemetry.rejection.reason";

pub const ALL_KEYS: &[&str] = &[
    APP_MODE,
    CLI_INVOCATION_ID,
    CLI_COMMAND_NAME,
    UI_ACTION_NAME,
    UI_SCREEN_VISIT_ID,
    UI_NAVIGATION_SEQUENCE,
    UI_TRANSITION_REASON,
    JOB_ID,
    JOB_TYPE,
    OUTCOME,
    LAUNCH_STAGE_NAME,
    LAUNCH_TARGET_KIND,
    BACKGROUND_CYCLE_NAME,
    CONNECTION_PEER_TYPE,
    AGENT_STATE,
    AGENT_STATUS_SOURCE,
    AGENT_STATUS_CONFIDENCE,
    AGENT_STATUS_STUCK,
    AUTH_MODE,
    CREDENTIAL_SOURCE_TYPE,
    WORKSPACE_ISOLATION_MODE,
    NETWORK_MODE,
    DIND_MODE,
    CONFIG_SCOPE,
    CONFIG_OPERATION,
    CONFIG_SCHEMA_VERSION_FROM,
    CONFIG_SCHEMA_VERSION_TO,
    CONFIG_MIGRATION_STEP_COUNT,
    TRUST_DECISION,
    TRUST_SOURCE_TYPE,
    CACHE_NAME,
    CACHE_RESULT,
    PTY_EXIT_REASON,
    STREAM_DIRECTION,
    TELEMETRY_SIGNAL,
    TELEMETRY_REJECTION_REASON,
];

/// Standard semantic-convention keys are isolated here so downstream crates
/// never depend on the upstream module layout.
pub mod std_attrs {
    pub const SERVICE_NAME: &str = "service.name";
    pub const SERVICE_NAMESPACE: &str = "service.namespace";
    pub const SERVICE_VERSION: &str = "service.version";
    pub const SERVICE_INSTANCE_ID: &str = "service.instance.id";
    pub const PROCESS_PID: &str = "process.pid";
    pub const PROCESS_EXECUTABLE_NAME: &str = "process.executable.name";
    pub const PROCESS_EXIT_CODE: &str = "process.exit.code";
    pub const PROCESS_COMMAND: &str = "process.command";
    pub const CONTAINER_ID: &str = "container.id";
    pub const SESSION_ID: &str = "session.id";
    pub const SESSION_PREVIOUS_ID: &str = "session.previous_id";
    pub const APP_SCREEN_ID: &str = "app.screen.id";
    pub const APP_SCREEN_NAME: &str = "app.screen.name";
    pub const APP_WIDGET_ID: &str = "app.widget.id";
    pub const APP_WIDGET_NAME: &str = "app.widget.name";
    pub const GEN_AI_AGENT_NAME: &str = "gen_ai.agent.name";
    pub const GEN_AI_CONVERSATION_ID: &str = "gen_ai.conversation.id";
    pub const GEN_AI_PROVIDER_NAME: &str = "gen_ai.provider.name";
    pub const ERROR_TYPE: &str = "error.type";
    pub const RPC_SYSTEM_NAME: &str = "rpc.system.name";
    pub const RPC_METHOD: &str = "rpc.method";
    pub const HTTP_REQUEST_METHOD: &str = "http.request.method";
    pub const URL_TEMPLATE: &str = "url.template";
    pub const SERVER_ADDRESS: &str = "server.address";
    pub const DB_SYSTEM_NAME: &str = "db.system.name";
    pub const DB_OPERATION_NAME: &str = "db.operation.name";
    pub const ALL_KEYS: &[&str] = &[
        SERVICE_NAME,
        SERVICE_NAMESPACE,
        SERVICE_VERSION,
        SERVICE_INSTANCE_ID,
        PROCESS_PID,
        PROCESS_EXECUTABLE_NAME,
        PROCESS_EXIT_CODE,
        PROCESS_COMMAND,
        CONTAINER_ID,
        SESSION_ID,
        SESSION_PREVIOUS_ID,
        APP_SCREEN_ID,
        APP_SCREEN_NAME,
        APP_WIDGET_ID,
        APP_WIDGET_NAME,
        GEN_AI_AGENT_NAME,
        GEN_AI_CONVERSATION_ID,
        GEN_AI_PROVIDER_NAME,
        ERROR_TYPE,
        RPC_SYSTEM_NAME,
        RPC_METHOD,
        HTTP_REQUEST_METHOD,
        URL_TEMPLATE,
        SERVER_ADDRESS,
        DB_SYSTEM_NAME,
        DB_OPERATION_NAME,
    ];
}

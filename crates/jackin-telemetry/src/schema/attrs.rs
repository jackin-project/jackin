// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

// registry-type: enum
pub const AGENT_STATE: &str = "agent.state";
pub const AGENT_STATE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: AGENT_STATE,
    description: "Effective coding-agent state.",
    value_type: super::ValueType::String,
    allowed_values: &["working", "blocked", "done", "idle", "unknown"],
};
// registry-type: enum
pub const AGENT_STATUS_CONFIDENCE: &str = "agent.status.confidence";
pub const AGENT_STATUS_CONFIDENCE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: AGENT_STATUS_CONFIDENCE,
    description: "Agent status confidence.",
    value_type: super::ValueType::String,
    allowed_values: &["unknown", "weak", "strong", "authoritative"],
};
// registry-type: enum
pub const AGENT_STATUS_SOURCE: &str = "agent.status.source";
pub const AGENT_STATUS_SOURCE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: AGENT_STATUS_SOURCE,
    description: "Agent status evidence class.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "none",
        "visible_screen",
        "shell_integration",
        "foreground_process",
        "reported",
    ],
};
// registry-type: boolean
pub const AGENT_STATUS_STUCK: &str = "agent.status.stuck";
pub const AGENT_STATUS_STUCK_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: AGENT_STATUS_STUCK,
    description: "Whether the status authority considers the agent stuck.",
    value_type: super::ValueType::Boolean,
    allowed_values: &[],
};
// registry-type: enum
pub const APP_MODE: &str = "app.mode";
pub const APP_MODE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: APP_MODE,
    description: "Application mode.",
    value_type: super::ValueType::String,
    allowed_values: &["one_shot", "interactive", "daemon", "capsule"],
};
// registry-type: enum
pub const APP_SCREEN_ID: &str = "app.screen.id";
pub const APP_SCREEN_ID_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: APP_SCREEN_ID,
    description: "Stable application screen identifier.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "workspace.list",
        "workspace.editor",
        "settings",
        "workspace.create",
        "launch.progress",
        "capsule",
    ],
};
// registry-type: string
pub const APP_SCREEN_NAME: &str = "app.screen.name";
pub const APP_SCREEN_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: APP_SCREEN_NAME,
    description: "Stable application screen name.",
    value_type: super::ValueType::String,
    allowed_values: &[],
};
// registry-type: string
pub const APP_WIDGET_ID: &str = "app.widget.id";
pub const APP_WIDGET_ID_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: APP_WIDGET_ID,
    description: "Stable application widget identifier.",
    value_type: super::ValueType::String,
    allowed_values: &[],
};
// registry-type: string
pub const APP_WIDGET_NAME: &str = "app.widget.name";
pub const APP_WIDGET_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: APP_WIDGET_NAME,
    description: "Stable application widget name.",
    value_type: super::ValueType::String,
    allowed_values: &[],
};
// registry-type: enum
pub const AUTH_MODE: &str = "auth.mode";
pub const AUTH_MODE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: AUTH_MODE,
    description: "Authentication mode.",
    value_type: super::ValueType::String,
    allowed_values: &["sync", "api_key", "oauth_token", "ignore"],
};
// registry-type: enum
pub const BACKGROUND_CYCLE_NAME: &str = "background.cycle.name";
pub const BACKGROUND_CYCLE_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: BACKGROUND_CYCLE_NAME,
    description: "Periodic cycle class.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "branch_context",
        "pr_context",
        "usage_account",
        "provider_probe",
        "instance_refresh",
        "agent_status",
    ],
};
// registry-type: enum
pub const CACHE_NAME: &str = "cache.name";
pub const CACHE_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: CACHE_NAME,
    description: "Product cache class.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "role_repository",
        "agent_binary",
        "capsule_binary",
        "derived_image",
        "usage_snapshot",
    ],
};
// registry-type: enum
pub const CACHE_RESULT: &str = "cache.result";
pub const CACHE_RESULT_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: CACHE_RESULT,
    description: "Cache operation result.",
    value_type: super::ValueType::String,
    allowed_values: &["hit", "miss", "stale", "reuse", "bypass"],
};
// registry-type: enum
pub const CLI_COMMAND_NAME: &str = "cli.command.name";
pub const CLI_COMMAND_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: CLI_COMMAND_NAME,
    description: "Registry-backed command path.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "load",
        "hardline",
        "eject",
        "exile",
        "purge",
        "prewarm",
        "prune",
        "console",
        "role",
        "workspace",
        "config",
        "daemon",
        "doctor",
        "diagnostics",
        "status",
        "usage",
        "help",
        "role.validate",
        "role.migrate",
        "role.create",
        "role.construct-version",
        "role.published-image",
        "role.published-image-repository",
        "role.publish-labels",
        "daemon.serve",
        "daemon.install",
        "daemon.uninstall",
        "daemon.start",
        "daemon.stop",
        "daemon.restart",
        "daemon.status",
        "diagnostics.validate",
        "workspace.create",
        "workspace.list",
        "workspace.show",
        "workspace.edit",
        "workspace.prune",
        "workspace.remove",
        "workspace.env",
        "workspace.env.set",
        "workspace.env.unset",
        "workspace.env.list",
        "workspace.claude-token",
        "workspace.claude-token.setup",
        "workspace.claude-token.rotate",
        "workspace.claude-token.revoke",
        "workspace.claude-token.doctor",
        "config.mount",
        "config.mount.add",
        "config.mount.remove",
        "config.mount.list",
        "config.trust",
        "config.trust.grant",
        "config.trust.revoke",
        "config.trust.list",
        "config.auth",
        "config.auth.set",
        "config.auth.show",
        "config.env",
        "config.env.set",
        "config.env.unset",
        "config.env.list",
        "config.git",
        "config.git.coauthor-trailer",
        "config.git.coauthor-trailer.enable",
        "config.git.coauthor-trailer.disable",
        "config.git.dco",
        "config.git.dco.enable",
        "config.git.dco.disable",
        "prune.roles",
        "prune.cache",
        "prune.images",
        "prune.instances",
        "prune.system",
        "usage.accounts",
        "usage.verify",
    ],
};
// registry-type: string
pub const CLI_INVOCATION_ID: &str = "cli.invocation.id";
pub const CLI_INVOCATION_ID_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: CLI_INVOCATION_ID,
    description: "Opaque top-level invocation identifier.",
    value_type: super::ValueType::String,
    allowed_values: &[],
};
// registry-type: int
pub const CONFIG_MIGRATION_STEP_COUNT: &str = "config.migration.step_count";
pub const CONFIG_MIGRATION_STEP_COUNT_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: CONFIG_MIGRATION_STEP_COUNT,
    description: "Applied configuration migration count.",
    value_type: super::ValueType::Integer,
    allowed_values: &[],
};
// registry-type: enum
pub const CONFIG_OPERATION: &str = "config.operation";
pub const CONFIG_OPERATION_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: CONFIG_OPERATION,
    description: "Configuration operation.",
    value_type: super::ValueType::String,
    allowed_values: &["load", "validate", "migrate", "save"],
};
// registry-type: string
pub const CONFIG_SCHEMA_VERSION_FROM: &str = "config.schema.version.from";
pub const CONFIG_SCHEMA_VERSION_FROM_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: CONFIG_SCHEMA_VERSION_FROM,
    description: "Previous configuration schema version, validated against config.scope.",
    value_type: super::ValueType::String,
    allowed_values: &[],
};
// registry-type: string
pub const CONFIG_SCHEMA_VERSION_TO: &str = "config.schema.version.to";
pub const CONFIG_SCHEMA_VERSION_TO_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: CONFIG_SCHEMA_VERSION_TO,
    description: "New configuration schema version, validated against config.scope and never legacy.",
    value_type: super::ValueType::String,
    allowed_values: &[],
};
// registry-type: enum
pub const CONFIG_SCOPE: &str = "config.scope";
pub const CONFIG_SCOPE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: CONFIG_SCOPE,
    description: "Configuration scope.",
    value_type: super::ValueType::String,
    allowed_values: &["global", "workspace"],
};
// registry-type: enum
pub const CONNECTION_PEER_TYPE: &str = "connection.peer.type";
pub const CONNECTION_PEER_TYPE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: CONNECTION_PEER_TYPE,
    description: "Connection peer class.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "host_daemon",
        "capsule_control",
        "capsule_attach",
        "docker",
        "provider",
        "parallax",
    ],
};
// registry-type: enum
pub const CREDENTIAL_SOURCE_TYPE: &str = "credential.source.type";
pub const CREDENTIAL_SOURCE_TYPE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: CREDENTIAL_SOURCE_TYPE,
    description: "Credential source class.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "environment",
        "agent_home",
        "onepassword",
        "github_cli",
        "oauth_store",
        "none",
    ],
};
// registry-type: enum
pub const DB_OPERATION_NAME: &str = "db.operation.name";
pub const DB_OPERATION_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: DB_OPERATION_NAME,
    description: "Database operation name.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "connect", "begin", "commit", "rollback", "select", "insert", "upsert", "update", "delete",
    ],
};
// registry-type: enum
pub const DIND_MODE: &str = "dind.mode";
pub const DIND_MODE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: DIND_MODE,
    description: "Docker-in-Docker mode.",
    value_type: super::ValueType::String,
    allowed_values: &["none", "rootless", "privileged"],
};
// registry-type: enum
pub const GEN_AI_AGENT_NAME: &str = "gen_ai.agent.name";
pub const GEN_AI_AGENT_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: GEN_AI_AGENT_NAME,
    description: "Generative AI agent name.",
    value_type: super::ValueType::String,
    allowed_values: &["claude", "codex", "amp", "kimi", "opencode", "grok"],
};
// registry-type: string
pub const GEN_AI_CONVERSATION_ID: &str = "gen_ai.conversation.id";
pub const GEN_AI_CONVERSATION_ID_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: GEN_AI_CONVERSATION_ID,
    description: "Generative AI conversation identifier.",
    value_type: super::ValueType::String,
    allowed_values: &[],
};
// registry-type: enum
pub const GEN_AI_OPERATION_NAME: &str = "gen_ai.operation.name";
pub const GEN_AI_OPERATION_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: GEN_AI_OPERATION_NAME,
    description: "Generative AI operation name.",
    value_type: super::ValueType::String,
    allowed_values: &["chat"],
};
// registry-type: enum
pub const GEN_AI_PROVIDER_NAME: &str = "gen_ai.provider.name";
pub const GEN_AI_PROVIDER_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: GEN_AI_PROVIDER_NAME,
    description: "Generative AI provider name.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "anthropic",
        "openai",
        "amp",
        "xai",
        "zai",
        "minimax",
        "kimi",
    ],
};
// registry-type: enum
pub const GEN_AI_TOKEN_TYPE: &str = "gen_ai.token.type";
pub const GEN_AI_TOKEN_TYPE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: GEN_AI_TOKEN_TYPE,
    description: "Generative AI token type.",
    value_type: super::ValueType::String,
    allowed_values: &["input", "output"],
};
// registry-type: string
pub const JOB_ID: &str = "job.id";
pub const JOB_ID_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: JOB_ID,
    description: "Opaque detached job identifier.",
    value_type: super::ValueType::String,
    allowed_values: &[],
};
// registry-type: enum
pub const JOB_TYPE: &str = "job.type";
pub const JOB_TYPE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: JOB_TYPE,
    description: "Detached job class.",
    value_type: super::ValueType::String,
    allowed_values: &["image_prewarm", "sidecar_prewarm"],
};
// registry-type: enum
pub const LAUNCH_STAGE_NAME: &str = "launch.stage.name";
pub const LAUNCH_STAGE_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: LAUNCH_STAGE_NAME,
    description: "Launch pipeline stage.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "identity",
        "role",
        "credentials",
        "construct",
        "agent_binaries",
        "derived_image",
        "workspace",
        "network",
        "sidecar",
        "capsule",
        "hardline",
    ],
};
// registry-type: enum
pub const LAUNCH_TARGET_KIND: &str = "launch.target.kind";
pub const LAUNCH_TARGET_KIND_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: LAUNCH_TARGET_KIND,
    description: "Launch target class.",
    value_type: super::ValueType::String,
    allowed_values: &["workspace", "directory"],
};
// registry-type: enum
pub const NETWORK_MODE: &str = "network.mode";
pub const NETWORK_MODE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: NETWORK_MODE,
    description: "Network policy mode.",
    value_type: super::ValueType::String,
    allowed_values: &["none", "allowlist", "open"],
};
// registry-type: enum
pub const OUTCOME: &str = "outcome";
pub const OUTCOME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: OUTCOME,
    description: "Bounded operation outcome.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "success",
        "failure",
        "error",
        "timeout",
        "skip",
        "cancellation",
    ],
};
// registry-type: enum
pub const PROCESS_EXECUTABLE_NAME: &str = "process.executable.name";
pub const PROCESS_EXECUTABLE_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: PROCESS_EXECUTABLE_NAME,
    description: "Bounded process executable classification without paths or arguments.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "jackin",
        "jackin-daemon",
        "jackin-capsule",
        "jackin-role",
        "git",
        "gh",
        "op",
        "docker",
        "container",
        "mise",
        "ps",
        "osascript",
        "sh",
        "caffeinate",
        "kill",
        "less",
        "more",
        "bat",
        "claude",
        "codex",
        "amp",
        "kimi",
        "opencode",
        "grok",
        "configured_command",
        "other",
    ],
};
// registry-type: int
pub const PROCESS_EXIT_CODE: &str = "process.exit.code";
pub const PROCESS_EXIT_CODE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: PROCESS_EXIT_CODE,
    description: "Process exit code.",
    value_type: super::ValueType::Integer,
    allowed_values: &[],
};
// registry-type: enum
pub const PTY_EXIT_REASON: &str = "pty.exit.reason";
pub const PTY_EXIT_REASON_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: PTY_EXIT_REASON,
    description: "PTY exit reason.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "clean",
        "signal",
        "nonzero_exit",
        "wait_failed",
        "cancelled",
    ],
};
// registry-type: enum
pub const RPC_METHOD: &str = "rpc.method";
pub const RPC_METHOD_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: RPC_METHOD,
    description: "Fully-qualified logical RPC method name.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "jackin.capsule.Control/Status",
        "jackin.capsule.Control/TelemetryHealth",
        "jackin.capsule.Control/Snapshot",
        "jackin.capsule.Control/Agents",
        "jackin.capsule.Control/ReportRuntimeEvent",
        "jackin.capsule.Control/StatusCapture",
        "jackin.capsule.Control/UsageFocused",
        "jackin.capsule.Control/UsageRefreshFocused",
        "jackin.capsule.Control/UsageAccountList",
        "jackin.capsule.Control/ExecCommand",
        "jackin.capsule.Control/TokenUsage",
        "jackin.capsule.Control/Unknown",
        "jackin.capsule.Attach/Handshake",
        "jackin.capsule.Attach/Detach",
        "jackin.capsule.Attach/Focus",
        "jackin.capsule.Attach/ClipboardImageTransfer",
        "jackin.host.Credentials/Resolve",
        "jackin.host.Daemon/Hello",
        "jackin.host.Daemon/Status",
        "jackin.host.Daemon/TelemetryHealth",
        "jackin.host.Daemon/AttentionSnapshot",
        "jackin.host.Daemon/Shutdown",
    ],
};
// registry-type: enum
pub const RPC_SYSTEM_NAME: &str = "rpc.system.name";
pub const RPC_SYSTEM_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: RPC_SYSTEM_NAME,
    description: "Identifies the remote system being called.",
    value_type: super::ValueType::String,
    allowed_values: &["jackin"],
};
// registry-type: string
pub const SESSION_PREVIOUS_ID: &str = "session.previous_id";
pub const SESSION_PREVIOUS_ID_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: SESSION_PREVIOUS_ID,
    description: "Previous session id when a session is reattached.",
    value_type: super::ValueType::String,
    allowed_values: &[],
};
// registry-type: enum
pub const STREAM_DIRECTION: &str = "stream.direction";
pub const STREAM_DIRECTION_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: STREAM_DIRECTION,
    description: "Byte stream direction.",
    value_type: super::ValueType::String,
    allowed_values: &["input", "output"],
};
// registry-type: enum
pub const TELEMETRY_REJECTION_REASON: &str = "telemetry.rejection.reason";
pub const TELEMETRY_REJECTION_REASON_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: TELEMETRY_REJECTION_REASON,
    description: "Facade rejection reason.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "unknown_name",
        "unknown_attribute",
        "invalid_value",
        "privacy",
        "cardinality",
        "size_limit",
    ],
};
// registry-type: enum
pub const TELEMETRY_SIGNAL: &str = "telemetry.signal";
pub const TELEMETRY_SIGNAL_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: TELEMETRY_SIGNAL,
    description: "OpenTelemetry signal.",
    value_type: super::ValueType::String,
    allowed_values: &["log", "trace", "metric"],
};
// registry-type: enum
pub const TRUST_DECISION: &str = "trust.decision";
pub const TRUST_DECISION_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: TRUST_DECISION,
    description: "Trust decision.",
    value_type: super::ValueType::String,
    allowed_values: &["granted", "revoked", "rejected"],
};
// registry-type: enum
pub const TRUST_SOURCE_TYPE: &str = "trust.source.type";
pub const TRUST_SOURCE_TYPE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: TRUST_SOURCE_TYPE,
    description: "Trust source class.",
    value_type: super::ValueType::String,
    allowed_values: &["builtin", "external"],
};
// registry-type: enum
pub const UI_ACTION_NAME: &str = "ui.action.name";
pub const UI_ACTION_NAME_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: UI_ACTION_NAME,
    description: "Completed semantic UI action.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "workspace.open",
        "workspace.save",
        "workspace.launch",
        "settings.open",
        "settings.save",
        "dialog.confirm",
        "dialog.cancel",
        "agent.select",
        "agent.spawn",
        "tab.switch",
        "tab.rename",
        "tab.close",
        "pane.split",
        "pane.focus",
        "pane.resize",
        "pane.zoom",
        "pane.clear",
        "pane.close",
        "usage.refresh",
        "session.detach",
        "file.export",
        "image.stage",
        "link.open",
        "app.exit_request",
        "screen.back",
        "workspace.create",
        "workspace.delete",
        "instance.purge",
    ],
};
// registry-type: int
pub const UI_NAVIGATION_SEQUENCE: &str = "ui.navigation.sequence";
pub const UI_NAVIGATION_SEQUENCE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: UI_NAVIGATION_SEQUENCE,
    description: "Monotonic navigation sequence.",
    value_type: super::ValueType::Integer,
    allowed_values: &[],
};
// registry-type: string
pub const UI_SCREEN_VISIT_ID: &str = "ui.screen.visit.id";
pub const UI_SCREEN_VISIT_ID_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: UI_SCREEN_VISIT_ID,
    description: "Opaque screen visit identifier.",
    value_type: super::ValueType::String,
    allowed_values: &[],
};
// registry-type: enum
pub const UI_TRANSITION_REASON: &str = "ui.transition.reason";
pub const UI_TRANSITION_REASON_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: UI_TRANSITION_REASON,
    description: "Screen transition reason.",
    value_type: super::ValueType::String,
    allowed_values: &[
        "action",
        "launch",
        "attach",
        "detach",
        "back",
        "cancel",
        "completion",
        "failure",
        "shutdown",
    ],
};
// registry-type: enum
pub const WORKSPACE_ISOLATION_MODE: &str = "workspace.isolation.mode";
pub const WORKSPACE_ISOLATION_MODE_DEF: super::AttributeMetadata = super::AttributeMetadata {
    name: WORKSPACE_ISOLATION_MODE,
    description: "Workspace isolation mode.",
    value_type: super::ValueType::String,
    allowed_values: &["shared", "worktree", "clone"],
};

pub const ALL_KEYS: &[&str] = &[
    AGENT_STATE,
    AGENT_STATUS_CONFIDENCE,
    AGENT_STATUS_SOURCE,
    AGENT_STATUS_STUCK,
    APP_MODE,
    APP_SCREEN_ID,
    APP_SCREEN_NAME,
    APP_WIDGET_ID,
    APP_WIDGET_NAME,
    AUTH_MODE,
    BACKGROUND_CYCLE_NAME,
    CACHE_NAME,
    CACHE_RESULT,
    CLI_COMMAND_NAME,
    CLI_INVOCATION_ID,
    CONFIG_MIGRATION_STEP_COUNT,
    CONFIG_OPERATION,
    CONFIG_SCHEMA_VERSION_FROM,
    CONFIG_SCHEMA_VERSION_TO,
    CONFIG_SCOPE,
    CONNECTION_PEER_TYPE,
    CREDENTIAL_SOURCE_TYPE,
    DB_OPERATION_NAME,
    DIND_MODE,
    GEN_AI_AGENT_NAME,
    GEN_AI_CONVERSATION_ID,
    GEN_AI_OPERATION_NAME,
    GEN_AI_PROVIDER_NAME,
    GEN_AI_TOKEN_TYPE,
    JOB_ID,
    JOB_TYPE,
    LAUNCH_STAGE_NAME,
    LAUNCH_TARGET_KIND,
    NETWORK_MODE,
    OUTCOME,
    PROCESS_EXECUTABLE_NAME,
    PROCESS_EXIT_CODE,
    PTY_EXIT_REASON,
    RPC_METHOD,
    RPC_SYSTEM_NAME,
    SESSION_PREVIOUS_ID,
    STREAM_DIRECTION,
    TELEMETRY_REJECTION_REASON,
    TELEMETRY_SIGNAL,
    TRUST_DECISION,
    TRUST_SOURCE_TYPE,
    UI_ACTION_NAME,
    UI_NAVIGATION_SEQUENCE,
    UI_SCREEN_VISIT_ID,
    UI_TRANSITION_REASON,
    WORKSPACE_ISOLATION_MODE,
];

pub const ALL_DEFINITIONS: &[super::AttributeMetadata] = &[
    AGENT_STATE_DEF,
    AGENT_STATUS_CONFIDENCE_DEF,
    AGENT_STATUS_SOURCE_DEF,
    AGENT_STATUS_STUCK_DEF,
    APP_MODE_DEF,
    APP_SCREEN_ID_DEF,
    APP_SCREEN_NAME_DEF,
    APP_WIDGET_ID_DEF,
    APP_WIDGET_NAME_DEF,
    AUTH_MODE_DEF,
    BACKGROUND_CYCLE_NAME_DEF,
    CACHE_NAME_DEF,
    CACHE_RESULT_DEF,
    CLI_COMMAND_NAME_DEF,
    CLI_INVOCATION_ID_DEF,
    CONFIG_MIGRATION_STEP_COUNT_DEF,
    CONFIG_OPERATION_DEF,
    CONFIG_SCHEMA_VERSION_FROM_DEF,
    CONFIG_SCHEMA_VERSION_TO_DEF,
    CONFIG_SCOPE_DEF,
    CONNECTION_PEER_TYPE_DEF,
    CREDENTIAL_SOURCE_TYPE_DEF,
    DB_OPERATION_NAME_DEF,
    DIND_MODE_DEF,
    GEN_AI_AGENT_NAME_DEF,
    GEN_AI_CONVERSATION_ID_DEF,
    GEN_AI_OPERATION_NAME_DEF,
    GEN_AI_PROVIDER_NAME_DEF,
    GEN_AI_TOKEN_TYPE_DEF,
    JOB_ID_DEF,
    JOB_TYPE_DEF,
    LAUNCH_STAGE_NAME_DEF,
    LAUNCH_TARGET_KIND_DEF,
    NETWORK_MODE_DEF,
    OUTCOME_DEF,
    PROCESS_EXECUTABLE_NAME_DEF,
    PROCESS_EXIT_CODE_DEF,
    PTY_EXIT_REASON_DEF,
    RPC_METHOD_DEF,
    RPC_SYSTEM_NAME_DEF,
    SESSION_PREVIOUS_ID_DEF,
    STREAM_DIRECTION_DEF,
    TELEMETRY_REJECTION_REASON_DEF,
    TELEMETRY_SIGNAL_DEF,
    TRUST_DECISION_DEF,
    TRUST_SOURCE_TYPE_DEF,
    UI_ACTION_NAME_DEF,
    UI_NAVIGATION_SEQUENCE_DEF,
    UI_SCREEN_VISIT_ID_DEF,
    UI_TRANSITION_REASON_DEF,
    WORKSPACE_ISOLATION_MODE_DEF,
];

pub fn definition(name: &str) -> Option<&'static super::AttributeMetadata> {
    ALL_DEFINITIONS
        .iter()
        .find(|definition| definition.name == name)
}

/// Standard semantic-convention keys isolated behind a stable facade.
pub mod std_attrs {
    pub use opentelemetry_semantic_conventions::attribute::APP_BUILD_ID;
    pub use opentelemetry_semantic_conventions::attribute::APP_CRASH_ID;
    pub use opentelemetry_semantic_conventions::attribute::APP_JANK_FRAME_COUNT;
    pub use opentelemetry_semantic_conventions::attribute::APP_JANK_PERIOD;
    pub use opentelemetry_semantic_conventions::attribute::APP_JANK_THRESHOLD;
    pub use opentelemetry_semantic_conventions::attribute::APP_SCREEN_ID;
    pub use opentelemetry_semantic_conventions::attribute::APP_SCREEN_NAME;
    pub use opentelemetry_semantic_conventions::attribute::APP_WIDGET_ID;
    pub use opentelemetry_semantic_conventions::attribute::APP_WIDGET_NAME;
    pub use opentelemetry_semantic_conventions::attribute::CODE_FILE_PATH;
    pub use opentelemetry_semantic_conventions::attribute::CODE_FUNCTION_NAME;
    pub use opentelemetry_semantic_conventions::attribute::CODE_LINE_NUMBER;
    pub use opentelemetry_semantic_conventions::attribute::CONTAINER_ID;
    pub use opentelemetry_semantic_conventions::attribute::CPU_MODE;
    pub use opentelemetry_semantic_conventions::attribute::DB_OPERATION_NAME;
    pub use opentelemetry_semantic_conventions::attribute::DB_SYSTEM_NAME;
    pub use opentelemetry_semantic_conventions::attribute::ERROR_TYPE;
    pub use opentelemetry_semantic_conventions::attribute::EXCEPTION_MESSAGE;
    pub use opentelemetry_semantic_conventions::attribute::EXCEPTION_STACKTRACE;
    pub use opentelemetry_semantic_conventions::attribute::EXCEPTION_TYPE;
    pub use opentelemetry_semantic_conventions::attribute::HTTP_REQUEST_METHOD;
    pub use opentelemetry_semantic_conventions::attribute::NETWORK_TRANSPORT;
    pub use opentelemetry_semantic_conventions::attribute::NETWORK_TYPE;
    pub use opentelemetry_semantic_conventions::attribute::OS_NAME;
    pub use opentelemetry_semantic_conventions::attribute::OS_TYPE;
    pub use opentelemetry_semantic_conventions::attribute::OS_VERSION;
    pub use opentelemetry_semantic_conventions::attribute::PROCESS_COMMAND;
    pub use opentelemetry_semantic_conventions::attribute::PROCESS_EXECUTABLE_NAME;
    pub use opentelemetry_semantic_conventions::attribute::PROCESS_EXIT_CODE;
    pub use opentelemetry_semantic_conventions::attribute::PROCESS_PID;
    pub use opentelemetry_semantic_conventions::attribute::PROCESS_RUNTIME_NAME;
    pub use opentelemetry_semantic_conventions::attribute::PROCESS_RUNTIME_VERSION;
    pub use opentelemetry_semantic_conventions::attribute::RPC_METHOD;
    pub use opentelemetry_semantic_conventions::attribute::RPC_SYSTEM_NAME;
    pub use opentelemetry_semantic_conventions::attribute::SERVER_ADDRESS;
    pub use opentelemetry_semantic_conventions::attribute::SERVICE_INSTANCE_ID;
    pub use opentelemetry_semantic_conventions::attribute::SERVICE_NAME;
    pub use opentelemetry_semantic_conventions::attribute::SERVICE_NAMESPACE;
    pub use opentelemetry_semantic_conventions::attribute::SERVICE_VERSION;
    pub use opentelemetry_semantic_conventions::attribute::SESSION_ID;
    pub use opentelemetry_semantic_conventions::attribute::SESSION_PREVIOUS_ID;
    pub use opentelemetry_semantic_conventions::attribute::URL_TEMPLATE;
    // Local pin: not authoritative in opentelemetry-semantic-conventions =0.32.1; registry schema 1.43.0.
    pub const GEN_AI_AGENT_NAME: &str = "gen_ai.agent.name";
    // Local pin: not authoritative in opentelemetry-semantic-conventions =0.32.1; registry schema 1.43.0.
    pub const GEN_AI_CONVERSATION_ID: &str = "gen_ai.conversation.id";
    // Local pin: not authoritative in opentelemetry-semantic-conventions =0.32.1; registry schema 1.43.0.
    pub const GEN_AI_OPERATION_NAME: &str = "gen_ai.operation.name";
    // Local pin: not authoritative in opentelemetry-semantic-conventions =0.32.1; registry schema 1.43.0.
    pub const GEN_AI_PROVIDER_NAME: &str = "gen_ai.provider.name";
    // Local pin: not authoritative in opentelemetry-semantic-conventions =0.32.1; registry schema 1.43.0.
    pub const GEN_AI_TOKEN_TYPE: &str = "gen_ai.token.type";
    pub const ALL_KEYS: &[&str] = &[
        APP_BUILD_ID,
        APP_CRASH_ID,
        APP_JANK_FRAME_COUNT,
        APP_JANK_PERIOD,
        APP_JANK_THRESHOLD,
        APP_SCREEN_ID,
        APP_SCREEN_NAME,
        APP_WIDGET_ID,
        APP_WIDGET_NAME,
        CODE_FILE_PATH,
        CODE_FUNCTION_NAME,
        CODE_LINE_NUMBER,
        CONTAINER_ID,
        CPU_MODE,
        DB_OPERATION_NAME,
        DB_SYSTEM_NAME,
        ERROR_TYPE,
        EXCEPTION_MESSAGE,
        EXCEPTION_STACKTRACE,
        EXCEPTION_TYPE,
        HTTP_REQUEST_METHOD,
        NETWORK_TRANSPORT,
        NETWORK_TYPE,
        OS_NAME,
        OS_TYPE,
        OS_VERSION,
        PROCESS_COMMAND,
        PROCESS_EXECUTABLE_NAME,
        PROCESS_EXIT_CODE,
        PROCESS_PID,
        PROCESS_RUNTIME_NAME,
        PROCESS_RUNTIME_VERSION,
        RPC_METHOD,
        RPC_SYSTEM_NAME,
        SERVER_ADDRESS,
        SERVICE_INSTANCE_ID,
        SERVICE_NAME,
        SERVICE_NAMESPACE,
        SERVICE_VERSION,
        SESSION_ID,
        SESSION_PREVIOUS_ID,
        URL_TEMPLATE,
        GEN_AI_AGENT_NAME,
        GEN_AI_CONVERSATION_ID,
        GEN_AI_OPERATION_NAME,
        GEN_AI_PROVIDER_NAME,
        GEN_AI_TOKEN_TYPE,
    ];
    pub const UPSTREAM_ALIASES: &[(&str, &str)] = &[
        (APP_BUILD_ID, "app.build_id"),
        (APP_CRASH_ID, "app.crash.id"),
        (APP_JANK_FRAME_COUNT, "app.jank.frame_count"),
        (APP_JANK_PERIOD, "app.jank.period"),
        (APP_JANK_THRESHOLD, "app.jank.threshold"),
        (APP_SCREEN_ID, "app.screen.id"),
        (APP_SCREEN_NAME, "app.screen.name"),
        (APP_WIDGET_ID, "app.widget.id"),
        (APP_WIDGET_NAME, "app.widget.name"),
        (CODE_FILE_PATH, "code.file.path"),
        (CODE_FUNCTION_NAME, "code.function.name"),
        (CODE_LINE_NUMBER, "code.line.number"),
        (CONTAINER_ID, "container.id"),
        (CPU_MODE, "cpu.mode"),
        (DB_OPERATION_NAME, "db.operation.name"),
        (DB_SYSTEM_NAME, "db.system.name"),
        (ERROR_TYPE, "error.type"),
        (EXCEPTION_MESSAGE, "exception.message"),
        (EXCEPTION_STACKTRACE, "exception.stacktrace"),
        (EXCEPTION_TYPE, "exception.type"),
        (HTTP_REQUEST_METHOD, "http.request.method"),
        (NETWORK_TRANSPORT, "network.transport"),
        (NETWORK_TYPE, "network.type"),
        (OS_NAME, "os.name"),
        (OS_TYPE, "os.type"),
        (OS_VERSION, "os.version"),
        (PROCESS_COMMAND, "process.command"),
        (PROCESS_EXECUTABLE_NAME, "process.executable.name"),
        (PROCESS_EXIT_CODE, "process.exit.code"),
        (PROCESS_PID, "process.pid"),
        (PROCESS_RUNTIME_NAME, "process.runtime.name"),
        (PROCESS_RUNTIME_VERSION, "process.runtime.version"),
        (RPC_METHOD, "rpc.method"),
        (RPC_SYSTEM_NAME, "rpc.system.name"),
        (SERVER_ADDRESS, "server.address"),
        (SERVICE_INSTANCE_ID, "service.instance.id"),
        (SERVICE_NAME, "service.name"),
        (SERVICE_NAMESPACE, "service.namespace"),
        (SERVICE_VERSION, "service.version"),
        (SESSION_ID, "session.id"),
        (SESSION_PREVIOUS_ID, "session.previous_id"),
        (URL_TEMPLATE, "url.template"),
    ];
    pub const RUST_CRATE_SCHEMA_URL: &str = "https://opentelemetry.io/schemas/1.42.0";
}

// Compatibility re-exports; new code uses `std_attrs`.
pub use std_attrs::{APP_JANK_FRAME_COUNT, APP_JANK_PERIOD, APP_JANK_THRESHOLD};

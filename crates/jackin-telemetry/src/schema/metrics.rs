// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

// registry: instrument=counter; unit={event}; attributes=agent.state:required,agent.status.confidence:required,agent.status.source:required,gen_ai.agent.name:required
pub const AGENT_STATE_FLAPS: &str = "agent.state.flaps";
pub const AGENT_STATE_FLAPS_DEF: super::MetricMetadata = super::MetricMetadata {
    name: AGENT_STATE_FLAPS,
    description: "Agent state flap detections.",
    instrument: super::MetricInstrument::Counter,
    unit: "{event}",
    boundaries: &[],
    attributes: &[
        super::AttributeRequirement {
            name: "agent.state",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["working", "blocked", "done", "idle", "unknown"],
        },
        super::AttributeRequirement {
            name: "agent.status.confidence",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["unknown", "weak", "strong", "authoritative"],
        },
        super::AttributeRequirement {
            name: "agent.status.source",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "none",
                "visible_screen",
                "shell_integration",
                "foreground_process",
                "reported",
            ],
        },
        super::AttributeRequirement {
            name: "gen_ai.agent.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["claude", "codex", "amp", "kimi", "opencode", "grok"],
        },
    ],
};
// registry: instrument=counter; unit={event}; attributes=agent.state:required,agent.status.confidence:required,agent.status.source:required,gen_ai.agent.name:required
pub const AGENT_STATE_STUCK: &str = "agent.state.stuck";
pub const AGENT_STATE_STUCK_DEF: super::MetricMetadata = super::MetricMetadata {
    name: AGENT_STATE_STUCK,
    description: "Agent stuck-state detections.",
    instrument: super::MetricInstrument::Counter,
    unit: "{event}",
    boundaries: &[],
    attributes: &[
        super::AttributeRequirement {
            name: "agent.state",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["working", "blocked", "done", "idle", "unknown"],
        },
        super::AttributeRequirement {
            name: "agent.status.confidence",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["unknown", "weak", "strong", "authoritative"],
        },
        super::AttributeRequirement {
            name: "agent.status.source",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "none",
                "visible_screen",
                "shell_integration",
                "foreground_process",
                "reported",
            ],
        },
        super::AttributeRequirement {
            name: "gen_ai.agent.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["claude", "codex", "amp", "kimi", "opencode", "grok"],
        },
    ],
};
// registry: instrument=counter; unit={transition}; attributes=agent.state:required,agent.status.confidence:required,agent.status.source:required,gen_ai.agent.name:required
pub const AGENT_STATE_TRANSITIONS: &str = "agent.state.transitions";
pub const AGENT_STATE_TRANSITIONS_DEF: super::MetricMetadata = super::MetricMetadata {
    name: AGENT_STATE_TRANSITIONS,
    description: "Effective agent state transitions.",
    instrument: super::MetricInstrument::Counter,
    unit: "{transition}",
    boundaries: &[],
    attributes: &[
        super::AttributeRequirement {
            name: "agent.state",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["working", "blocked", "done", "idle", "unknown"],
        },
        super::AttributeRequirement {
            name: "agent.status.confidence",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["unknown", "weak", "strong", "authoritative"],
        },
        super::AttributeRequirement {
            name: "agent.status.source",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "none",
                "visible_screen",
                "shell_integration",
                "foreground_process",
                "reported",
            ],
        },
        super::AttributeRequirement {
            name: "gen_ai.agent.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["claude", "codex", "amp", "kimi", "opencode", "grok"],
        },
    ],
};
// registry: instrument=histogram; unit=s; attributes=
pub const BACKGROUND_CYCLE_DURATION: &str = "background.cycle.duration";
pub const BACKGROUND_CYCLE_DURATION_DEF: super::MetricMetadata = super::MetricMetadata {
    name: BACKGROUND_CYCLE_DURATION,
    description: "Background cycle duration.",
    instrument: super::MetricInstrument::Histogram,
    unit: "s",
    boundaries: &[
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ],
    attributes: &[],
};
// registry: instrument=counter; unit={cycle}; attributes=
pub const BACKGROUND_CYCLES: &str = "background.cycles";
pub const BACKGROUND_CYCLES_DEF: super::MetricMetadata = super::MetricMetadata {
    name: BACKGROUND_CYCLES,
    description: "Background cycles started.",
    instrument: super::MetricInstrument::Counter,
    unit: "{cycle}",
    boundaries: &[],
    attributes: &[],
};
// registry: instrument=counter; unit={decision}; attributes=cache.name:required,cache.result:required
pub const CACHE_DECISIONS: &str = "cache.decisions";
pub const CACHE_DECISIONS_DEF: super::MetricMetadata = super::MetricMetadata {
    name: CACHE_DECISIONS,
    description: "Product cache decisions.",
    instrument: super::MetricInstrument::Counter,
    unit: "{decision}",
    boundaries: &[],
    attributes: &[
        super::AttributeRequirement {
            name: "cache.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "role_repository",
                "agent_binary",
                "capsule_binary",
                "derived_image",
                "usage_snapshot",
            ],
        },
        super::AttributeRequirement {
            name: "cache.result",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["hit", "miss", "stale", "reuse", "bypass"],
        },
    ],
};
// registry: instrument=histogram; unit=s; attributes=cli.command.name:required,error.type:recommended,outcome:required
pub const CLI_DURATION: &str = "cli.duration";
pub const CLI_DURATION_DEF: super::MetricMetadata = super::MetricMetadata {
    name: CLI_DURATION,
    description: "CLI invocation duration.",
    instrument: super::MetricInstrument::Histogram,
    unit: "s",
    boundaries: &[
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ],
    attributes: &[
        super::AttributeRequirement {
            name: "cli.command.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
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
        },
        super::AttributeRequirement {
            name: "error.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "success",
                "failure",
                "error",
                "timeout",
                "skip",
                "cancellation",
            ],
        },
    ],
};
// registry: instrument=counter; unit={failure}; attributes=cli.command.name:required,error.type:recommended,outcome:required
pub const CLI_FAILURES: &str = "cli.failures";
pub const CLI_FAILURES_DEF: super::MetricMetadata = super::MetricMetadata {
    name: CLI_FAILURES,
    description: "CLI invocations ending in failure.",
    instrument: super::MetricInstrument::Counter,
    unit: "{failure}",
    boundaries: &[],
    attributes: &[
        super::AttributeRequirement {
            name: "cli.command.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
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
        },
        super::AttributeRequirement {
            name: "error.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "success",
                "failure",
                "error",
                "timeout",
                "skip",
                "cancellation",
            ],
        },
    ],
};
// registry: instrument=counter; unit={invocation}; attributes=cli.command.name:required,error.type:recommended,outcome:required
pub const CLI_INVOCATIONS: &str = "cli.invocations";
pub const CLI_INVOCATIONS_DEF: super::MetricMetadata = super::MetricMetadata {
    name: CLI_INVOCATIONS,
    description: "CLI invocations completed.",
    instrument: super::MetricInstrument::Counter,
    unit: "{invocation}",
    boundaries: &[],
    attributes: &[
        super::AttributeRequirement {
            name: "cli.command.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
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
        },
        super::AttributeRequirement {
            name: "error.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "success",
                "failure",
                "error",
                "timeout",
                "skip",
                "cancellation",
            ],
        },
    ],
};
// registry: instrument=updowncounter; unit={connection}; attributes=connection.peer.type:required
pub const CONNECTION_ACTIVE: &str = "connection.active";
pub const CONNECTION_ACTIVE_DEF: super::MetricMetadata = super::MetricMetadata {
    name: CONNECTION_ACTIVE,
    description: "Active connection attempts.",
    instrument: super::MetricInstrument::UpDownCounter,
    unit: "{connection}",
    boundaries: &[],
    attributes: &[super::AttributeRequirement {
        name: "connection.peer.type",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Required,
        allowed_values: &[
            "host_daemon",
            "capsule_control",
            "capsule_attach",
            "docker",
            "provider",
            "parallax",
        ],
    }],
};
// registry: instrument=counter; unit={attempt}; attributes=connection.peer.type:required,error.type:recommended,outcome:required
pub const CONNECTION_ATTEMPTS: &str = "connection.attempts";
pub const CONNECTION_ATTEMPTS_DEF: super::MetricMetadata = super::MetricMetadata {
    name: CONNECTION_ATTEMPTS,
    description: "Connection attempts.",
    instrument: super::MetricInstrument::Counter,
    unit: "{attempt}",
    boundaries: &[],
    attributes: &[
        super::AttributeRequirement {
            name: "connection.peer.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "host_daemon",
                "capsule_control",
                "capsule_attach",
                "docker",
                "provider",
                "parallax",
            ],
        },
        super::AttributeRequirement {
            name: "error.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "success",
                "failure",
                "error",
                "timeout",
                "skip",
                "cancellation",
            ],
        },
    ],
};
// registry: instrument=histogram; unit=s; attributes=connection.peer.type:required,error.type:recommended,outcome:required
pub const CONNECTION_DURATION: &str = "connection.duration";
pub const CONNECTION_DURATION_DEF: super::MetricMetadata = super::MetricMetadata {
    name: CONNECTION_DURATION,
    description: "Connection attempt duration.",
    instrument: super::MetricInstrument::Histogram,
    unit: "s",
    boundaries: &[
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ],
    attributes: &[
        super::AttributeRequirement {
            name: "connection.peer.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "host_daemon",
                "capsule_control",
                "capsule_attach",
                "docker",
                "provider",
                "parallax",
            ],
        },
        super::AttributeRequirement {
            name: "error.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "success",
                "failure",
                "error",
                "timeout",
                "skip",
                "cancellation",
            ],
        },
    ],
};
// registry: instrument=histogram; unit=s; attributes=db.operation.name:required
pub const DB_CLIENT_OPERATION_DURATION: &str = "db.client.operation.duration";
pub const DB_CLIENT_OPERATION_DURATION_DEF: super::MetricMetadata = super::MetricMetadata {
    name: DB_CLIENT_OPERATION_DURATION,
    description: "Database client operation duration.",
    instrument: super::MetricInstrument::Histogram,
    unit: "s",
    boundaries: &[
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ],
    attributes: &[super::AttributeRequirement {
        name: "db.operation.name",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Required,
        allowed_values: &[],
    }],
};
// registry: instrument=updowncounter; unit={execution}; attributes=launch.stage.name:required,launch.target.kind:required
pub const LAUNCH_STAGE_ACTIVE: &str = "launch.stage.active";
pub const LAUNCH_STAGE_ACTIVE_DEF: super::MetricMetadata = super::MetricMetadata {
    name: LAUNCH_STAGE_ACTIVE,
    description: "Active launch stage executions.",
    instrument: super::MetricInstrument::UpDownCounter,
    unit: "{execution}",
    boundaries: &[],
    attributes: &[
        super::AttributeRequirement {
            name: "launch.stage.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
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
        },
        super::AttributeRequirement {
            name: "launch.target.kind",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["workspace", "directory"],
        },
    ],
};
// registry: instrument=histogram; unit=s; attributes=error.type:recommended,launch.stage.name:required,launch.target.kind:required,outcome:required
pub const LAUNCH_STAGE_DURATION: &str = "launch.stage.duration";
pub const LAUNCH_STAGE_DURATION_DEF: super::MetricMetadata = super::MetricMetadata {
    name: LAUNCH_STAGE_DURATION,
    description: "Launch stage duration.",
    instrument: super::MetricInstrument::Histogram,
    unit: "s",
    boundaries: &[
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ],
    attributes: &[
        super::AttributeRequirement {
            name: "error.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "launch.stage.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
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
        },
        super::AttributeRequirement {
            name: "launch.target.kind",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["workspace", "directory"],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "success",
                "failure",
                "error",
                "timeout",
                "skip",
                "cancellation",
            ],
        },
    ],
};
// registry: instrument=counter; unit={execution}; attributes=error.type:recommended,launch.stage.name:required,launch.target.kind:required,outcome:required
pub const LAUNCH_STAGE_EXECUTIONS: &str = "launch.stage.executions";
pub const LAUNCH_STAGE_EXECUTIONS_DEF: super::MetricMetadata = super::MetricMetadata {
    name: LAUNCH_STAGE_EXECUTIONS,
    description: "Launch stage executions.",
    instrument: super::MetricInstrument::Counter,
    unit: "{execution}",
    boundaries: &[],
    attributes: &[
        super::AttributeRequirement {
            name: "error.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "launch.stage.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
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
        },
        super::AttributeRequirement {
            name: "launch.target.kind",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["workspace", "directory"],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "success",
                "failure",
                "error",
                "timeout",
                "skip",
                "cancellation",
            ],
        },
    ],
};
// registry: instrument=updowncounter; unit={job}; attributes=
pub const PREWARM_ACTIVE: &str = "prewarm.active";
pub const PREWARM_ACTIVE_DEF: super::MetricMetadata = super::MetricMetadata {
    name: PREWARM_ACTIVE,
    description: "Active detached prewarm jobs.",
    instrument: super::MetricInstrument::UpDownCounter,
    unit: "{job}",
    boundaries: &[],
    attributes: &[],
};
// registry: instrument=histogram; unit=s; attributes=
pub const PREWARM_DURATION: &str = "prewarm.duration";
pub const PREWARM_DURATION_DEF: super::MetricMetadata = super::MetricMetadata {
    name: PREWARM_DURATION,
    description: "Detached prewarm job duration.",
    instrument: super::MetricInstrument::Histogram,
    unit: "s",
    boundaries: &[
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ],
    attributes: &[],
};
// registry: instrument=counter; unit={job}; attributes=
pub const PREWARM_JOBS: &str = "prewarm.jobs";
pub const PREWARM_JOBS_DEF: super::MetricMetadata = super::MetricMetadata {
    name: PREWARM_JOBS,
    description: "Detached prewarm jobs started.",
    instrument: super::MetricInstrument::Counter,
    unit: "{job}",
    boundaries: &[],
    attributes: &[],
};
// registry: instrument=updowncounter; unit={request}; attributes=rpc.method:required
pub const RPC_ACTIVE: &str = "rpc.active";
pub const RPC_ACTIVE_DEF: super::MetricMetadata = super::MetricMetadata {
    name: RPC_ACTIVE,
    description: "Active RPC requests.",
    instrument: super::MetricInstrument::UpDownCounter,
    unit: "{request}",
    boundaries: &[],
    attributes: &[super::AttributeRequirement {
        name: "rpc.method",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Required,
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
    }],
};
// registry: instrument=histogram; unit=s; attributes=error.type:recommended,outcome:required,rpc.method:required
pub const RPC_DURATION: &str = "rpc.duration";
pub const RPC_DURATION_DEF: super::MetricMetadata = super::MetricMetadata {
    name: RPC_DURATION,
    description: "RPC request duration.",
    instrument: super::MetricInstrument::Histogram,
    unit: "s",
    boundaries: &[
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ],
    attributes: &[
        super::AttributeRequirement {
            name: "error.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "success",
                "failure",
                "error",
                "timeout",
                "skip",
                "cancellation",
            ],
        },
        super::AttributeRequirement {
            name: "rpc.method",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
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
        },
    ],
};
// registry: instrument=counter; unit={request}; attributes=error.type:recommended,outcome:required,rpc.method:required
pub const RPC_REQUESTS: &str = "rpc.requests";
pub const RPC_REQUESTS_DEF: super::MetricMetadata = super::MetricMetadata {
    name: RPC_REQUESTS,
    description: "RPC requests.",
    instrument: super::MetricInstrument::Counter,
    unit: "{request}",
    boundaries: &[],
    attributes: &[
        super::AttributeRequirement {
            name: "error.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "success",
                "failure",
                "error",
                "timeout",
                "skip",
                "cancellation",
            ],
        },
        super::AttributeRequirement {
            name: "rpc.method",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
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
        },
    ],
};
// registry: instrument=counter; unit={rejection}; attributes=telemetry.rejection.reason:required,telemetry.signal:required
pub const TELEMETRY_REJECTIONS: &str = "telemetry.rejections";
pub const TELEMETRY_REJECTIONS_DEF: super::MetricMetadata = super::MetricMetadata {
    name: TELEMETRY_REJECTIONS,
    description: "Governed facade rejections.",
    instrument: super::MetricInstrument::Counter,
    unit: "{rejection}",
    boundaries: &[],
    attributes: &[
        super::AttributeRequirement {
            name: "telemetry.rejection.reason",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "unknown_name",
                "unknown_attribute",
                "invalid_value",
                "privacy",
                "cardinality",
                "size_limit",
            ],
        },
        super::AttributeRequirement {
            name: "telemetry.signal",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["log", "trace", "metric"],
        },
    ],
};
// registry: instrument=counter; unit={validation}; attributes=
pub const TELEMETRY_VALIDATE: &str = "telemetry.validate";
pub const TELEMETRY_VALIDATE_DEF: super::MetricMetadata = super::MetricMetadata {
    name: TELEMETRY_VALIDATE,
    description: "Telemetry delivery validation markers.",
    instrument: super::MetricInstrument::Counter,
    unit: "{validation}",
    boundaries: &[],
    attributes: &[],
};
// registry: instrument=counter; unit={move}; attributes=
pub const TERMINAL_CURSOR_MOVES: &str = "terminal.cursor.moves";
pub const TERMINAL_CURSOR_MOVES_DEF: super::MetricMetadata = super::MetricMetadata {
    name: TERMINAL_CURSOR_MOVES,
    description: "Terminal cursor moves.",
    instrument: super::MetricInstrument::Counter,
    unit: "{move}",
    boundaries: &[],
    attributes: &[],
};
// registry: instrument=counter; unit={event}; attributes=
pub const TERMINAL_INPUT_MOUSE: &str = "terminal.input.mouse";
pub const TERMINAL_INPUT_MOUSE_DEF: super::MetricMetadata = super::MetricMetadata {
    name: TERMINAL_INPUT_MOUSE,
    description: "Semantic terminal mouse inputs.",
    instrument: super::MetricInstrument::Counter,
    unit: "{event}",
    boundaries: &[],
    attributes: &[],
};
// registry: instrument=counter; unit=By; attributes=stream.direction:required
pub const TERMINAL_IO_BYTES: &str = "terminal.io.bytes";
pub const TERMINAL_IO_BYTES_DEF: super::MetricMetadata = super::MetricMetadata {
    name: TERMINAL_IO_BYTES,
    description: "Terminal stream bytes.",
    instrument: super::MetricInstrument::Counter,
    unit: "By",
    boundaries: &[],
    attributes: &[super::AttributeRequirement {
        name: "stream.direction",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Required,
        allowed_values: &["input", "output"],
    }],
};
// registry: instrument=counter; unit={cell}; attributes=
pub const TERMINAL_RENDER_CELLS: &str = "terminal.render.cells";
pub const TERMINAL_RENDER_CELLS_DEF: super::MetricMetadata = super::MetricMetadata {
    name: TERMINAL_RENDER_CELLS,
    description: "Terminal cells painted.",
    instrument: super::MetricInstrument::Counter,
    unit: "{cell}",
    boundaries: &[],
    attributes: &[],
};
// registry: instrument=histogram; unit=s; attributes=
pub const TERMINAL_RENDER_DURATION: &str = "terminal.render.duration";
pub const TERMINAL_RENDER_DURATION_DEF: super::MetricMetadata = super::MetricMetadata {
    name: TERMINAL_RENDER_DURATION,
    description: "Terminal render duration.",
    instrument: super::MetricInstrument::Histogram,
    unit: "s",
    boundaries: &[
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ],
    attributes: &[],
};
// registry: instrument=counter; unit={frame}; attributes=
pub const TERMINAL_RENDER_FRAMES: &str = "terminal.render.frames";
pub const TERMINAL_RENDER_FRAMES_DEF: super::MetricMetadata = super::MetricMetadata {
    name: TERMINAL_RENDER_FRAMES,
    description: "Terminal render frames.",
    instrument: super::MetricInstrument::Counter,
    unit: "{frame}",
    boundaries: &[],
    attributes: &[],
};
// registry: instrument=counter; unit={action}; attributes=ui.action.name:required
pub const UI_ACTIONS: &str = "ui.actions";
pub const UI_ACTIONS_DEF: super::MetricMetadata = super::MetricMetadata {
    name: UI_ACTIONS,
    description: "Completed UI actions.",
    instrument: super::MetricInstrument::Counter,
    unit: "{action}",
    boundaries: &[],
    attributes: &[super::AttributeRequirement {
        name: "ui.action.name",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Required,
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
    }],
};
// registry: instrument=histogram; unit=s; attributes=app.widget.id:required
pub const UI_FOCUS_DURATION: &str = "ui.focus.duration";
pub const UI_FOCUS_DURATION_DEF: super::MetricMetadata = super::MetricMetadata {
    name: UI_FOCUS_DURATION,
    description: "UI widget focus duration.",
    instrument: super::MetricInstrument::Histogram,
    unit: "s",
    boundaries: &[
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ],
    attributes: &[super::AttributeRequirement {
        name: "app.widget.id",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Required,
        allowed_values: &[],
    }],
};
// registry: instrument=counter; unit={crossing}; attributes=app.screen.id:required
pub const UI_JANK: &str = "ui.jank";
pub const UI_JANK_DEF: super::MetricMetadata = super::MetricMetadata {
    name: UI_JANK,
    description: "UI render jank threshold crossings.",
    instrument: super::MetricInstrument::Counter,
    unit: "{crossing}",
    boundaries: &[],
    attributes: &[super::AttributeRequirement {
        name: "app.screen.id",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Required,
        allowed_values: &[
            "workspace.list",
            "workspace.editor",
            "settings",
            "workspace.create",
            "launch.progress",
            "capsule",
        ],
    }],
};
// registry: instrument=histogram; unit=s; attributes=app.screen.id:required
pub const UI_RENDER_DURATION: &str = "ui.render.duration";
pub const UI_RENDER_DURATION_DEF: super::MetricMetadata = super::MetricMetadata {
    name: UI_RENDER_DURATION,
    description: "UI render duration.",
    instrument: super::MetricInstrument::Histogram,
    unit: "s",
    boundaries: &[
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ],
    attributes: &[super::AttributeRequirement {
        name: "app.screen.id",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Required,
        allowed_values: &[
            "workspace.list",
            "workspace.editor",
            "settings",
            "workspace.create",
            "launch.progress",
            "capsule",
        ],
    }],
};
// registry: instrument=histogram; unit=s; attributes=app.screen.id:required,ui.transition.reason:required
pub const UI_SCREEN_DWELL: &str = "ui.screen.dwell";
pub const UI_SCREEN_DWELL_DEF: super::MetricMetadata = super::MetricMetadata {
    name: UI_SCREEN_DWELL,
    description: "UI screen dwell duration.",
    instrument: super::MetricInstrument::Histogram,
    unit: "s",
    boundaries: &[
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ],
    attributes: &[
        super::AttributeRequirement {
            name: "app.screen.id",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "workspace.list",
                "workspace.editor",
                "settings",
                "workspace.create",
                "launch.progress",
                "capsule",
            ],
        },
        super::AttributeRequirement {
            name: "ui.transition.reason",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
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
        },
    ],
};
// registry: instrument=counter; unit={transition}; attributes=app.screen.id:required,ui.transition.reason:required
pub const UI_TRANSITIONS: &str = "ui.transitions";
pub const UI_TRANSITIONS_DEF: super::MetricMetadata = super::MetricMetadata {
    name: UI_TRANSITIONS,
    description: "UI screen transitions.",
    instrument: super::MetricInstrument::Counter,
    unit: "{transition}",
    boundaries: &[],
    attributes: &[
        super::AttributeRequirement {
            name: "app.screen.id",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[
                "workspace.list",
                "workspace.editor",
                "settings",
                "workspace.create",
                "launch.progress",
                "capsule",
            ],
        },
        super::AttributeRequirement {
            name: "ui.transition.reason",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
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
        },
    ],
};

pub const ALL: &[&str] = &[
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
    LAUNCH_STAGE_ACTIVE,
    LAUNCH_STAGE_DURATION,
    LAUNCH_STAGE_EXECUTIONS,
    PREWARM_ACTIVE,
    PREWARM_DURATION,
    PREWARM_JOBS,
    RPC_ACTIVE,
    RPC_DURATION,
    RPC_REQUESTS,
    TELEMETRY_REJECTIONS,
    TELEMETRY_VALIDATE,
    TERMINAL_CURSOR_MOVES,
    TERMINAL_INPUT_MOUSE,
    TERMINAL_IO_BYTES,
    TERMINAL_RENDER_CELLS,
    TERMINAL_RENDER_DURATION,
    TERMINAL_RENDER_FRAMES,
    UI_ACTIONS,
    UI_FOCUS_DURATION,
    UI_JANK,
    UI_RENDER_DURATION,
    UI_SCREEN_DWELL,
    UI_TRANSITIONS,
];

pub const DEFINITIONS: &[super::MetricMetadata] = &[
    AGENT_STATE_FLAPS_DEF,
    AGENT_STATE_STUCK_DEF,
    AGENT_STATE_TRANSITIONS_DEF,
    BACKGROUND_CYCLE_DURATION_DEF,
    BACKGROUND_CYCLES_DEF,
    CACHE_DECISIONS_DEF,
    CLI_DURATION_DEF,
    CLI_FAILURES_DEF,
    CLI_INVOCATIONS_DEF,
    CONNECTION_ACTIVE_DEF,
    CONNECTION_ATTEMPTS_DEF,
    CONNECTION_DURATION_DEF,
    DB_CLIENT_OPERATION_DURATION_DEF,
    LAUNCH_STAGE_ACTIVE_DEF,
    LAUNCH_STAGE_DURATION_DEF,
    LAUNCH_STAGE_EXECUTIONS_DEF,
    PREWARM_ACTIVE_DEF,
    PREWARM_DURATION_DEF,
    PREWARM_JOBS_DEF,
    RPC_ACTIVE_DEF,
    RPC_DURATION_DEF,
    RPC_REQUESTS_DEF,
    TELEMETRY_REJECTIONS_DEF,
    TELEMETRY_VALIDATE_DEF,
    TERMINAL_CURSOR_MOVES_DEF,
    TERMINAL_INPUT_MOUSE_DEF,
    TERMINAL_IO_BYTES_DEF,
    TERMINAL_RENDER_CELLS_DEF,
    TERMINAL_RENDER_DURATION_DEF,
    TERMINAL_RENDER_FRAMES_DEF,
    UI_ACTIONS_DEF,
    UI_FOCUS_DURATION_DEF,
    UI_JANK_DEF,
    UI_RENDER_DURATION_DEF,
    UI_SCREEN_DWELL_DEF,
    UI_TRANSITIONS_DEF,
];

#[must_use]
pub fn definition(name: &str) -> Option<&'static super::MetricMetadata> {
    DEFINITIONS
        .iter()
        .find(|definition| definition.name == name)
}

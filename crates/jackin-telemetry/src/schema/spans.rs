// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

// registry: kind=internal; attributes=outcome:recommended
pub const APP_SHUTDOWN: &str = "app.shutdown";
pub const APP_SHUTDOWN_DEF: super::SpanMetadata = super::SpanMetadata {
    name: APP_SHUTDOWN,
    description: "Bounded application shutdown.",
    kind: super::SpanKind::Internal,
    attributes: &[super::AttributeRequirement {
        name: "outcome",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Recommended,
        allowed_values: &[
            "success",
            "failure",
            "error",
            "timeout",
            "skip",
            "cancellation",
        ],
    }],
};
// registry: kind=internal; attributes=outcome:recommended
pub const APP_STARTUP: &str = "app.startup";
pub const APP_STARTUP_DEF: super::SpanMetadata = super::SpanMetadata {
    name: APP_STARTUP,
    description: "Bounded application startup.",
    kind: super::SpanKind::Internal,
    attributes: &[super::AttributeRequirement {
        name: "outcome",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Recommended,
        allowed_values: &[
            "success",
            "failure",
            "error",
            "timeout",
            "skip",
            "cancellation",
        ],
    }],
};
// registry: kind=internal; attributes=outcome:recommended
pub const BACKGROUND_CYCLE: &str = "background.cycle";
pub const BACKGROUND_CYCLE_DEF: super::SpanMetadata = super::SpanMetadata {
    name: BACKGROUND_CYCLE,
    description: "One periodic cycle.",
    kind: super::SpanKind::Internal,
    attributes: &[super::AttributeRequirement {
        name: "outcome",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Recommended,
        allowed_values: &[
            "success",
            "failure",
            "error",
            "timeout",
            "skip",
            "cancellation",
        ],
    }],
};
// registry: kind=internal; attributes=outcome:recommended
pub const CLI_COMMAND: &str = "cli.command";
pub const CLI_COMMAND_DEF: super::SpanMetadata = super::SpanMetadata {
    name: CLI_COMMAND,
    description: "Bounded CLI command.",
    kind: super::SpanKind::Internal,
    attributes: &[super::AttributeRequirement {
        name: "outcome",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Recommended,
        allowed_values: &[
            "success",
            "failure",
            "error",
            "timeout",
            "skip",
            "cancellation",
        ],
    }],
};
// registry: kind=client; attributes=outcome:recommended
pub const CONNECTION_ATTEMPT: &str = "connection.attempt";
pub const CONNECTION_ATTEMPT_DEF: super::SpanMetadata = super::SpanMetadata {
    name: CONNECTION_ATTEMPT,
    description: "One connection attempt.",
    kind: super::SpanKind::Client,
    attributes: &[super::AttributeRequirement {
        name: "outcome",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Recommended,
        allowed_values: &[
            "success",
            "failure",
            "error",
            "timeout",
            "skip",
            "cancellation",
        ],
    }],
};
// registry: kind=client; attributes=db.operation.name:required,db.system.name:required,outcome:recommended
pub const DB_CLIENT: &str = "db.client";
pub const DB_CLIENT_DEF: super::SpanMetadata = super::SpanMetadata {
    name: DB_CLIENT,
    description: "One bounded database client operation.",
    kind: super::SpanKind::Client,
    attributes: &[
        super::AttributeRequirement {
            name: "db.operation.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "db.system.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
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
// registry: kind=client; attributes=container.id:recommended,gen_ai.provider.name:recommended,http.request.method:required,outcome:recommended,server.address:recommended,url.template:required
pub const HTTP_CLIENT: &str = "http.client";
pub const HTTP_CLIENT_DEF: super::SpanMetadata = super::SpanMetadata {
    name: HTTP_CLIENT,
    description: "One bounded HTTP client request.",
    kind: super::SpanKind::Client,
    attributes: &[
        super::AttributeRequirement {
            name: "container.id",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "gen_ai.provider.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "http.request.method",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
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
            name: "server.address",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "url.template",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[],
        },
    ],
};
// registry: kind=internal; attributes=launch.target.kind:required,outcome:recommended
pub const LAUNCH: &str = "launch";
pub const LAUNCH_DEF: super::SpanMetadata = super::SpanMetadata {
    name: LAUNCH,
    description: "One bounded launch pipeline.",
    kind: super::SpanKind::Internal,
    attributes: &[
        super::AttributeRequirement {
            name: "launch.target.kind",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &["workspace", "directory"],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
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
// registry: kind=internal; attributes=launch.stage.name:required,outcome:recommended
pub const LAUNCH_STAGE: &str = "launch.stage";
pub const LAUNCH_STAGE_DEF: super::SpanMetadata = super::SpanMetadata {
    name: LAUNCH_STAGE,
    description: "One bounded launch pipeline stage.",
    kind: super::SpanKind::Internal,
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
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
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
// registry: kind=consumer; attributes=job.id:recommended,job.type:recommended,outcome:recommended
pub const PREWARM_ATTEMPT: &str = "prewarm.attempt";
pub const PREWARM_ATTEMPT_DEF: super::SpanMetadata = super::SpanMetadata {
    name: PREWARM_ATTEMPT,
    description: "Detached prewarm job attempt.",
    kind: super::SpanKind::Consumer,
    attributes: &[
        super::AttributeRequirement {
            name: "job.id",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "job.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &["image_prewarm", "sidecar_prewarm"],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
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
// registry: kind=producer; attributes=job.id:recommended,job.type:recommended,outcome:recommended
pub const PREWARM_SCHEDULE: &str = "prewarm.schedule";
pub const PREWARM_SCHEDULE_DEF: super::SpanMetadata = super::SpanMetadata {
    name: PREWARM_SCHEDULE,
    description: "Detached prewarm job scheduling decision.",
    kind: super::SpanKind::Producer,
    attributes: &[
        super::AttributeRequirement {
            name: "job.id",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "job.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &["image_prewarm", "sidecar_prewarm"],
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
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
// registry: kind=client; attributes=outcome:recommended,process.executable.name:recommended,process.exit_code:recommended
pub const PROCESS_COMMAND: &str = "process.command";
pub const PROCESS_COMMAND_DEF: super::SpanMetadata = super::SpanMetadata {
    name: PROCESS_COMMAND,
    description: "One subprocess command.",
    kind: super::SpanKind::Client,
    attributes: &[
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
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
            name: "process.executable.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "process.exit_code",
            value_type: super::ValueType::Integer,
            requirement: super::RequirementLevel::Recommended,
            allowed_values: &[],
        },
    ],
};
// registry: kind=client; attributes=outcome:recommended,rpc.method:required,rpc.system.name:required
pub const RPC_CLIENT: &str = "rpc.client";
pub const RPC_CLIENT_DEF: super::SpanMetadata = super::SpanMetadata {
    name: RPC_CLIENT,
    description: "One bounded RPC client request.",
    kind: super::SpanKind::Client,
    attributes: &[
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
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
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "rpc.system.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[],
        },
    ],
};
// registry: kind=server; attributes=outcome:recommended,rpc.method:required,rpc.system.name:required
pub const RPC_SERVER: &str = "rpc.server";
pub const RPC_SERVER_DEF: super::SpanMetadata = super::SpanMetadata {
    name: RPC_SERVER,
    description: "One bounded RPC server request.",
    kind: super::SpanKind::Server,
    attributes: &[
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
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
            allowed_values: &[],
        },
        super::AttributeRequirement {
            name: "rpc.system.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
            allowed_values: &[],
        },
    ],
};
// registry: kind=internal; attributes=outcome:recommended
pub const TELEMETRY_VALIDATE: &str = "telemetry.validate";
pub const TELEMETRY_VALIDATE_DEF: super::SpanMetadata = super::SpanMetadata {
    name: TELEMETRY_VALIDATE,
    description: "Telemetry delivery validation.",
    kind: super::SpanKind::Internal,
    attributes: &[super::AttributeRequirement {
        name: "outcome",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Recommended,
        allowed_values: &[
            "success",
            "failure",
            "error",
            "timeout",
            "skip",
            "cancellation",
        ],
    }],
};
// registry: kind=internal; attributes=outcome:recommended
pub const UI_ACTION: &str = "ui.action";
pub const UI_ACTION_DEF: super::SpanMetadata = super::SpanMetadata {
    name: UI_ACTION,
    description: "Bounded semantic UI action.",
    kind: super::SpanKind::Internal,
    attributes: &[super::AttributeRequirement {
        name: "outcome",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Recommended,
        allowed_values: &[
            "success",
            "failure",
            "error",
            "timeout",
            "skip",
            "cancellation",
        ],
    }],
};
// registry: kind=internal; attributes=outcome:recommended
pub const UI_RENDER: &str = "ui.render";
pub const UI_RENDER_DEF: super::SpanMetadata = super::SpanMetadata {
    name: UI_RENDER,
    description: "Bounded action-triggered render.",
    kind: super::SpanKind::Internal,
    attributes: &[super::AttributeRequirement {
        name: "outcome",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Recommended,
        allowed_values: &[
            "success",
            "failure",
            "error",
            "timeout",
            "skip",
            "cancellation",
        ],
    }],
};
// registry: kind=internal; attributes=outcome:recommended
pub const UI_SCREEN_TRANSITION: &str = "ui.screen.transition";
pub const UI_SCREEN_TRANSITION_DEF: super::SpanMetadata = super::SpanMetadata {
    name: UI_SCREEN_TRANSITION,
    description: "Bounded screen transition.",
    kind: super::SpanKind::Internal,
    attributes: &[super::AttributeRequirement {
        name: "outcome",
        value_type: super::ValueType::String,
        requirement: super::RequirementLevel::Recommended,
        allowed_values: &[
            "success",
            "failure",
            "error",
            "timeout",
            "skip",
            "cancellation",
        ],
    }],
};

pub const ALL: &[&str] = &[
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

pub const DEFINITIONS: &[super::SpanMetadata] = &[
    APP_SHUTDOWN_DEF,
    APP_STARTUP_DEF,
    BACKGROUND_CYCLE_DEF,
    CLI_COMMAND_DEF,
    CONNECTION_ATTEMPT_DEF,
    DB_CLIENT_DEF,
    HTTP_CLIENT_DEF,
    LAUNCH_DEF,
    LAUNCH_STAGE_DEF,
    PREWARM_ATTEMPT_DEF,
    PREWARM_SCHEDULE_DEF,
    PROCESS_COMMAND_DEF,
    RPC_CLIENT_DEF,
    RPC_SERVER_DEF,
    TELEMETRY_VALIDATE_DEF,
    UI_ACTION_DEF,
    UI_RENDER_DEF,
    UI_SCREEN_TRANSITION_DEF,
];

#[must_use]
pub fn definition(name: &str) -> Option<&'static super::SpanMetadata> {
    DEFINITIONS
        .iter()
        .find(|definition| definition.name == name)
}

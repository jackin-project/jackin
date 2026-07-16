// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
// GENERATED from registry/ — do not hand-edit. Regenerate: cargo xtask telemetry-registry --generate.

// registry: attributes=
pub const AGENT_STATE_CHANGED: &str = "agent.state.changed";
pub const AGENT_STATE_CHANGED_DEF: super::EventMetadata = super::EventMetadata {
    name: AGENT_STATE_CHANGED,
    description: "Effective coding-agent state changed.",
    attributes: &[],
};
// registry: attributes=cache.name:required,cache.result:required
pub const CACHE_DECISION: &str = "cache.decision";
pub const CACHE_DECISION_DEF: super::EventMetadata = super::EventMetadata {
    name: CACHE_DECISION,
    description: "Product cache decision made.",
    attributes: &[
        super::AttributeRequirement {
            name: "cache.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
        super::AttributeRequirement {
            name: "cache.result",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
    ],
};
// registry: attributes=
pub const CAPSULE_SESSION_CLEAN_SHUTDOWN: &str = "capsule.session.clean.shutdown";
pub const CAPSULE_SESSION_CLEAN_SHUTDOWN_DEF: super::EventMetadata = super::EventMetadata {
    name: CAPSULE_SESSION_CLEAN_SHUTDOWN,
    description: "Capsule session shut down cleanly.",
    attributes: &[],
};
// registry: attributes=
pub const CAPSULE_SESSION_DETACH: &str = "capsule.session.detach";
pub const CAPSULE_SESSION_DETACH_DEF: super::EventMetadata = super::EventMetadata {
    name: CAPSULE_SESSION_DETACH,
    description: "Operator detached from a capsule session.",
    attributes: &[],
};
// registry: attributes=config.migration.step_count:recommended,config.operation:required,config.schema.version.from:recommended,config.schema.version.to:recommended,config.scope:required,error.type:recommended,outcome:required
pub const CONFIG_OPERATION: &str = "config.operation";
pub const CONFIG_OPERATION_DEF: super::EventMetadata = super::EventMetadata {
    name: CONFIG_OPERATION,
    description: "Configuration operation completed.",
    attributes: &[
        super::AttributeRequirement {
            name: "config.migration.step_count",
            value_type: super::ValueType::Integer,
            requirement: super::RequirementLevel::Recommended,
        },
        super::AttributeRequirement {
            name: "config.operation",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
        super::AttributeRequirement {
            name: "config.schema.version.from",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
        },
        super::AttributeRequirement {
            name: "config.schema.version.to",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
        },
        super::AttributeRequirement {
            name: "config.scope",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
        super::AttributeRequirement {
            name: "error.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
    ],
};
// registry: attributes=
pub const DEBUG_LINE: &str = "debug.line";
pub const DEBUG_LINE_DEF: super::EventMetadata = super::EventMetadata {
    name: DEBUG_LINE,
    description: "Governed debug breadcrumb.",
    attributes: &[],
};
// registry: attributes=
pub const ERROR_TYPED: &str = "error.typed";
pub const ERROR_TYPED_DEF: super::EventMetadata = super::EventMetadata {
    name: ERROR_TYPED,
    description: "Typed product error occurred.",
    attributes: &[],
};
// registry: attributes=dind.mode:required,network.mode:required,outcome:required,workspace.isolation.mode:required
pub const ISOLATION_DECISION: &str = "isolation.decision";
pub const ISOLATION_DECISION_DEF: super::EventMetadata = super::EventMetadata {
    name: ISOLATION_DECISION,
    description: "Workspace isolation and network policy selected.",
    attributes: &[
        super::AttributeRequirement {
            name: "dind.mode",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
        super::AttributeRequirement {
            name: "network.mode",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
        super::AttributeRequirement {
            name: "workspace.isolation.mode",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
    ],
};
// registry: attributes=error.type:required,network.mode:required,outcome:required
pub const ISOLATION_FIREWALL_FAILED: &str = "isolation.firewall.failed";
pub const ISOLATION_FIREWALL_FAILED_DEF: super::EventMetadata = super::EventMetadata {
    name: ISOLATION_FIREWALL_FAILED,
    description: "Fail-closed firewall application failed.",
    attributes: &[
        super::AttributeRequirement {
            name: "error.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
        super::AttributeRequirement {
            name: "network.mode",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
    ],
};
// registry: attributes=app.build_id:conditionally_required,app.crash.id:recommended,exception.message:recommended,exception.stacktrace:recommended,exception.type:recommended,os.name:conditionally_required,os.version:conditionally_required,service.version:conditionally_required,session.id:recommended
pub const APP_CRASH: &str = "app.crash";
pub const APP_CRASH_DEF: super::EventMetadata = super::EventMetadata {
    name: APP_CRASH,
    description: "Application crashed.",
    attributes: &[
        super::AttributeRequirement {
            name: "app.build_id",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::ConditionallyRequired,
        },
        super::AttributeRequirement {
            name: "app.crash.id",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
        },
        super::AttributeRequirement {
            name: "exception.message",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
        },
        super::AttributeRequirement {
            name: "exception.stacktrace",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
        },
        super::AttributeRequirement {
            name: "exception.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
        },
        super::AttributeRequirement {
            name: "os.name",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::ConditionallyRequired,
        },
        super::AttributeRequirement {
            name: "os.version",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::ConditionallyRequired,
        },
        super::AttributeRequirement {
            name: "service.version",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::ConditionallyRequired,
        },
        super::AttributeRequirement {
            name: "session.id",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
        },
    ],
};
// registry: attributes=app.jank.frame_count:recommended,app.jank.period:recommended,app.jank.threshold:recommended
pub const APP_JANK: &str = "app.jank";
pub const APP_JANK_DEF: super::EventMetadata = super::EventMetadata {
    name: APP_JANK,
    description: "Render jank threshold crossed.",
    attributes: &[
        super::AttributeRequirement {
            name: "app.jank.frame_count",
            value_type: super::ValueType::Integer,
            requirement: super::RequirementLevel::Recommended,
        },
        super::AttributeRequirement {
            name: "app.jank.period",
            value_type: super::ValueType::Double,
            requirement: super::RequirementLevel::Recommended,
        },
        super::AttributeRequirement {
            name: "app.jank.threshold",
            value_type: super::ValueType::Double,
            requirement: super::RequirementLevel::Recommended,
        },
    ],
};
// registry: attributes=
pub const LAUNCH_STAGE_DONE: &str = "launch.stage.done";
pub const LAUNCH_STAGE_DONE_DEF: super::EventMetadata = super::EventMetadata {
    name: LAUNCH_STAGE_DONE,
    description: "Launch stage completed.",
    attributes: &[],
};
// registry: attributes=
pub const LAUNCH_STAGE_FAILED: &str = "launch.stage.failed";
pub const LAUNCH_STAGE_FAILED_DEF: super::EventMetadata = super::EventMetadata {
    name: LAUNCH_STAGE_FAILED,
    description: "Launch stage failed.",
    attributes: &[],
};
// registry: attributes=
pub const LAUNCH_STAGE_SKIPPED: &str = "launch.stage.skipped";
pub const LAUNCH_STAGE_SKIPPED_DEF: super::EventMetadata = super::EventMetadata {
    name: LAUNCH_STAGE_SKIPPED,
    description: "Launch stage skipped.",
    attributes: &[],
};
// registry: attributes=
pub const LAUNCH_STAGE_STARTED: &str = "launch.stage.started";
pub const LAUNCH_STAGE_STARTED_DEF: super::EventMetadata = super::EventMetadata {
    name: LAUNCH_STAGE_STARTED,
    description: "Launch stage started.",
    attributes: &[],
};
// registry: attributes=
pub const OPERATION_LOG: &str = "operation.log";
pub const OPERATION_LOG_DEF: super::EventMetadata = super::EventMetadata {
    name: OPERATION_LOG,
    description: "Bounded operation breadcrumb.",
    attributes: &[],
};
// registry: attributes=
pub const OPERATION_WARN: &str = "operation.warn";
pub const OPERATION_WARN_DEF: super::EventMetadata = super::EventMetadata {
    name: OPERATION_WARN,
    description: "Bounded operation warning.",
    attributes: &[],
};
// registry: attributes=
pub const PERFORMANCE_SLOW_FOREGROUND_WAIT: &str = "performance.slow.foreground.wait";
pub const PERFORMANCE_SLOW_FOREGROUND_WAIT_DEF: super::EventMetadata = super::EventMetadata {
    name: PERFORMANCE_SLOW_FOREGROUND_WAIT,
    description: "Foreground wait exceeded its threshold.",
    attributes: &[],
};
// registry: attributes=
pub const PROCESS_SUBPROCESS_DONE: &str = "process.subprocess.done";
pub const PROCESS_SUBPROCESS_DONE_DEF: super::EventMetadata = super::EventMetadata {
    name: PROCESS_SUBPROCESS_DONE,
    description: "Subprocess completed.",
    attributes: &[],
};
// registry: attributes=
pub const PTY_EXIT: &str = "pty.exit";
pub const PTY_EXIT_DEF: super::EventMetadata = super::EventMetadata {
    name: PTY_EXIT,
    description: "PTY child process exited.",
    attributes: &[],
};
// registry: attributes=
pub const PTY_SPAWN: &str = "pty.spawn";
pub const PTY_SPAWN_DEF: super::EventMetadata = super::EventMetadata {
    name: PTY_SPAWN,
    description: "PTY child process spawned.",
    attributes: &[],
};
// registry: attributes=
pub const RUN_SUMMARY: &str = "run.summary";
pub const RUN_SUMMARY_DEF: super::EventMetadata = super::EventMetadata {
    name: RUN_SUMMARY,
    description: "Invocation summary produced.",
    attributes: &[],
};
// registry: attributes=
pub const SESSION_END: &str = "session.end";
pub const SESSION_END_DEF: super::EventMetadata = super::EventMetadata {
    name: SESSION_END,
    description: "Interactive session ended.",
    attributes: &[],
};
// registry: attributes=
pub const SESSION_START: &str = "session.start";
pub const SESSION_START_DEF: super::EventMetadata = super::EventMetadata {
    name: SESSION_START,
    description: "Interactive session started.",
    attributes: &[],
};
// registry: attributes=
pub const TELEMETRY_VALIDATE: &str = "telemetry.validate";
pub const TELEMETRY_VALIDATE_DEF: super::EventMetadata = super::EventMetadata {
    name: TELEMETRY_VALIDATE,
    description: "Telemetry delivery validation marker.",
    attributes: &[],
};
// registry: attributes=
pub const TIMING_DONE: &str = "timing.done";
pub const TIMING_DONE_DEF: super::EventMetadata = super::EventMetadata {
    name: TIMING_DONE,
    description: "Timing interval completed.",
    attributes: &[],
};
// registry: attributes=
pub const TIMING_STARTED: &str = "timing.started";
pub const TIMING_STARTED_DEF: super::EventMetadata = super::EventMetadata {
    name: TIMING_STARTED,
    description: "Timing interval started.",
    attributes: &[],
};
// registry: attributes=error.type:recommended,outcome:required,trust.decision:required,trust.source.type:required
pub const TRUST_DECISION: &str = "trust.decision";
pub const TRUST_DECISION_DEF: super::EventMetadata = super::EventMetadata {
    name: TRUST_DECISION,
    description: "Role-source trust decision applied.",
    attributes: &[
        super::AttributeRequirement {
            name: "error.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Recommended,
        },
        super::AttributeRequirement {
            name: "outcome",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
        super::AttributeRequirement {
            name: "trust.decision",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
        super::AttributeRequirement {
            name: "trust.source.type",
            value_type: super::ValueType::String,
            requirement: super::RequirementLevel::Required,
        },
    ],
};
// registry: attributes=
pub const UI_SCREEN_ENTERED: &str = "ui.screen.entered";
pub const UI_SCREEN_ENTERED_DEF: super::EventMetadata = super::EventMetadata {
    name: UI_SCREEN_ENTERED,
    description: "Screen visit entered.",
    attributes: &[],
};
// registry: attributes=
pub const UI_SCREEN_EXITED: &str = "ui.screen.exited";
pub const UI_SCREEN_EXITED_DEF: super::EventMetadata = super::EventMetadata {
    name: UI_SCREEN_EXITED,
    description: "Screen visit exited.",
    attributes: &[],
};
// registry: attributes=
pub const UI_WIDGET_FOCUSED: &str = "ui.widget.focused";
pub const UI_WIDGET_FOCUSED_DEF: super::EventMetadata = super::EventMetadata {
    name: UI_WIDGET_FOCUSED,
    description: "Widget gained focus.",
    attributes: &[],
};
// registry: attributes=
pub const UI_WIDGET_UNFOCUSED: &str = "ui.widget.unfocused";
pub const UI_WIDGET_UNFOCUSED_DEF: super::EventMetadata = super::EventMetadata {
    name: UI_WIDGET_UNFOCUSED,
    description: "Widget lost focus.",
    attributes: &[],
};

pub const ALL: &[&str] = &[
    AGENT_STATE_CHANGED,
    CACHE_DECISION,
    CAPSULE_SESSION_CLEAN_SHUTDOWN,
    CAPSULE_SESSION_DETACH,
    CONFIG_OPERATION,
    DEBUG_LINE,
    ERROR_TYPED,
    ISOLATION_DECISION,
    ISOLATION_FIREWALL_FAILED,
    APP_CRASH,
    APP_JANK,
    LAUNCH_STAGE_DONE,
    LAUNCH_STAGE_FAILED,
    LAUNCH_STAGE_SKIPPED,
    LAUNCH_STAGE_STARTED,
    OPERATION_LOG,
    OPERATION_WARN,
    PERFORMANCE_SLOW_FOREGROUND_WAIT,
    PROCESS_SUBPROCESS_DONE,
    PTY_EXIT,
    PTY_SPAWN,
    RUN_SUMMARY,
    SESSION_END,
    SESSION_START,
    TELEMETRY_VALIDATE,
    TIMING_DONE,
    TIMING_STARTED,
    TRUST_DECISION,
    UI_SCREEN_ENTERED,
    UI_SCREEN_EXITED,
    UI_WIDGET_FOCUSED,
    UI_WIDGET_UNFOCUSED,
];

pub const DEFINITIONS: &[super::EventMetadata] = &[
    AGENT_STATE_CHANGED_DEF,
    CACHE_DECISION_DEF,
    CAPSULE_SESSION_CLEAN_SHUTDOWN_DEF,
    CAPSULE_SESSION_DETACH_DEF,
    CONFIG_OPERATION_DEF,
    DEBUG_LINE_DEF,
    ERROR_TYPED_DEF,
    ISOLATION_DECISION_DEF,
    ISOLATION_FIREWALL_FAILED_DEF,
    APP_CRASH_DEF,
    APP_JANK_DEF,
    LAUNCH_STAGE_DONE_DEF,
    LAUNCH_STAGE_FAILED_DEF,
    LAUNCH_STAGE_SKIPPED_DEF,
    LAUNCH_STAGE_STARTED_DEF,
    OPERATION_LOG_DEF,
    OPERATION_WARN_DEF,
    PERFORMANCE_SLOW_FOREGROUND_WAIT_DEF,
    PROCESS_SUBPROCESS_DONE_DEF,
    PTY_EXIT_DEF,
    PTY_SPAWN_DEF,
    RUN_SUMMARY_DEF,
    SESSION_END_DEF,
    SESSION_START_DEF,
    TELEMETRY_VALIDATE_DEF,
    TIMING_DONE_DEF,
    TIMING_STARTED_DEF,
    TRUST_DECISION_DEF,
    UI_SCREEN_ENTERED_DEF,
    UI_SCREEN_EXITED_DEF,
    UI_WIDGET_FOCUSED_DEF,
    UI_WIDGET_UNFOCUSED_DEF,
];

#[must_use]
pub fn definition(name: &str) -> Option<&'static super::EventMetadata> {
    DEFINITIONS
        .iter()
        .find(|definition| definition.name == name)
}

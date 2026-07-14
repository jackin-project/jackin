// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Fail-closed telemetry event registry.
//!
//! Every structured event exported through the diagnostics layer must resolve
//! to a registered [`EventDef`]. Validation rejects unknown names, unknown
//! attribute keys, missing required keys, prohibited spellings, and body shapes
//! that violate the redaction/size policy.

use std::fmt;

/// Native attribute type on the wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttrType {
    Str,
    I64,
    F64,
    Bool,
    StrArray,
}

/// One attribute key and its native type.
#[derive(Clone, Copy, Debug)]
pub struct AttrDef {
    pub key: &'static str,
    pub ty: AttrType,
}

/// Closed set of allowed `event.outcome` values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Outcome {
    Success,
    Failure,
    Timeout,
    Cancelled,
    CacheHit,
    ExpectedClose,
}

impl Outcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::CacheHit => "cache_hit",
            Self::ExpectedClose => "expected_close",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "success" => Some(Self::Success),
            "failure" => Some(Self::Failure),
            "timeout" => Some(Self::Timeout),
            "cancelled" => Some(Self::Cancelled),
            "cache_hit" => Some(Self::CacheHit),
            "expected_close" => Some(Self::ExpectedClose),
            _ => None,
        }
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Default severity for a registered event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Debug,
    Info,
    Warn,
    Error,
}

/// Privacy class for retention/export policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Privacy {
    Routine,
    Evidence,
}

/// Expected cardinality of the event stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cardinality {
    Low,
    Bounded,
}

/// Sink eligibility flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SinkSet {
    pub otlp: bool,
    pub jsonl: bool,
    pub capsule_file: bool,
}

impl SinkSet {
    pub const ALL: Self = Self {
        otlp: true,
        jsonl: true,
        capsule_file: true,
    };

    pub const OTLP_JSONL: Self = Self {
        otlp: true,
        jsonl: true,
        capsule_file: false,
    };
}

/// One registered event definition — the contract for emit sites and exporters.
#[derive(Clone, Copy, Debug)]
pub struct EventDef {
    /// Dotted wire name, e.g. `launch.stage.failed`.
    pub name: &'static str,
    /// Legacy kind token used by JSONL `kind` field (`snake_case` or dotted).
    pub kind: &'static str,
    pub severity: Severity,
    /// One stable body intent string.
    pub body: &'static str,
    pub required: &'static [AttrDef],
    pub optional: &'static [AttrDef],
    pub outcomes: &'static [Outcome],
    pub privacy: Privacy,
    pub cardinality: Cardinality,
    pub sinks: SinkSet,
    /// Attribute keys that feed the error fingerprint.
    pub fingerprint: &'static [&'static str],
    /// Owning crate name.
    pub owner: &'static str,
    /// Default `jackin.component` value.
    pub component: &'static str,
    /// Default `jackin.category` value.
    pub category: &'static str,
    /// Default `jackin.operation` stem (stage/timing qualify with stage token).
    pub operation: &'static str,
}

/// Registry validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    UnknownEvent(String),
    UnknownAttr { event: String, key: String },
    MissingRequired { event: String, key: String },
    ProhibitedKey(String),
    BodyPolicy(String),
    DisallowedOutcome { event: String, outcome: String },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEvent(name) => write!(f, "unknown event name `{name}`"),
            Self::UnknownAttr { event, key } => {
                write!(f, "unknown attribute `{key}` on event `{event}`")
            }
            Self::MissingRequired { event, key } => {
                write!(f, "missing required attribute `{key}` on event `{event}`")
            }
            Self::ProhibitedKey(key) => write!(f, "prohibited attribute key `{key}`"),
            Self::BodyPolicy(reason) => write!(f, "body policy violation: {reason}"),
            Self::DisallowedOutcome { event, outcome } => {
                write!(f, "outcome `{outcome}` not allowed on event `{event}`")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// Keys that must never appear as export attributes (wrong spelling or internal-only).
pub const PROHIBITED_EXPORT_KEYS: &[&str] =
    &["error_type", "log.category", "stage", "kind", "run_id"];

/// Launch/stage tokens observed in fixtures and production paths.
pub mod otel_stages {
    pub const PREFLIGHT: &str = "preflight";
    pub const IMAGE: &str = "image";
    pub const RUN: &str = "run";
    pub const ATTACH: &str = "attach";
    pub const CLEANUP: &str = "cleanup";
    pub const PREPARE: &str = "prepare";
    pub const DERIVED_IMAGE: &str = "derived image";
    pub const START_CONTAINER: &str = "start container";
    pub const HARDLINE: &str = "hardline";
    pub const CREDENTIALS: &str = "credentials";
    pub const BUILDING: &str = "building";
    pub const PLAN: &str = "plan";
    pub const RESTORE: &str = "restore";
    pub const SIDECAR: &str = "sidecar";
    pub const OP: &str = "op";
    pub const LAUNCH: &str = "launch";
    pub const ALL: &[&str] = &[
        PREFLIGHT,
        IMAGE,
        RUN,
        ATTACH,
        CLEANUP,
        PREPARE,
        DERIVED_IMAGE,
        START_CONTAINER,
        HARDLINE,
        CREDENTIALS,
        BUILDING,
        PLAN,
        RESTORE,
        SIDECAR,
        OP,
        LAUNCH,
    ];
}

/// Stable span name for a launch stage label (`launch.derived_image`, …).
/// Unknown labels fall back to `launch.stage` (registered generic) so free
/// strings cannot invent unbounded span names.
#[must_use]
pub fn launch_stage_span_name(stage: &str) -> &'static str {
    match stage {
        otel_stages::DERIVED_IMAGE => "launch.derived_image",
        otel_stages::PREFLIGHT => "launch.preflight",
        otel_stages::IMAGE => "launch.image",
        otel_stages::RUN => "launch.run",
        otel_stages::ATTACH => "launch.attach",
        otel_stages::CLEANUP => "launch.cleanup",
        otel_stages::PREPARE => "launch.prepare",
        otel_stages::START_CONTAINER => "launch.start_container",
        otel_stages::HARDLINE => "launch.hardline",
        otel_stages::CREDENTIALS => "launch.credentials",
        otel_stages::RESTORE => "launch.restore",
        otel_stages::SIDECAR => "launch.sidecar",
        otel_stages::LAUNCH => "launch.launch",
        otel_stages::PLAN => "launch.plan",
        otel_stages::OP => "launch.op",
        otel_stages::BUILDING => "launch.building",
        _ => "launch.stage",
    }
}

const ATTR_JACKIN_STAGE: AttrDef = AttrDef {
    key: "jackin.stage",
    ty: AttrType::Str,
};
const ATTR_ERROR_TYPE: AttrDef = AttrDef {
    key: "error.type",
    ty: AttrType::Str,
};
const ATTR_DETAIL: AttrDef = AttrDef {
    key: "detail",
    ty: AttrType::Str,
};
const ATTR_PROCESS_COMMAND: AttrDef = AttrDef {
    key: "process.command",
    ty: AttrType::Str,
};
const ATTR_PROCESS_ARGS: AttrDef = AttrDef {
    key: "process.args_redacted",
    ty: AttrType::Str,
};
const ATTR_PROCESS_EXIT: AttrDef = AttrDef {
    key: "process.exit_code",
    ty: AttrType::I64,
};

const OUTCOMES_SUCCESS: &[Outcome] = &[Outcome::Success];
const OUTCOMES_SUCCESS_FAIL: &[Outcome] = &[Outcome::Success, Outcome::Failure];
const OUTCOMES_STAGE: &[Outcome] = &[Outcome::Success, Outcome::Failure, Outcome::Cancelled];
const OUTCOMES_PROCESS: &[Outcome] = &[
    Outcome::Success,
    Outcome::Failure,
    Outcome::Timeout,
    Outcome::Cancelled,
];
const OUTCOMES_EXPECTED_CLOSE: &[Outcome] = &[Outcome::ExpectedClose];
const OUTCOMES_ERROR: &[Outcome] = &[Outcome::Failure];

macro_rules! def {
    (
        name: $name:expr,
        kind: $kind:expr,
        severity: $sev:expr,
        body: $body:expr,
        required: $req:expr,
        optional: $opt:expr,
        outcomes: $out:expr,
        privacy: $priv:expr,
        cardinality: $card:expr,
        sinks: $sinks:expr,
        fingerprint: $fp:expr,
        owner: $owner:expr,
        component: $comp:expr,
        category: $cat:expr,
        operation: $op:expr $(,)?
    ) => {
        EventDef {
            name: $name,
            kind: $kind,
            severity: $sev,
            body: $body,
            required: $req,
            optional: $opt,
            outcomes: $out,
            privacy: $priv,
            cardinality: $card,
            sinks: $sinks,
            fingerprint: $fp,
            owner: $owner,
            component: $comp,
            category: $cat,
            operation: $op,
        }
    };
}

/// All registered event definitions. Order matches historical `otel_events::ALL`
/// plus facade/error defs needed by the typed operation API.
pub static EVENT_DEFS: &[EventDef] = &[
    def!(
        name: "launch.stage.started",
        kind: "stage_started",
        severity: Severity::Info,
        body: "launch stage started",
        required: &[],
        optional: &[ATTR_JACKIN_STAGE, ATTR_DETAIL],
        outcomes: OUTCOMES_STAGE,
        privacy: Privacy::Routine,
        cardinality: Cardinality::Low,
        sinks: SinkSet::ALL,
        fingerprint: &[],
        owner: "jackin-diagnostics",
        component: "host",
        category: "launch",
        operation: "stage",
    ),
    def!(
        name: "launch.stage.done",
        kind: "stage_done",
        severity: Severity::Info,
        body: "launch stage completed",
        required: &[],
        optional: &[ATTR_JACKIN_STAGE, ATTR_DETAIL],
        outcomes: OUTCOMES_STAGE,
        privacy: Privacy::Routine,
        cardinality: Cardinality::Low,
        sinks: SinkSet::ALL,
        fingerprint: &[],
        owner: "jackin-diagnostics",
        component: "host",
        category: "launch",
        operation: "stage",
    ),
    def!(
        name: "launch.stage.failed",
        kind: "stage_failed",
        severity: Severity::Error,
        body: "launch stage failed",
        required: &[],
        optional: &[ATTR_JACKIN_STAGE, ATTR_DETAIL, ATTR_ERROR_TYPE],
        outcomes: OUTCOMES_ERROR,
        privacy: Privacy::Evidence,
        cardinality: Cardinality::Low,
        sinks: SinkSet::ALL,
        fingerprint: &["error.type", "jackin.stage"],
        owner: "jackin-diagnostics",
        component: "host",
        category: "launch",
        operation: "stage",
    ),
    def!(
        name: "launch.stage.skipped",
        kind: "stage_skipped",
        severity: Severity::Info,
        body: "launch stage skipped",
        required: &[],
        optional: &[ATTR_JACKIN_STAGE, ATTR_DETAIL],
        outcomes: &[Outcome::Cancelled, Outcome::Success],
        privacy: Privacy::Routine,
        cardinality: Cardinality::Low,
        sinks: SinkSet::ALL,
        fingerprint: &[],
        owner: "jackin-diagnostics",
        component: "host",
        category: "launch",
        operation: "stage",
    ),
    def!(
        name: "timing.started",
        kind: "timing_started",
        severity: Severity::Info,
        body: "timing interval started",
        required: &[],
        optional: &[ATTR_JACKIN_STAGE, ATTR_DETAIL],
        outcomes: OUTCOMES_SUCCESS,
        privacy: Privacy::Routine,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet::OTLP_JSONL,
        fingerprint: &[],
        owner: "jackin-diagnostics",
        component: "host",
        category: "timing",
        operation: "timing",
    ),
    def!(
        name: "timing.done",
        kind: "timing_done",
        severity: Severity::Info,
        body: "timing interval completed",
        required: &[],
        optional: &[ATTR_JACKIN_STAGE, ATTR_DETAIL],
        outcomes: OUTCOMES_SUCCESS_FAIL,
        privacy: Privacy::Routine,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet::OTLP_JSONL,
        fingerprint: &[],
        owner: "jackin-diagnostics",
        component: "host",
        category: "timing",
        operation: "timing",
    ),
    def!(
        name: "debug.line",
        kind: "debug",
        severity: Severity::Debug,
        body: "debug firehose line",
        required: &[],
        optional: &[ATTR_DETAIL],
        outcomes: OUTCOMES_SUCCESS,
        privacy: Privacy::Evidence,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet::OTLP_JSONL,
        fingerprint: &[],
        owner: "jackin-diagnostics",
        component: "host",
        category: "debug",
        operation: "debug",
    ),
    def!(
        name: "process.subprocess.done",
        kind: "subprocess_done",
        severity: Severity::Info,
        body: "subprocess finished",
        required: &[],
        optional: &[ATTR_PROCESS_COMMAND, ATTR_PROCESS_EXIT, ATTR_DETAIL],
        outcomes: OUTCOMES_PROCESS,
        privacy: Privacy::Routine,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet::ALL,
        fingerprint: &["process.exit_code"],
        owner: "jackin-diagnostics",
        component: "host",
        category: "process",
        operation: "process.subprocess.done",
    ),
    def!(
        name: "telemetry.otlp.internal",
        kind: "otlp_internal",
        severity: Severity::Warn,
        body: "opentelemetry internal event",
        required: &[],
        optional: &[ATTR_DETAIL],
        outcomes: OUTCOMES_SUCCESS_FAIL,
        privacy: Privacy::Evidence,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet {
            otlp: false,
            jsonl: true,
            capsule_file: false,
        },
        fingerprint: &[],
        owner: "jackin-diagnostics",
        component: "host",
        category: "telemetry",
        operation: "telemetry.otlp.internal",
    ),
    def!(
        name: "run.summary",
        kind: "run_summary",
        severity: Severity::Info,
        body: "run summary",
        required: &[],
        optional: &[ATTR_DETAIL],
        outcomes: OUTCOMES_SUCCESS,
        privacy: Privacy::Routine,
        cardinality: Cardinality::Low,
        sinks: SinkSet::ALL,
        fingerprint: &[],
        owner: "jackin-diagnostics",
        component: "host",
        category: "summary",
        operation: "run.summary",
    ),
    def!(
        name: "performance.slow.foreground.wait",
        kind: "slow_foreground_wait",
        severity: Severity::Warn,
        body: "slow foreground wait",
        required: &[],
        optional: &[ATTR_DETAIL],
        outcomes: OUTCOMES_SUCCESS,
        privacy: Privacy::Routine,
        cardinality: Cardinality::Low,
        sinks: SinkSet::ALL,
        fingerprint: &[],
        owner: "jackin-diagnostics",
        component: "host",
        category: "performance",
        operation: "performance.slow.foreground.wait",
    ),
    def!(
        name: "capsule.session.detach",
        kind: "session_detach",
        severity: Severity::Info,
        body: "operator detached",
        required: &[],
        optional: &[],
        outcomes: OUTCOMES_EXPECTED_CLOSE,
        privacy: Privacy::Routine,
        cardinality: Cardinality::Low,
        sinks: SinkSet::ALL,
        fingerprint: &[],
        owner: "jackin-diagnostics",
        component: "capsule",
        category: "session",
        operation: "capsule.session.detach",
    ),
    def!(
        name: "capsule.session.clean.shutdown",
        kind: "clean_shutdown",
        severity: Severity::Info,
        body: "container exited cleanly",
        required: &[],
        optional: &[],
        outcomes: OUTCOMES_EXPECTED_CLOSE,
        privacy: Privacy::Routine,
        cardinality: Cardinality::Low,
        sinks: SinkSet::ALL,
        fingerprint: &[],
        owner: "jackin-diagnostics",
        component: "capsule",
        category: "session",
        operation: "capsule.session.clean.shutdown",
    ),
    def!(
        name: "process.execute",
        kind: "process.execute",
        severity: Severity::Info,
        body: "host process execute",
        required: &[],
        optional: &[
            ATTR_PROCESS_COMMAND,
            ATTR_PROCESS_ARGS,
            ATTR_PROCESS_EXIT,
            ATTR_ERROR_TYPE,
        ],
        outcomes: OUTCOMES_PROCESS,
        privacy: Privacy::Routine,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet::ALL,
        fingerprint: &["error.type", "process.exit_code"],
        owner: "jackin-docker",
        component: "host",
        category: "process",
        operation: "process.execute",
    ),
    // Typed facade / free-form error kinds used before plan-008 migration.
    def!(
        name: "error.typed",
        kind: "error.typed",
        severity: Severity::Error,
        body: "typed error",
        required: &[ATTR_ERROR_TYPE],
        optional: &[ATTR_DETAIL],
        outcomes: OUTCOMES_ERROR,
        privacy: Privacy::Evidence,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet::ALL,
        fingerprint: &["error.type"],
        owner: "jackin-diagnostics",
        component: "host",
        category: "error",
        operation: "error.typed",
    ),
    def!(
        name: "operation.log",
        kind: "operation",
        severity: Severity::Info,
        body: "operation log",
        required: &[],
        optional: &[ATTR_DETAIL, ATTR_ERROR_TYPE],
        outcomes: &[
            Outcome::Success,
            Outcome::Failure,
            Outcome::Cancelled,
            Outcome::Timeout,
            Outcome::CacheHit,
        ],
        privacy: Privacy::Routine,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet::ALL,
        fingerprint: &["error.type"],
        owner: "jackin-diagnostics",
        component: "host",
        category: "operation",
        operation: "operation.log",
    ),
    // Generic capsule breadcrumb events (plan 004) — floor shape for clog!/cdebug!.
    def!(
        name: "capsule.log",
        kind: "capsule.log",
        severity: Severity::Info,
        body: "capsule log",
        required: &[],
        optional: &[ATTR_DETAIL],
        outcomes: OUTCOMES_SUCCESS,
        privacy: Privacy::Routine,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet::ALL,
        fingerprint: &[],
        owner: "jackin-usage",
        component: "capsule",
        category: "capsule",
        operation: "capsule.log",
    ),
    def!(
        name: "capsule.debug",
        kind: "capsule.debug",
        severity: Severity::Debug,
        body: "capsule debug",
        required: &[],
        optional: &[ATTR_DETAIL],
        outcomes: OUTCOMES_SUCCESS,
        privacy: Privacy::Evidence,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet::ALL,
        fingerprint: &[],
        owner: "jackin-usage",
        component: "capsule",
        category: "capsule",
        operation: "capsule.debug",
    ),
    def!(
        name: "capsule.warn",
        kind: "capsule.warn",
        severity: Severity::Warn,
        body: "capsule warn",
        required: &[],
        optional: &[ATTR_DETAIL],
        outcomes: &[Outcome::Cancelled, Outcome::Success],
        privacy: Privacy::Routine,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet::ALL,
        fingerprint: &[],
        owner: "jackin-usage",
        component: "capsule",
        category: "capsule",
        operation: "capsule.warn",
    ),
    def!(
        name: "capsule.error",
        kind: "capsule.error",
        severity: Severity::Error,
        body: "capsule error",
        required: &[],
        optional: &[ATTR_DETAIL, ATTR_ERROR_TYPE],
        outcomes: OUTCOMES_ERROR,
        privacy: Privacy::Evidence,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet::ALL,
        fingerprint: &["error.type"],
        owner: "jackin-usage",
        component: "capsule",
        category: "capsule",
        operation: "capsule.error",
    ),
    def!(
        name: "feature.decision",
        kind: "feature.decision",
        severity: Severity::Info,
        body: "feature decision",
        required: &[],
        optional: &[
            AttrDef { key: "feature.key", ty: AttrType::Str },
            AttrDef { key: "feature.provider", ty: AttrType::Str },
            AttrDef { key: "feature.variant", ty: AttrType::Str },
        ],
        outcomes: OUTCOMES_SUCCESS,
        privacy: Privacy::Routine,
        cardinality: Cardinality::Low,
        sinks: SinkSet::ALL,
        fingerprint: &[],
        owner: "jackin-diagnostics",
        component: "host",
        category: "feature",
        operation: "feature.decision",
    ),
    def!(
        name: "capsule.trace",
        kind: "capsule.trace",
        severity: Severity::Debug,
        body: "capsule trace",
        required: &[],
        optional: &[ATTR_DETAIL],
        outcomes: OUTCOMES_SUCCESS,
        privacy: Privacy::Evidence,
        cardinality: Cardinality::Bounded,
        sinks: SinkSet {
            otlp: true,
            jsonl: false,
            capsule_file: false,
        },
        fingerprint: &[],
        owner: "jackin-usage",
        component: "capsule",
        category: "capsule",
        operation: "capsule.trace",
    ),
];

/// Look up a definition by dotted event name or legacy kind token.
#[must_use]
pub fn lookup(name_or_kind: &str) -> Option<&'static EventDef> {
    EVENT_DEFS
        .iter()
        .find(|def| def.name == name_or_kind || def.kind == name_or_kind)
}

/// True when `key` is prohibited as an export attribute.
#[must_use]
pub fn is_prohibited_key(key: &str) -> bool {
    PROHIBITED_EXPORT_KEYS.contains(&key)
}

/// Fail-closed validation of an event emission intent.
pub fn validate(
    name: &str,
    attrs: &[(&str, &str)],
    body: &str,
) -> Result<&'static EventDef, RegistryError> {
    if body.starts_with('[') {
        return Err(RegistryError::BodyPolicy(
            "body must not start with '[' (bracket prefixes are console-only)".into(),
        ));
    }
    for (key, _) in attrs {
        if is_prohibited_key(key) {
            return Err(RegistryError::ProhibitedKey((*key).to_owned()));
        }
    }
    let def = lookup(name).ok_or_else(|| RegistryError::UnknownEvent(name.to_owned()))?;
    let allowed: Vec<&str> = def
        .required
        .iter()
        .chain(def.optional.iter())
        .map(|a| a.key)
        .collect();
    for (key, _) in attrs {
        if !allowed.contains(key)
            && *key != "event.name"
            && *key != "event.outcome"
            && *key != "jackin.component"
            && *key != "jackin.operation"
            && *key != "jackin.category"
        {
            return Err(RegistryError::UnknownAttr {
                event: def.name.to_owned(),
                key: (*key).to_owned(),
            });
        }
    }
    for req in def.required {
        if !attrs.iter().any(|(k, _)| *k == req.key) {
            return Err(RegistryError::MissingRequired {
                event: def.name.to_owned(),
                key: req.key.to_owned(),
            });
        }
    }
    Ok(def)
}

/// Validate that `outcome` is allowed for the event.
pub fn validate_outcome(def: &EventDef, outcome: Outcome) -> Result<(), RegistryError> {
    if def.outcomes.contains(&outcome) {
        Ok(())
    } else {
        Err(RegistryError::DisallowedOutcome {
            event: def.name.to_owned(),
            outcome: outcome.as_str().to_owned(),
        })
    }
}

/// Normalize a free-form stage token for operation qualification.
#[must_use]
pub fn normalize_stage_token(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '.'
            }
        })
        .collect::<String>()
        .split('.')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

/// Whether a stage token is on the registered list (after normalization compare).
#[must_use]
pub fn is_known_stage(stage: &str) -> bool {
    let normalized = normalize_stage_token(stage);
    otel_stages::ALL
        .iter()
        .any(|s| normalize_stage_token(s) == normalized)
}

#[cfg(test)]
mod tests;

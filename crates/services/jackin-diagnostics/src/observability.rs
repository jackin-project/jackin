// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Direct OTLP subscriber setup with no terminal or local-file sink.

use tracing_subscriber::prelude::*;

mod config;
mod health;
mod resource;
pub use health::{
    CapsuleExportCoverage, TelemetryFlushStatus, TelemetryHealth, TelemetrySignalHealth,
    record_telemetry_rejection, telemetry_health_snapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationReport {
    pub elapsed: std::time::Duration,
    pub health: TelemetryHealth,
}

/// Sanitized class of invalid OTLP configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryConfigFailure {
    MissingSignalEndpoint,
    UnsupportedProtocol,
    ConflictingSampler,
    UnsupportedCompression,
    InvalidTimeout,
    InvalidHeaders,
    InvalidResourceAttributes,
    InvalidEndpoint,
    EmptyValue,
    IncompleteClientIdentity,
}

impl From<config::OtlpConfigError> for TelemetryConfigFailure {
    fn from(value: config::OtlpConfigError) -> Self {
        match value {
            config::OtlpConfigError::MissingSignalEndpoint(_) => Self::MissingSignalEndpoint,
            config::OtlpConfigError::UnsupportedProtocol { .. } => Self::UnsupportedProtocol,
            config::OtlpConfigError::ConflictingSampler => Self::ConflictingSampler,
            config::OtlpConfigError::UnsupportedCompression { .. } => Self::UnsupportedCompression,
            config::OtlpConfigError::InvalidTimeout { .. } => Self::InvalidTimeout,
            config::OtlpConfigError::InvalidHeaders { .. } => Self::InvalidHeaders,
            config::OtlpConfigError::InvalidResourceAttribute => Self::InvalidResourceAttributes,
            config::OtlpConfigError::InvalidEndpoint(_) => Self::InvalidEndpoint,
            config::OtlpConfigError::EmptyValue(_) => Self::EmptyValue,
            config::OtlpConfigError::IncompleteClientIdentity(_) => Self::IncompleteClientIdentity,
        }
    }
}

impl std::fmt::Display for TelemetryConfigFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::MissingSignalEndpoint => "missing signal endpoint",
            Self::UnsupportedProtocol => "unsupported protocol",
            Self::ConflictingSampler => "conflicting sampler",
            Self::UnsupportedCompression => "unsupported compression",
            Self::InvalidTimeout => "invalid timeout",
            Self::InvalidHeaders => "invalid headers",
            Self::InvalidResourceAttributes => "invalid resource attributes",
            Self::InvalidEndpoint => "invalid endpoint",
            Self::EmptyValue => "empty configuration value",
            Self::IncompleteClientIdentity => "incomplete client identity",
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationFailure {
    NoEndpoint,
    Disabled,
    Config(TelemetryConfigFailure),
    Inactive,
    Timeout,
    Export(&'static str),
    Rejected,
}

impl std::fmt::Display for ValidationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoEndpoint => formatter.write_str("no endpoint configured"),
            Self::Disabled => formatter.write_str("OpenTelemetry SDK is disabled"),
            Self::Config(failure) => {
                write!(formatter, "invalid telemetry configuration: {failure}")
            }
            Self::Inactive => formatter.write_str("telemetry providers are not active"),
            Self::Timeout => formatter.write_str("telemetry flush timed out"),
            Self::Export(signal) => write!(formatter, "telemetry export failed for {signal}"),
            Self::Rejected => {
                formatter.write_str("telemetry marker was rejected by the governed facade")
            }
        }
    }
}

impl std::error::Error for ValidationFailure {}

/// Emit one marker for every signal and synchronously confirm exporter delivery.
pub fn validate_delivery() -> Result<ValidationReport, ValidationFailure> {
    if std::env::var("OTEL_SDK_DISABLED").is_ok_and(|value| value.eq_ignore_ascii_case("true")) {
        return Err(ValidationFailure::Disabled);
    }
    match resolved_otlp_config_fingerprint() {
        Err(failure) => return Err(ValidationFailure::Config(failure)),
        Ok(None) => return Err(ValidationFailure::NoEndpoint),
        Ok(Some(_)) => {}
    }
    let before = telemetry_health_snapshot();
    if before.active_signals != 3 {
        return Err(ValidationFailure::Inactive);
    }
    let operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::TELEMETRY_VALIDATE, &[])
            .map_err(|_| ValidationFailure::Rejected)?;
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TELEMETRY_VALIDATE,
        jackin_telemetry::FieldSet::default(),
    )
    .map_err(|_| ValidationFailure::Rejected)?;
    jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE)
        .add(1, &[])
        .map_err(|_| ValidationFailure::Rejected)?;
    operation.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    let started = std::time::Instant::now();
    otlp::validate_flush()?;
    let health = telemetry_health_snapshot();
    validate_delivery_delta(before, health)?;
    Ok(ValidationReport {
        elapsed: started.elapsed(),
        health,
    })
}

fn validate_delivery_delta(
    before: TelemetryHealth,
    after: TelemetryHealth,
) -> Result<(), ValidationFailure> {
    if after.flush != TelemetryFlushStatus::Succeeded {
        return Err(ValidationFailure::Export("flush"));
    }
    if after.facade_rejections > before.facade_rejections {
        return Err(ValidationFailure::Rejected);
    }
    for (name, prior, current) in [
        ("traces", before.traces, after.traces),
        ("logs", before.logs, after.logs),
        ("metrics", before.metrics, after.metrics),
    ] {
        if current.failures > prior.failures || current.successes <= prior.successes {
            return Err(ValidationFailure::Export(name));
        }
    }
    Ok(())
}

/// Install the global subscriber and direct OTLP exporters when configured.
///
/// Without an endpoint this installs only the governed subscriber; telemetry
/// remains in memory and no local telemetry artifact is created. With a
/// standard OTLP endpoint, spans, logs, and metrics are exported directly and
/// correlated by governed invocation and session attributes.
///
/// Returns `Ok(true)` when all three OTLP providers were installed and
/// `Ok(false)` when no endpoint is configured. Returns `Err` when configured
/// providers fail to build or the subscriber is already set. The owning
/// [`RunDiagnostics`](crate::RunDiagnostics) keeps product execution fail-open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceIdentity {
    service_name: &'static str,
    app_mode: jackin_telemetry::schema::enums::AppMode,
}

impl ServiceIdentity {
    pub const HOST_ONE_SHOT: Self = Self {
        service_name: "jackin",
        app_mode: jackin_telemetry::schema::enums::AppMode::OneShot,
    };
    pub const HOST_INTERACTIVE: Self = Self {
        service_name: "jackin",
        app_mode: jackin_telemetry::schema::enums::AppMode::Interactive,
    };
    pub const CAPSULE: Self = Self {
        service_name: "jackin-capsule",
        app_mode: jackin_telemetry::schema::enums::AppMode::Capsule,
    };
    pub const DAEMON: Self = Self {
        service_name: "jackin-daemon",
        app_mode: jackin_telemetry::schema::enums::AppMode::Daemon,
    };
    pub const ROLE: Self = Self {
        service_name: "jackin-role",
        app_mode: jackin_telemetry::schema::enums::AppMode::OneShot,
    };

    #[must_use]
    pub const fn service_name(self) -> &'static str {
        self.service_name
    }

    #[must_use]
    pub const fn app_mode(self) -> jackin_telemetry::schema::enums::AppMode {
        self.app_mode
    }
}

pub fn init_tracing(debug: bool, run_id: &str) -> anyhow::Result<bool> {
    init_tracing_for(debug, run_id, ServiceIdentity::HOST_ONE_SHOT)
}

pub fn init_tracing_for(
    debug: bool,
    run_id: &str,
    identity: ServiceIdentity,
) -> anyhow::Result<bool> {
    jackin_telemetry::limits::install_redactor(crate::redact::redact_text);
    let env = |key: &str| std::env::var(key).ok();
    if let Some(config) = config::resolve_otlp_config(&env)? {
        let endpoints = otlp::OtlpEndpoints::from_config(&config);
        return match otlp::init(debug, run_id, identity, &endpoints) {
            Ok(()) => Ok(true),
            Err(error) => Err(error),
        };
    }

    // No fmt layer: the operator's terminal must never receive the firehose.
    let _ = (debug, run_id, identity);
    tracing_subscriber::registry()
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"))?;
    Ok(false)
}

/// Install real OTLP providers against an explicit test receiver endpoint.
#[cfg(feature = "test-support")]
pub fn init_wire_test_export(endpoint: &str, identity: ServiceIdentity) -> anyhow::Result<()> {
    let endpoints = otlp::OtlpEndpoints::new(endpoint, endpoint, endpoint);
    otlp::init(false, "wire-conformance", identity, &endpoints)
}

/// Force all three wire-test providers to deliver their current batches.
#[cfg(feature = "test-support")]
pub fn flush_wire_test_export() -> Result<(), ValidationFailure> {
    otlp::validate_flush()
}

#[cfg(feature = "test-support")]
#[doc(hidden)]
pub fn otlp_runtime_creation_count_for_test() -> u64 {
    otlp::runtime_creation_count()
}

#[cfg(feature = "test-support")]
#[doc(hidden)]
pub fn otlp_runtime_active_for_test() -> bool {
    otlp::runtime_is_active()
}

/// The first explicitly-requested OTLP protocol jackin cannot honor, when an
/// OTLP endpoint is configured (i.e. export is intended). `None` means the
/// configuration is exportable (grpc or unset) or no endpoint is set. Callers
/// use this to fail fast with a clear operator error before doing any work.
#[must_use]
pub fn unsupported_otlp_protocol() -> Option<String> {
    let env = |key: &str| std::env::var(key).ok();
    match config::resolve_otlp_config(&env) {
        Err(config::OtlpConfigError::UnsupportedProtocol { variable }) => Some(variable.to_owned()),
        _ => None,
    }
}

/// Flush and shut down the OTLP exporters, if any are active.
///
/// Batch exporters hold the tail of a run in memory; a run that exits without
/// this call silently drops its last spans, log records, and metrics. Invoked
/// from `ActiveRunGuard::drop` so it runs on every exit path out of the run —
/// including `?` error early-returns — rather than only the success path.
/// No-op when no endpoint was configured.
pub(crate) fn shutdown_otlp() {
    otlp::shutdown();
}

/// Flush and shut down the capsule's OTLP exporters at process exit. The public
/// counterpart to the host's guard-driven [`shutdown_otlp`]; the capsule has no
/// `ActiveRunGuard`, so it calls this explicitly before the daemon exits.
pub fn shutdown_capsule_tracing() {
    otlp::shutdown();
}

/// Install OTLP export for the in-container capsule process.
///
/// W3C trace context links the session back to the launch trace. Returns
/// `Ok(true)` when export was activated,
/// `Ok(false)` when no endpoint is configured (the common, no-op case).
pub fn init_capsule_tracing(traceparent: Option<&str>) -> anyhow::Result<bool> {
    jackin_telemetry::limits::install_redactor(crate::redact::redact_text);
    let env = |key: &str| std::env::var(key).ok();
    let activated = match config::resolve_otlp_config(&env)? {
        Some(config) => {
            otlp::init_capsule(traceparent, &config)?;
            true
        }
        None => false,
    };
    Ok(activated)
}

/// The configured host OTLP endpoint (`OTEL_EXPORTER_OTLP_ENDPOINT`), or `None`
/// when export is off / not compiled.
#[must_use]
pub fn configured_endpoint() -> Option<String> {
    otlp::base_endpoint()
}

/// Human-readable host OTLP endpoint configuration for debug banners.
#[must_use]
pub fn configured_endpoint_summary() -> Option<String> {
    otlp::endpoint_summary()
}

/// Operator-facing backend query line for an invocation id, when an OTLP endpoint is
/// configured. Returns `None` when export is off.
///
/// Renders `parallax run <id>` when the endpoint summary looks like the
/// Parallax reference backend; otherwise a backend-neutral
/// `cli.invocation.id=<id>` filter string.
#[must_use]
pub fn backend_query_hint(invocation_id: &str) -> Option<String> {
    let endpoint = configured_endpoint_summary()?;
    let query = if endpoint.to_ascii_lowercase().contains("parallax") {
        format!("parallax invocation {invocation_id}")
    } else {
        format!("query your OTLP backend for cli.invocation.id={invocation_id}")
    };
    Some(query)
}

/// Whether the operator set any OTLP endpoint env var (export intended), even if
/// the resulting config is incomplete and so installs no exporter. Lets the
/// caller surface "export configured but disabled" instead of silently treating
/// it as never requested. Always `false` without the `otlp` feature.
#[must_use]
pub fn otlp_endpoint_configured() -> bool {
    config::any_endpoint_configured(&|key| std::env::var(key).ok())
}

/// Whether host OTLP authentication material is configured. Values are never
/// returned so launch policy cannot accidentally copy them into a Capsule.
#[must_use]
pub fn otlp_auth_configured() -> bool {
    config::any_auth_configured(&|key| std::env::var(key).ok())
}

/// Effective, privacy-safe configuration for one OTLP signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtlpSignalFingerprint {
    pub authority: String,
    pub tls: bool,
}

/// Effective per-signal OTLP configuration without credentials or paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtlpConfigFingerprint {
    pub traces: OtlpSignalFingerprint,
    pub logs: OtlpSignalFingerprint,
    pub metrics: OtlpSignalFingerprint,
    pub compression: &'static str,
    pub sampler: &'static str,
}

/// Resolve and sanitize the same configuration used to build the providers.
pub fn resolved_otlp_config_fingerprint()
-> Result<Option<OtlpConfigFingerprint>, TelemetryConfigFailure> {
    let env = |key: &str| std::env::var(key).ok();
    config::resolve_otlp_config(&env)
        .map(|config| config.map(|config| OtlpConfigFingerprint::from_config(&config)))
        .map_err(Into::into)
}

impl OtlpConfigFingerprint {
    fn from_config(config: &config::OtlpConfig) -> Self {
        let signal = |endpoint: &str| OtlpSignalFingerprint {
            authority: endpoint_authority(endpoint).unwrap_or_default(),
            tls: endpoint.starts_with("https://"),
        };
        Self {
            traces: signal(&config.traces_endpoint),
            logs: signal(&config.logs_endpoint),
            metrics: signal(&config.metrics_endpoint),
            compression: "gzip",
            sampler: "parentbased_always_on",
        }
    }
}

fn endpoint_authority(endpoint: &str) -> Option<String> {
    let (_, rest) = endpoint.split_once("://")?;
    let authority = rest.split('/').next()?;
    (!authority.is_empty()).then(|| authority.to_owned())
}

/// How a launched container should reach the host OTLP backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerOtlp {
    /// Value for `OTEL_EXPORTER_OTLP_ENDPOINT` inside the container.
    pub endpoint: String,
    /// Whether the launch must add `host.docker.internal:host-gateway` so the
    /// rewritten loopback host resolves to the host on Linux engines.
    pub needs_host_gateway: bool,
}

/// The configured OTLP endpoint rewritten to be reachable from inside a
/// container. `None` when export is off.
#[must_use]
pub fn container_otlp() -> Option<ContainerOtlp> {
    container_endpoint().map(|endpoint| rewrite_endpoint_for_container(&endpoint))
}

/// The single endpoint to inject as the container's `OTEL_EXPORTER_OTLP_ENDPOINT`.
/// Prefers the base var; falls back to the resolved traces endpoint so a
/// per-signal-only host config (per-signal vars, no base) still gives the capsule
/// a reachable collector instead of silently disabling capsule export. gRPC sends
/// every signal to one target, so a single endpoint is the right container shape.
fn container_endpoint() -> Option<String> {
    otlp::container_endpoint()
}

/// Rewrite a host-loopback OTLP endpoint to `host.docker.internal` (the host
/// gateway), leaving any already-routable host untouched. Hand-rolled rather
/// than pulling a URL parser: the only transform is swapping a loopback
/// authority, and the input is jackin❯'s own `scheme://host[:port][/path]`.
fn rewrite_endpoint_for_container(endpoint: &str) -> ContainerOtlp {
    if let Some((scheme, rest)) = endpoint.split_once("://") {
        let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) if port.bytes().all(|b| b.is_ascii_digit()) => (host, Some(port)),
            _ => (authority, None),
        };
        if matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]") {
            let port = port.map(|port| format!(":{port}")).unwrap_or_default();
            let path = if path.is_empty() {
                String::new()
            } else {
                format!("/{path}")
            };
            return ContainerOtlp {
                endpoint: format!("{scheme}://host.docker.internal{port}{path}"),
                needs_host_gateway: true,
            };
        }
    }
    ContainerOtlp {
        endpoint: endpoint.to_owned(),
        needs_host_gateway: false,
    }
}

#[cfg(test)]
mod tests;

/// OTLP export: spans (stage timings + screen/launch traces), logs (the
/// diagnostics event stream), and process/runtime metrics to one endpoint.
/// Only compiled with `--features otlp`; entirely absent from default builds
/// so there is zero link-time cost. No `fmt` layer is attached: OTLP export is
/// a separate sink from the operator's screen, which stays free of the firehose.
mod otlp;

#[cfg(any(test, feature = "test-support"))]
pub use otlp::{TestExport, test_capsule_layers};
/// In-memory export rig for crate tests (operation facade, conformance).
#[cfg(test)]
pub(crate) use otlp::{emit_session_start_for_test, test_layers};

pub(crate) fn emit_progress_event(
    _invocation_id: &str,
    kind: &str,
    message: &str,
    _stage: Option<&str>,
    _detail: Option<&str>,
) {
    emit_progress_event_inner(kind, message, None);
}

pub(crate) fn emit_progress_error(
    _invocation_id: &str,
    kind: &str,
    message: &str,
    _stage: Option<&str>,
    _detail: Option<&str>,
) {
    emit_progress_event_inner(kind, message, Some("operation_error"));
}

pub(crate) fn emit_progress_error_typed(
    _invocation_id: &str,
    kind: &str,
    message: &str,
    _stage: Option<&str>,
    _detail: Option<&str>,
    error_type: Option<&str>,
) {
    emit_progress_event_inner(kind, message, error_type.or(Some("operation_error")));
}

fn emit_progress_event_inner(kind: &str, message: &str, error_type: Option<&str>) {
    use jackin_telemetry::event;
    use jackin_telemetry::{Attr, FieldSet, Value};

    // Launch stages use the shared paired guard, which owns their typed events,
    // span, and metrics in both rich and headless paths.
    if matches!(
        kind,
        "stage_started" | "stage_done" | "stage_failed" | "stage_skipped"
    ) {
        return;
    }

    let (def, outcome) = match kind {
        "timing_started" => (&event::TIMING_STARTED, "success"),
        "timing_done" => (&event::TIMING_DONE, "success"),
        "debug" => (&event::DEBUG_LINE, "success"),
        "subprocess_done" => (
            &event::PROCESS_SUBPROCESS_DONE,
            if error_type.is_some() {
                "failure"
            } else {
                "success"
            },
        ),
        "run_summary" => (&event::RUN_SUMMARY, "success"),
        "slow_foreground_wait" => (&event::PERFORMANCE_SLOW_FOREGROUND_WAIT, "success"),
        "session_detach" => (&event::CAPSULE_SESSION_DETACH, "cancellation"),
        "clean_shutdown" => (&event::CAPSULE_SESSION_CLEAN_SHUTDOWN, "success"),
        _ => (&event::ERROR_TYPED, "failure"),
    };
    let message = crate::redact::redact_text(message);
    let mut attrs = vec![Attr {
        key: jackin_telemetry::schema::attrs::OUTCOME,
        value: Value::Str(outcome),
    }];
    if let Some(error_type) = error_type {
        attrs.push(Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
            value: Value::Str(error_type),
        });
    }
    let _event_result =
        jackin_telemetry::emit_event(def, FieldSet::new(&attrs, Some(message.as_ref())));
}

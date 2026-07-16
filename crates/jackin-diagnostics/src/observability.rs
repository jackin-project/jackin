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
mod otlp {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
    use opentelemetry_otlp::{Compression, WithExportConfig, WithTonicConfig};
    use opentelemetry_sdk::Resource;
    use opentelemetry_sdk::logs::BatchConfigBuilder as LogBatchConfigBuilder;
    use opentelemetry_sdk::logs::SdkLoggerProvider;
    use opentelemetry_sdk::logs::log_processor_with_async_runtime::BatchLogProcessor;
    use opentelemetry_sdk::metrics::SdkMeterProvider;
    use opentelemetry_sdk::metrics::periodic_reader_with_async_runtime::PeriodicReader;
    use opentelemetry_sdk::runtime::Tokio;
    use opentelemetry_sdk::trace::span_processor_with_async_runtime::BatchSpanProcessor;
    use opentelemetry_sdk::trace::{
        BatchConfigBuilder as SpanBatchConfigBuilder, Sampler, SdkTracerProvider,
    };
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::prelude::*;

    use super::ServiceIdentity;
    use super::health;
    use super::resource::build_resource_for;
    #[cfg(test)]
    use super::resource::{
        build_resource_for_sources, container_id_from_cgroup, semantic_os_type,
        verified_container_id,
    };
    mod governance;
    mod metric_governance;
    mod retry;
    use governance::{GovernedLogProcessor, GovernedSpanProcessor};
    use metric_governance::validate_metric_export;
    #[cfg(test)]
    use metric_governance::{
        metric_contract_fields, validate_metric_attributes, validate_metric_points,
    };

    const EXPORT_ATTEMPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);

    /// The three SDK providers for one run, flushed together at shutdown.
    /// Named (not a positional tuple) so the flush sequence can't transpose
    /// tracer/logger/meter — all three expose identical `force_flush`/`shutdown`
    /// signatures, so a tuple destructure in the wrong order would compile
    /// silently. All three providers are required and activated atomically.
    struct OtlpProviders {
        tracer: SdkTracerProvider,
        logger: SdkLoggerProvider,
        meter: SdkMeterProvider,
        generation: u64,
    }

    impl OtlpProviders {
        /// Flush buffered telemetry, then shut the exporters down. Called once,
        /// from `ActiveRunGuard::drop`, on every run exit path.
        ///
        /// A `force_flush` failure is the authoritative "the backend did not
        /// receive this run" signal — the SDK surfaces a failed export through
        /// this `Result`, not (reliably) through a `tracing` event. So a flush
        /// error emits one compact operator notice (stderr / deferred under a
        /// rich TUI) rather than being dropped; otherwise an unreachable or
        /// wrong-protocol backend would fail completely silently. `shutdown`
        /// errors stay quiet — by then the data is already flushed-or-lost and a
        /// second notice adds only noise.
        fn flush_and_shutdown(&self, deadline: std::time::Instant) -> bool {
            let (trace_flush, log_flush, metric_flush) = self.force_flush_all(deadline);
            #[cfg(test)]
            SHUTDOWN_ORDER
                .lock()
                .expect("shutdown order lock")
                .push("tracer");
            let tracer = self.tracer.clone();
            let trace_shutdown = sdk_operation_before(deadline, move |timeout| {
                tracer.shutdown_with_timeout(timeout)
            });
            #[cfg(test)]
            SHUTDOWN_ORDER
                .lock()
                .expect("shutdown order lock")
                .push("logger");
            let logger = self.logger.clone();
            let log_shutdown = sdk_operation_before(deadline, move |timeout| {
                logger.shutdown_with_timeout(timeout)
            });
            #[cfg(test)]
            SHUTDOWN_ORDER
                .lock()
                .expect("shutdown order lock")
                .push("meter");
            let meter = self.meter.clone();
            let metric_shutdown = sdk_operation_before(deadline, move |timeout| {
                meter.shutdown_with_timeout(timeout)
            });
            let failed = trace_flush
                .err()
                .or_else(|| log_flush.err())
                .or_else(|| metric_flush.err());
            let flushed = failed.is_none();
            health::record_flush(self.generation, flushed);
            if failed.is_some() {
                // Direct to stderr, not the deferred buffer: this fires at final
                // teardown where the run guard may outlive the terminal session,
                // so a buffered notice could never be drained. The TUI is already
                // gone by now, so stderr can't corrupt it.
                crate::logging::emit_teardown_notice(
                    "telemetry export failed to reach the backend (run telemetry may be incomplete)",
                );
            }
            flushed && trace_shutdown.is_ok() && log_shutdown.is_ok() && metric_shutdown.is_ok()
        }

        fn force_flush_all(
            &self,
            deadline: std::time::Instant,
        ) -> (Result<(), String>, Result<(), String>, Result<(), String>) {
            #[cfg(test)]
            SHUTDOWN_ORDER.lock().expect("shutdown order lock").extend([
                "flush.tracer",
                "flush.logger",
                "flush.meter",
            ]);
            let tracer = self.tracer.clone();
            let logger = self.logger.clone();
            let meter = self.meter.clone();
            let traces = FlushTask::spawn(move || tracer.force_flush());
            let logs = FlushTask::spawn(move || logger.force_flush());
            let metrics = FlushTask::spawn(move || meter.force_flush());
            (
                traces.finish_before(deadline),
                logs.finish_before(deadline),
                metrics.finish_before(deadline),
            )
        }
    }

    struct FlushTask {
        receiver: std::sync::mpsc::Receiver<opentelemetry_sdk::error::OTelSdkResult>,
        handle: std::thread::JoinHandle<()>,
    }

    impl FlushTask {
        fn spawn(
            operation: impl FnOnce() -> opentelemetry_sdk::error::OTelSdkResult + Send + 'static,
        ) -> Self {
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            let handle = jackin_telemetry::spawn::thread_joined(move || {
                drop(sender.send(operation()));
            });
            Self { receiver, handle }
        }

        fn finish_before(self, deadline: std::time::Instant) -> Result<(), String> {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                retain_flush_worker(self.handle);
                return Err("telemetry flush budget exhausted".to_owned());
            }
            match self.receiver.recv_timeout(remaining) {
                Ok(result) => {
                    self.handle
                        .join()
                        .map_err(|_| "telemetry flush worker panicked".to_owned())?;
                    result.map_err(|_| "telemetry flush failed".to_owned())
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    retain_flush_worker(self.handle);
                    Err("telemetry flush budget exhausted".to_owned())
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => self
                    .handle
                    .join()
                    .map_err(|_| "telemetry flush worker panicked".to_owned())
                    .and(Err("telemetry flush failed".to_owned())),
            }
        }
    }

    fn sdk_operation_before<F>(deadline: std::time::Instant, operation: F) -> Result<(), String>
    where
        F: FnOnce(std::time::Duration) -> opentelemetry_sdk::error::OTelSdkResult + Send + 'static,
    {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err("telemetry shutdown budget exhausted".to_owned());
        }
        FlushTask::spawn(move || operation(remaining))
            .finish_before(deadline)
            .map_err(|error| {
                if error == "telemetry flush failed" {
                    "telemetry shutdown failed".to_owned()
                } else {
                    error
                }
            })
    }

    #[cfg(test)]
    fn flush_before<F>(deadline: std::time::Instant, operation: F) -> Result<(), String>
    where
        F: FnOnce() -> opentelemetry_sdk::error::OTelSdkResult,
    {
        if std::time::Instant::now() >= deadline {
            return Err("telemetry shutdown budget exhausted".to_owned());
        }
        operation().map_err(|_| "telemetry flush failed".to_owned())
    }

    pub(super) fn validate_flush() -> Result<(), super::ValidationFailure> {
        let providers = PROVIDERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let providers = providers
            .as_ref()
            .ok_or(super::ValidationFailure::Inactive)?;
        let (trace, logs, metrics) = providers
            .force_flush_all(std::time::Instant::now() + std::time::Duration::from_secs(5));
        health::record_flush(
            providers.generation,
            trace.is_ok() && logs.is_ok() && metrics.is_ok(),
        );
        validate_flush_results(&trace, &logs, &metrics)
    }

    fn validate_flush_results(
        trace: &Result<(), String>,
        logs: &Result<(), String>,
        metrics: &Result<(), String>,
    ) -> Result<(), super::ValidationFailure> {
        if [trace, logs, metrics].into_iter().any(|result| {
            result
                .as_ref()
                .is_err_and(|error| error.contains("budget exhausted"))
        }) {
            return Err(super::ValidationFailure::Timeout);
        }
        if trace.is_err() {
            return Err(super::ValidationFailure::Export("traces"));
        }
        if logs.is_err() {
            return Err(super::ValidationFailure::Export("logs"));
        }
        if metrics.is_err() {
            return Err(super::ValidationFailure::Export("metrics"));
        }
        Ok(())
    }

    static ACTIVATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static PROVIDERS: std::sync::Mutex<Option<OtlpProviders>> = std::sync::Mutex::new(None);
    static PENDING_FLUSH_WORKERS: std::sync::Mutex<Vec<std::thread::JoinHandle<()>>> =
        std::sync::Mutex::new(Vec::new());
    #[cfg(test)]
    static SHUTDOWN_ORDER: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());

    /// Dedicated multi-thread tokio runtime that drives OTLP export. Held for the
    /// process lifetime so the async-runtime batch processors (and tonic's h2
    /// connection driver) have a reactor decoupled from jackin❯'s current-thread
    /// main: the `futures_executor::block_on` flush parks the main thread, and
    /// these worker threads keep exporting regardless. One worker is plenty for
    /// a single run's telemetry volume.
    static OTEL_RUNTIME: std::sync::Mutex<Option<tokio::runtime::Runtime>> =
        std::sync::Mutex::new(None);
    #[cfg(any(test, feature = "test-support"))]
    static OTEL_RUNTIME_CREATIONS: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);

    /// Build-or-get the dedicated telemetry runtime. Providers must be built
    /// inside its [`tokio::runtime::Runtime::enter`] guard so their workers spawn
    /// onto it rather than the ambient (current-thread) app runtime.
    fn otel_runtime()
    -> anyhow::Result<std::sync::MutexGuard<'static, Option<tokio::runtime::Runtime>>> {
        let mut runtime = OTEL_RUNTIME
            .lock()
            .map_err(|_| anyhow::anyhow!("OTLP telemetry runtime lock poisoned"))?;
        if runtime.is_none() {
            *runtime = Some(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(1)
                    .enable_all()
                    .thread_name("jackin-otel")
                    .build()
                    .map_err(|e| anyhow::anyhow!("OTLP telemetry runtime init failed: {e}"))?,
            );
            #[cfg(any(test, feature = "test-support"))]
            OTEL_RUNTIME_CREATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(runtime)
    }

    fn rollback_runtime() {
        if let Some(runtime) = OTEL_RUNTIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            runtime.shutdown_background();
        }
    }

    fn ensure_inactive() -> anyhow::Result<()> {
        if PROVIDERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
            || OTEL_RUNTIME
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_some()
        {
            anyhow::bail!("OTLP providers are already active");
        }
        Ok(())
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(super) struct OtlpEndpoints {
        traces: String,
        logs: String,
        metrics: String,
        traces_timeout: std::time::Duration,
        logs_timeout: std::time::Duration,
        metrics_timeout: std::time::Duration,
        traces_tls: super::config::TlsConfig,
        logs_tls: super::config::TlsConfig,
        metrics_tls: super::config::TlsConfig,
    }

    impl OtlpEndpoints {
        pub(super) fn from_config(config: &super::config::OtlpConfig) -> Self {
            Self {
                traces: config.traces_endpoint.clone(),
                logs: config.logs_endpoint.clone(),
                metrics: config.metrics_endpoint.clone(),
                traces_timeout: config.traces_timeout,
                logs_timeout: config.logs_timeout,
                metrics_timeout: config.metrics_timeout,
                traces_tls: config.traces_tls.clone(),
                logs_tls: config.logs_tls.clone(),
                metrics_tls: config.metrics_tls.clone(),
            }
        }

        /// The per-signal endpoints a single base produces. OTLP/gRPC sends every
        /// signal to the same endpoint verbatim and routes by gRPC service name,
        /// so — unlike OTLP/HTTP — no `/v1/<signal>` path is appended and all
        /// three share `base`.
        fn from_base(base: &str) -> Self {
            Self::new(base, base, base)
        }

        /// The one construction choke point. Every field is run through
        /// [`grpc_endpoint`] here so the "normalized gRPC channel target"
        /// invariant has a single enforcement site rather than being re-asserted
        /// at each caller (where one could silently drift).
        pub(super) fn new(traces: &str, logs: &str, metrics: &str) -> Self {
            Self {
                traces: grpc_endpoint(traces),
                logs: grpc_endpoint(logs),
                metrics: grpc_endpoint(metrics),
                traces_timeout: std::time::Duration::from_secs(5),
                logs_timeout: std::time::Duration::from_secs(5),
                metrics_timeout: std::time::Duration::from_secs(5),
                traces_tls: super::config::TlsConfig::default(),
                logs_tls: super::config::TlsConfig::default(),
                metrics_tls: super::config::TlsConfig::default(),
            }
        }
    }

    /// Host OTLP endpoints, when configured via the standard OTLP env vars.
    /// `OTEL_EXPORTER_OTLP_ENDPOINT` provides a base for every signal; the
    /// per-signal endpoint vars wrappers commonly inject override it per signal.
    pub(super) fn endpoints() -> Option<OtlpEndpoints> {
        let env = |key: &str| std::env::var(key).ok();
        super::config::resolve_otlp_config(&env)
            .ok()
            .flatten()
            .map(|config| OtlpEndpoints::from_config(&config))
    }

    fn validate_standard_env() -> anyhow::Result<()> {
        if let Ok(sampler) = std::env::var("OTEL_TRACES_SAMPLER")
            && !sampler.trim().is_empty()
            && sampler.trim() != "parentbased_always_on"
        {
            anyhow::bail!("OTEL_TRACES_SAMPLER conflicts with required parentbased_always_on");
        }
        for var in [
            "OTEL_EXPORTER_OTLP_COMPRESSION",
            "OTEL_EXPORTER_OTLP_TRACES_COMPRESSION",
            "OTEL_EXPORTER_OTLP_LOGS_COMPRESSION",
            "OTEL_EXPORTER_OTLP_METRICS_COMPRESSION",
        ] {
            if let Ok(value) = std::env::var(var)
                && !value.trim().is_empty()
                && value.trim() != "gzip"
            {
                anyhow::bail!("{var} is unsupported; expected gzip");
            }
        }
        for var in [
            "OTEL_EXPORTER_OTLP_TIMEOUT",
            "OTEL_EXPORTER_OTLP_TRACES_TIMEOUT",
            "OTEL_EXPORTER_OTLP_LOGS_TIMEOUT",
            "OTEL_EXPORTER_OTLP_METRICS_TIMEOUT",
        ] {
            if let Ok(value) = std::env::var(var) {
                let _: u64 = value
                    .trim()
                    .parse()
                    .map_err(|_| anyhow::anyhow!("{var} must be an integer millisecond timeout"))?;
            }
        }
        Ok(())
    }

    /// The endpoint handed to a launched container. The base var wins; absent it,
    /// the resolved traces endpoint stands in so a per-signal-only host config
    /// still reaches the capsule. Both are already `grpc_endpoint`-normalized.
    pub(super) fn container_endpoint() -> Option<String> {
        base_endpoint().or_else(|| endpoints().map(|endpoints| endpoints.traces))
    }

    pub(super) fn base_endpoint() -> Option<String> {
        let endpoint = resolve_endpoint(std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())?;
        super::config::normalize_endpoint(endpoint, "base").ok()
    }

    pub(super) fn endpoint_summary() -> Option<String> {
        let endpoints = endpoints()?;
        // A single configured base drives all three signal URLs, so collapse to
        // it; per-signal overrides break the match and are spelled out in full.
        if let Some(base) = base_endpoint()
            && endpoints == OtlpEndpoints::from_base(&base)
        {
            return sanitized_authority(&base);
        }
        Some(format!(
            "traces={}, logs={}, metrics={}",
            sanitized_authority(&endpoints.traces)?,
            sanitized_authority(&endpoints.logs)?,
            sanitized_authority(&endpoints.metrics)?,
        ))
    }

    fn sanitized_authority(endpoint: &str) -> Option<String> {
        let endpoint = url::Url::parse(endpoint).ok()?;
        let host = endpoint.host_str()?;
        let host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.to_owned()
        };
        let port = endpoint
            .port()
            .map_or_else(String::new, |port| format!(":{port}"));
        Some(format!("{}://{host}{port}", endpoint.scheme()))
    }

    /// The configured base endpoint, if any. An exported-but-empty var must not
    /// produce a blank endpoint, so an empty value resolves to `None` and no
    /// OTLP layer is installed.
    fn resolve_endpoint(otel: Option<String>) -> Option<String> {
        otel.filter(|s| !s.is_empty())
    }

    /// Normalize a gRPC endpoint: strip trailing slashes. The OTLP/gRPC exporter
    /// uses the endpoint as the channel target (`http://host:4317`) and routes by
    /// gRPC service name, so — unlike OTLP/HTTP — no signal path is appended.
    fn grpc_endpoint(endpoint: &str) -> String {
        endpoint.trim_end_matches('/').to_owned()
    }

    /// Whether an explicit `OTEL_EXPORTER_OTLP_*_PROTOCOL` value names something
    /// jackin cannot send. jackin exports OTLP over gRPC only; an empty value or
    /// `grpc` is fine, anything else (`http/protobuf`, `http/json`, …) is not.
    fn unsupported_protocol(value: &str) -> bool {
        let value = value.trim();
        !value.is_empty() && value != "grpc"
    }

    /// The standard OTLP protocol-selection env vars (generic + per-signal). The
    /// protocol guard and the fatal startup check both scan this one list so they
    /// can never drift — a new per-signal var missed by one but not the other
    /// would silently re-open the wrong-protocol no-deliver hole.
    const PROTOCOL_VARS: [&str; 4] = [
        "OTEL_EXPORTER_OTLP_PROTOCOL",
        "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL",
        "OTEL_EXPORTER_OTLP_LOGS_PROTOCOL",
        "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL",
    ];

    /// jackin exports OTLP over gRPC only. If a non-grpc protocol is explicitly
    /// requested via the standard env vars, fail loudly here rather than build a
    /// gRPC exporter against an endpoint meant for HTTP — a silent no-deliver is
    /// exactly the failure mode this guards against.
    fn ensure_grpc_protocol() -> Result<(), String> {
        for var in PROTOCOL_VARS {
            if let Ok(value) = std::env::var(var)
                && unsupported_protocol(&value)
            {
                return Err(format!(
                    "{var}={} is not supported — jackin exports OTLP over grpc only",
                    value.trim()
                ));
            }
        }
        Ok(())
    }

    /// Shared OTLP tracer/logger provider construction for host and capsule.
    ///
    /// Owns the protocol check, the dedicated telemetry runtime enter-guard, and
    /// both exporters + batch-processor providers so host/`init_capsule` cannot
    /// drift. Callers differ only in resource, endpoints, layer composition, and
    /// metrics handling. Returns the app runtime handle captured *before*
    /// entering the telemetry runtime (for tokio gauges).
    fn build_otlp_providers(
        resource: Resource,
        endpoints: &OtlpEndpoints,
    ) -> anyhow::Result<(
        SdkTracerProvider,
        SdkLoggerProvider,
        Option<tokio::runtime::Handle>,
    )> {
        ensure_grpc_protocol().map_err(|e| anyhow::anyhow!(e))?;
        validate_standard_env()?;
        let runtime = otel_runtime()?;
        // The tokio runtime gauges must report jackin❯'s app runtime, not the
        // dedicated telemetry runtime — capture its handle before entering ours.
        let app_handle = tokio::runtime::Handle::try_current().ok();
        // Build every exporter, processor, and reader inside the dedicated
        // runtime: the async-runtime processors spawn their worker tasks (and
        // tonic spawns its h2 connection driver) onto whichever runtime is
        // entered here, and they must land on the multi-thread telemetry runtime
        // — not jackin❯'s current-thread main, where flush would deadlock.
        let runtime = runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("telemetry runtime was not initialized"))?;
        let _runtime_guard = runtime.enter();
        let mut span_builder = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoints.traces.clone())
            .with_timeout(endpoints.traces_timeout)
            .with_compression(Compression::Gzip)
            .with_retry_policy(retry::policy());
        if let Some(tls) = exporter_tls(
            &endpoints.traces_tls,
            "traces",
            &endpoints.traces,
            endpoints.traces_timeout,
        )? {
            span_builder = span_builder.with_tls_config(tls);
        }
        let span_exporter = span_builder
            .build()
            .map_err(|_| anyhow::anyhow!("OTLP span exporter init failed"))?;
        let mut log_builder = opentelemetry_otlp::LogExporter::builder()
            .with_tonic()
            .with_endpoint(endpoints.logs.clone())
            .with_timeout(endpoints.logs_timeout)
            .with_compression(Compression::Gzip)
            .with_retry_policy(retry::policy());
        if let Some(tls) = exporter_tls(
            &endpoints.logs_tls,
            "logs",
            &endpoints.logs,
            endpoints.logs_timeout,
        )? {
            log_builder = log_builder.with_tls_config(tls);
        }
        let log_exporter = log_builder
            .build()
            .map_err(|_| anyhow::anyhow!("OTLP log exporter init failed"))?;

        // Attribute limits: generous but finite (observed max attrs + headroom).
        // Prevents unbounded dimension growth; DroppedAttributesCount must stay 0.
        let span_batch = SpanBatchConfigBuilder::default()
            .with_max_queue_size(2_048)
            .with_max_export_batch_size(512)
            .with_scheduled_delay(std::time::Duration::from_secs(1))
            .with_max_export_timeout(EXPORT_ATTEMPT_TIMEOUT)
            .build();
        let log_batch = LogBatchConfigBuilder::default()
            .with_max_queue_size(4_096)
            .with_max_export_batch_size(512)
            .with_scheduled_delay(std::time::Duration::from_secs(1))
            .with_max_export_timeout(EXPORT_ATTEMPT_TIMEOUT)
            .build();
        let tracer_provider = SdkTracerProvider::builder()
            .with_sampler(Sampler::ParentBased(Box::new(Sampler::AlwaysOn)))
            .with_max_attributes_per_span(64)
            .with_max_attributes_per_event(32)
            .with_span_processor(GovernedSpanProcessor(
                BatchSpanProcessor::builder(CountingSpanExporter(span_exporter), Tokio)
                    .with_batch_config(span_batch)
                    .build(),
            ))
            .with_resource(resource.clone())
            .build();
        let logger_provider = SdkLoggerProvider::builder()
            .with_log_processor(GovernedLogProcessor(
                BatchLogProcessor::builder(CountingLogExporter(log_exporter), Tokio)
                    .with_batch_config(log_batch)
                    .build(),
            ))
            .with_resource(resource)
            .build();
        Ok((tracer_provider, logger_provider, app_handle))
    }

    #[derive(Debug)]
    struct CountingSpanExporter(opentelemetry_otlp::SpanExporter);

    impl opentelemetry_sdk::trace::SpanExporter for CountingSpanExporter {
        async fn export(
            &self,
            batch: Vec<opentelemetry_sdk::trace::SpanData>,
        ) -> opentelemetry_sdk::error::OTelSdkResult {
            let result = self.0.export(batch).await;
            health::record_signal_export(health::Signal::Traces, result.is_ok());
            result
        }

        fn shutdown_with_timeout(
            &self,
            timeout: std::time::Duration,
        ) -> opentelemetry_sdk::error::OTelSdkResult {
            self.0.shutdown_with_timeout(timeout)
        }

        fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
            self.0.force_flush()
        }

        fn set_resource(&mut self, resource: &Resource) {
            self.0.set_resource(resource);
        }
    }

    #[derive(Debug)]
    struct CountingLogExporter(opentelemetry_otlp::LogExporter);

    impl opentelemetry_sdk::logs::LogExporter for CountingLogExporter {
        async fn export(
            &self,
            batch: opentelemetry_sdk::logs::LogBatch<'_>,
        ) -> opentelemetry_sdk::error::OTelSdkResult {
            let result = self.0.export(batch).await;
            health::record_signal_export(health::Signal::Logs, result.is_ok());
            result
        }

        fn shutdown_with_timeout(
            &self,
            timeout: std::time::Duration,
        ) -> opentelemetry_sdk::error::OTelSdkResult {
            self.0.shutdown_with_timeout(timeout)
        }

        fn event_enabled(
            &self,
            level: opentelemetry::logs::Severity,
            target: &str,
            name: Option<&str>,
        ) -> bool {
            self.0.event_enabled(level, target, name)
        }

        fn set_resource(&mut self, resource: &Resource) {
            self.0.set_resource(resource);
        }
    }

    #[derive(Debug)]
    struct CountingMetricExporter(opentelemetry_otlp::MetricExporter);

    impl opentelemetry_sdk::metrics::exporter::PushMetricExporter for CountingMetricExporter {
        async fn export(
            &self,
            metrics: &opentelemetry_sdk::metrics::data::ResourceMetrics,
        ) -> opentelemetry_sdk::error::OTelSdkResult {
            if let Err(reason) = validate_metric_export(metrics) {
                jackin_telemetry::record_export_rejection(jackin_telemetry::Signal::Metric, reason);
                return Ok(());
            }
            let result = self.0.export(metrics).await;
            health::record_signal_export(health::Signal::Metrics, result.is_ok());
            result
        }

        fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
            self.0.force_flush()
        }

        fn shutdown_with_timeout(
            &self,
            timeout: std::time::Duration,
        ) -> opentelemetry_sdk::error::OTelSdkResult {
            self.0.shutdown_with_timeout(timeout)
        }

        fn temporality(&self) -> opentelemetry_sdk::metrics::Temporality {
            self.0.temporality()
        }
    }

    fn exporter_tls(
        config: &super::config::TlsConfig,
        signal: &'static str,
        endpoint: &str,
        timeout: std::time::Duration,
    ) -> anyhow::Result<Option<opentelemetry_otlp::tonic_types::transport::ClientTlsConfig>> {
        use opentelemetry_otlp::tonic_types::transport::{Certificate, ClientTlsConfig, Identity};

        if !endpoint.starts_with("https://")
            && config.certificate.is_none()
            && config.client_key.is_none()
            && config.client_certificate.is_none()
        {
            return Ok(None);
        }
        let mut tls = ClientTlsConfig::new().with_enabled_roots().timeout(timeout);
        if let Some(path) = &config.certificate {
            let pem = std::fs::read(path).map_err(|error| {
                anyhow::anyhow!("failed to read OTLP {signal} CA certificate: {error}")
            })?;
            tls = tls.ca_certificate(Certificate::from_pem(pem));
        }
        if let (Some(certificate), Some(key)) = (&config.client_certificate, &config.client_key) {
            let certificate = std::fs::read(certificate).map_err(|error| {
                anyhow::anyhow!("failed to read OTLP {signal} client certificate: {error}")
            })?;
            let key = std::fs::read(key).map_err(|error| {
                anyhow::anyhow!("failed to read OTLP {signal} client key: {error}")
            })?;
            tls = tls.identity(Identity::from_pem(certificate, key));
        }
        Ok(Some(tls))
    }

    pub(super) fn init(
        debug: bool,
        _run_id: &str,
        identity: ServiceIdentity,
        endpoints: &OtlpEndpoints,
    ) -> anyhow::Result<()> {
        let _activation = ACTIVATION_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_inactive()?;
        let resource = build_resource_for(identity);
        let (tracer_provider, logger_provider, app_handle) =
            match build_otlp_providers(resource.clone(), endpoints) {
                Ok(providers) => providers,
                Err(error) => {
                    rollback_runtime();
                    return Err(error);
                }
            };
        let meter_provider = match init_metrics(&resource, endpoints, app_handle) {
            Ok(provider) => provider,
            Err(error) => {
                cleanup_partial(&tracer_provider, &logger_provider, None);
                rollback_runtime();
                return Err(error);
            }
        };
        use opentelemetry::metrics::MeterProvider as _;
        let meter_reservation =
            match jackin_telemetry::reserve_meter(&meter_provider.meter("jackin")) {
                Ok(reservation) => reservation,
                Err(error) => {
                    cleanup_partial(&tracer_provider, &logger_provider, Some(&meter_provider));
                    rollback_runtime();
                    return Err(error.into());
                }
            };

        let tracer = tracer_provider.tracer("jackin");
        let span_layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_location(false)
            .with_threads(false)
            .with_target(false)
            .with_tracked_inactivity(false)
            .with_error_records_to_exceptions(false)
            .with_error_events_to_status(false)
            .with_error_fields_to_exceptions(false);
        let log_layer = OpenTelemetryTracingBridge::new(&logger_provider);

        // Scope the export to jackin❯'s own telemetry. Dependency-internal
        // spans/logs stay out of OTLP unless the operator asks for them with
        // `JACKIN_OTEL_INTERNAL=1`.
        let span_directive =
            export_filter_directive(export_level_for(crate::TelemetrySink::OtlpSpans, debug));
        let log_directive =
            export_filter_directive(export_level_for(crate::TelemetrySink::OtlpLogs, debug));
        let installed = tracing_subscriber::registry()
            .with(
                span_layer
                    .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                        metadata.is_span()
                    }))
                    .with_filter(EnvFilter::new(span_directive)),
            )
            .with(
                log_layer
                    .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                        metadata.is_event()
                    }))
                    .with_filter(EnvFilter::new(log_directive)),
            )
            .try_init()
            .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"));
        if installed.is_ok() {
            if let Err(error) = meter_reservation.commit() {
                cleanup_partial(&tracer_provider, &logger_provider, Some(&meter_provider));
                rollback_runtime();
                return Err(error.into());
            }
            let generation = health::set_active_signals();
            *PROVIDERS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(OtlpProviders {
                tracer: tracer_provider,
                logger: logger_provider,
                meter: meter_provider,
                generation,
            });
        } else {
            drop(meter_reservation);
            cleanup_partial(&tracer_provider, &logger_provider, Some(&meter_provider));
            rollback_runtime();
        }
        installed
    }

    /// Install OTLP export for the capsule. Mirrors `init` but composes no
    /// direct OTLP layers and stamps the
    /// capsule resource; providers come from [`build_otlp_providers`].
    pub(super) fn init_capsule(
        _traceparent: Option<&str>,
        config: &super::config::OtlpConfig,
    ) -> anyhow::Result<()> {
        let _activation = ACTIVATION_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ensure_inactive()?;
        let resource = build_resource_for(ServiceIdentity::CAPSULE);
        let endpoints = OtlpEndpoints::from_config(config);
        let (tracer_provider, logger_provider, app_handle) =
            match build_otlp_providers(resource.clone(), &endpoints) {
                Ok(providers) => providers,
                Err(error) => {
                    rollback_runtime();
                    return Err(error);
                }
            };
        let meter_provider = match init_metrics(&resource, &endpoints, app_handle) {
            Ok(provider) => provider,
            Err(error) => {
                cleanup_partial(&tracer_provider, &logger_provider, None);
                rollback_runtime();
                return Err(error);
            }
        };
        use opentelemetry::metrics::MeterProvider as _;
        let meter_reservation =
            match jackin_telemetry::reserve_meter(&meter_provider.meter("jackin")) {
                Ok(reservation) => reservation,
                Err(error) => {
                    cleanup_partial(&tracer_provider, &logger_provider, Some(&meter_provider));
                    rollback_runtime();
                    return Err(error.into());
                }
            };

        let tracer = tracer_provider.tracer("jackin");
        let span_layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_location(false)
            .with_threads(false)
            .with_target(false)
            .with_tracked_inactivity(false)
            .with_error_records_to_exceptions(false)
            .with_error_events_to_status(false)
            .with_error_fields_to_exceptions(false);
        let log_layer = OpenTelemetryTracingBridge::new(&logger_provider);

        let span_directive = export_filter_directive(export_level_for(
            crate::TelemetrySink::OtlpSpans,
            capsule_debug(),
        ));
        let log_directive = export_filter_directive(export_level_for(
            crate::TelemetrySink::OtlpLogs,
            capsule_debug(),
        ));
        let installed = tracing_subscriber::registry()
            .with(
                span_layer
                    .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                        metadata.is_span()
                    }))
                    .with_filter(EnvFilter::new(span_directive)),
            )
            .with(
                log_layer
                    .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                        metadata.is_event()
                    }))
                    .with_filter(EnvFilter::new(log_directive)),
            )
            .try_init()
            .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"));
        if installed.is_ok() {
            if let Err(error) = meter_reservation.commit() {
                cleanup_partial(&tracer_provider, &logger_provider, Some(&meter_provider));
                rollback_runtime();
                return Err(error.into());
            }
            let generation = health::set_active_signals();
            *PROVIDERS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(OtlpProviders {
                tracer: tracer_provider,
                logger: logger_provider,
                meter: meter_provider,
                generation,
            });
        } else {
            drop(meter_reservation);
            cleanup_partial(&tracer_provider, &logger_provider, Some(&meter_provider));
            rollback_runtime();
        }
        installed
    }

    fn cleanup_partial(
        tracer: &SdkTracerProvider,
        logger: &SdkLoggerProvider,
        meter: Option<&SdkMeterProvider>,
    ) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let tracer = tracer.clone();
        drop(sdk_operation_before(deadline, move |timeout| {
            tracer.shutdown_with_timeout(timeout)
        }));
        let logger = logger.clone();
        drop(sdk_operation_before(deadline, move |timeout| {
            logger.shutdown_with_timeout(timeout)
        }));
        if let Some(meter) = meter {
            let meter = meter.clone();
            drop(sdk_operation_before(deadline, move |timeout| {
                meter.shutdown_with_timeout(timeout)
            }));
        }
    }

    /// Capsule OTLP filter debug gate — uses the shared telemetry resolver
    /// rather than parsing telemetry controls privately.
    fn capsule_debug() -> bool {
        matches!(
            crate::telemetry_level(false),
            crate::TelemetryLevel::Debug | crate::TelemetryLevel::Trace
        )
    }

    /// Tracing targets exported over OTLP. Global default is `off`: a
    /// dependency that starts emitting `tracing` data must be added here
    /// deliberately instead of leaking into the backend.
    const EXPORT_TARGETS: &[&str] = &[
        "jackin",
        "jackin_build_meta",
        "jackin_capsule",
        "jackin_config",
        "jackin_console",
        "jackin_core",
        "jackin_dev",
        "jackin_diagnostics",
        "jackin_diagnostics::session",
        "jackin_docker",
        "jackin_env",
        "jackin_host",
        "jackin_image",
        "jackin_instance",
        "jackin_isolation",
        "jackin_launch_tui",
        "jackin_manifest",
        "jackin_pr_trailers",
        "jackin_protocol",
        "jackin_telemetry",
        "jackin_runtime",
        "jackin_term",
        jackin_telemetry::TELEMETRY_TARGET,
        "termrock",
        "termrock_lookbook",
        "jackin_usage",
    ];

    fn export_filter_directive(level: &str) -> String {
        export_filter_directive_with_internal(
            level,
            std::env::var("JACKIN_OTEL_INTERNAL").is_ok_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            }),
        )
    }

    fn export_level_for(sink: crate::TelemetrySink, debug: bool) -> &'static str {
        crate::telemetry_level_name(crate::sink_level(sink, debug))
    }

    fn export_filter_directive_with_internal(level: &str, internal: bool) -> String {
        let mut directive = String::from("off");
        for target in EXPORT_TARGETS {
            directive.push_str(&format!(",{target}={level}"));
        }
        if internal {
            // Operator explicitly asked for dependency internals: restore the
            // global default level while still blocking exporter feedback loops.
            directive.push_str(&format!(
                ",{level},hyper=off,h2=off,tower=off,tonic=off,reqwest=off,\
                 opentelemetry=off,opentelemetry_sdk=off,opentelemetry_otlp=off"
            ));
        }
        directive
    }

    #[cfg(any(test, feature = "test-support"))]
    #[derive(Debug)]
    pub struct TestExport {
        pub(crate) spans: opentelemetry_sdk::trace::InMemorySpanExporter,
        pub(crate) logs: opentelemetry_sdk::logs::InMemoryLogExporter,
        pub(crate) tracer_provider: SdkTracerProvider,
        pub(crate) logger_provider: SdkLoggerProvider,
    }

    #[cfg(test)]
    pub(crate) fn test_layers(debug: bool, run_id: &str) -> (TestExport, impl tracing::Subscriber) {
        test_layers_at(if debug { "debug" } else { "info" }, run_id)
    }

    #[cfg(test)]
    pub(crate) fn test_layers_at(
        test_level: &str,
        _run_id: &str,
    ) -> (TestExport, impl tracing::Subscriber) {
        use opentelemetry::trace::TracerProvider as _;

        let spans = opentelemetry_sdk::trace::InMemorySpanExporter::default();
        let logs = opentelemetry_sdk::logs::InMemoryLogExporter::default();
        let resource = build_resource_for(ServiceIdentity::HOST_ONE_SHOT);
        let tracer_provider = SdkTracerProvider::builder()
            .with_max_attributes_per_span(64)
            .with_max_attributes_per_event(32)
            .with_span_processor(GovernedSpanProcessor(
                opentelemetry_sdk::trace::SimpleSpanProcessor::new(spans.clone()),
            ))
            .with_resource(resource.clone())
            .build();
        let logger_provider = SdkLoggerProvider::builder()
            .with_log_processor(GovernedLogProcessor(
                opentelemetry_sdk::logs::SimpleLogProcessor::new(logs.clone()),
            ))
            .with_resource(resource)
            .build();
        let tracer = tracer_provider.tracer("jackin");
        let span_layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_location(false)
            .with_threads(false)
            .with_target(false)
            .with_tracked_inactivity(false)
            .with_error_records_to_exceptions(false)
            .with_error_events_to_status(false)
            .with_error_fields_to_exceptions(false);
        let log_layer = OpenTelemetryTracingBridge::new(&logger_provider);
        let span_directive = export_filter_directive(test_level);
        let log_directive = export_filter_directive(test_level);
        let subscriber = tracing_subscriber::registry()
            .with(
                span_layer
                    .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                        metadata.is_span()
                    }))
                    .with_filter(EnvFilter::new(span_directive)),
            )
            .with(
                log_layer
                    .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                        metadata.is_event()
                    }))
                    .with_filter(EnvFilter::new(log_directive)),
            );

        (
            TestExport {
                spans,
                logs,
                tracer_provider,
                logger_provider,
            },
            subscriber,
        )
    }

    /// Capsule-side in-memory bootstrap for layer conformance tests.
    #[cfg(any(test, feature = "test-support"))]
    pub fn test_capsule_layers(debug: bool) -> (TestExport, impl tracing::Subscriber) {
        use opentelemetry::trace::TracerProvider as _;

        let spans = opentelemetry_sdk::trace::InMemorySpanExporter::default();
        let logs = opentelemetry_sdk::logs::InMemoryLogExporter::default();
        let resource = build_resource_for(ServiceIdentity::CAPSULE);
        let tracer_provider = SdkTracerProvider::builder()
            .with_max_attributes_per_span(64)
            .with_max_attributes_per_event(32)
            .with_span_processor(GovernedSpanProcessor(
                opentelemetry_sdk::trace::SimpleSpanProcessor::new(spans.clone()),
            ))
            .with_resource(resource.clone())
            .build();
        let logger_provider = SdkLoggerProvider::builder()
            .with_log_processor(GovernedLogProcessor(
                opentelemetry_sdk::logs::SimpleLogProcessor::new(logs.clone()),
            ))
            .with_resource(resource)
            .build();
        let tracer = tracer_provider.tracer("jackin");
        let span_layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_location(false)
            .with_threads(false)
            .with_target(false)
            .with_tracked_inactivity(false)
            .with_error_records_to_exceptions(false)
            .with_error_events_to_status(false)
            .with_error_fields_to_exceptions(false);
        let log_layer = OpenTelemetryTracingBridge::new(&logger_provider);
        let test_level = if debug { "debug" } else { "info" };
        let span_directive = export_filter_directive(test_level);
        let log_directive = export_filter_directive(test_level);
        let subscriber = tracing_subscriber::registry()
            .with(
                span_layer
                    .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                        metadata.is_span()
                    }))
                    .with_filter(EnvFilter::new(span_directive)),
            )
            .with(
                log_layer
                    .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                        metadata.is_event()
                    }))
                    .with_filter(EnvFilter::new(log_directive)),
            );

        (
            TestExport {
                spans,
                logs,
                tracer_provider,
                logger_provider,
            },
            subscriber,
        )
    }

    /// Test-only entry for the governed session-start event.
    #[cfg(test)]
    pub(crate) fn emit_session_start_for_test(
        session_id: &str,
        _run_id: Option<&str>,
        _traceparent: Option<&str>,
    ) {
        let attrs = [jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::SESSION_ID,
            value: jackin_telemetry::Value::Str(session_id),
        }];
        let _event_result = jackin_telemetry::emit_event(
            &jackin_telemetry::event::SESSION_START,
            jackin_telemetry::FieldSet::new(&attrs, None),
        );
    }

    #[derive(Debug, Default)]
    struct ProcessSnapshot {
        cpu_utilization_bits: std::sync::atomic::AtomicU64,
        memory_bytes: std::sync::atomic::AtomicI64,
        valid: std::sync::atomic::AtomicBool,
    }

    fn start_process_sampler(pid: sysinfo::Pid, cpu_count: f64) -> std::sync::Arc<ProcessSnapshot> {
        use std::sync::atomic::Ordering;

        let snapshot = std::sync::Arc::new(ProcessSnapshot::default());
        let weak = std::sync::Arc::downgrade(&snapshot);
        drop(jackin_telemetry::spawn::thread_stream(
            "telemetry.process_sampler",
            move || {
                let mut system = sysinfo::System::new();
                while let Some(snapshot) = weak.upgrade() {
                    system.refresh_processes_specifics(
                        sysinfo::ProcessesToUpdate::Some(&[pid]),
                        true,
                        sysinfo::ProcessRefreshKind::nothing()
                            .with_cpu()
                            .with_memory(),
                    );
                    if let Some(process) = system.process(pid) {
                        let utilization = f64::from(process.cpu_usage()) / 100.0 / cpu_count;
                        snapshot
                            .cpu_utilization_bits
                            .store(utilization.to_bits(), Ordering::Relaxed);
                        snapshot.memory_bytes.store(
                            i64::try_from(process.memory()).unwrap_or(i64::MAX),
                            Ordering::Relaxed,
                        );
                        snapshot.valid.store(true, Ordering::Release);
                    }
                    drop(snapshot);
                    std::thread::park_timeout(std::time::Duration::from_millis(2_500));
                }
            },
        ));
        snapshot
    }

    /// Process and runtime metrics: CPU utilization and
    /// memory via `sysinfo`, plus the stable tokio runtime counters (workers,
    /// alive tasks, global queue depth) read from `app_handle` — jackin❯'s *app*
    /// runtime handle, captured by the caller before entering the dedicated
    /// telemetry runtime. Capturing it here would instead read the telemetry
    /// runtime; reading it from the collect thread (no ambient runtime) would
    /// yield `None`.
    fn init_metrics(
        resource: &Resource,
        endpoints: &OtlpEndpoints,
        app_handle: Option<tokio::runtime::Handle>,
    ) -> anyhow::Result<SdkMeterProvider> {
        use opentelemetry::metrics::MeterProvider as _;
        let runtime = otel_runtime()?;
        let runtime = runtime
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("telemetry runtime was not initialized"))?;
        let _runtime_guard = runtime.enter();
        let mut metric_builder = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_temporality(opentelemetry_sdk::metrics::Temporality::Cumulative)
            .with_endpoint(endpoints.metrics.clone())
            .with_timeout(endpoints.metrics_timeout)
            .with_compression(Compression::Gzip)
            .with_retry_policy(retry::policy());
        if let Some(tls) = exporter_tls(
            &endpoints.metrics_tls,
            "metrics",
            &endpoints.metrics,
            endpoints.metrics_timeout,
        )? {
            metric_builder = metric_builder.with_tls_config(tls);
        }
        let metric_exporter = metric_builder
            .build()
            .map_err(|_| anyhow::anyhow!("OTLP metric exporter init failed"))?;
        let reader = PeriodicReader::builder(CountingMetricExporter(metric_exporter), Tokio)
            .with_interval(std::time::Duration::from_secs(30))
            .with_timeout(EXPORT_ATTEMPT_TIMEOUT)
            .build();
        let governed_view = |instrument: &opentelemetry_sdk::metrics::Instrument| {
            let definition = jackin_telemetry::schema::metrics::definition(instrument.name())?;
            let mut stream = opentelemetry_sdk::metrics::Stream::builder()
                .with_cardinality_limit(jackin_telemetry::limits::MAX_CARDINALITY);
            if instrument.kind() == opentelemetry_sdk::metrics::InstrumentKind::Histogram {
                stream = stream.with_aggregation(
                    opentelemetry_sdk::metrics::Aggregation::ExplicitBucketHistogram {
                        boundaries: definition.boundaries.to_vec(),
                        record_min_max: false,
                    },
                );
            }
            stream.build().ok()
        };
        let provider = SdkMeterProvider::builder()
            .with_reader(reader)
            .with_view(governed_view)
            .with_resource(resource.clone())
            .build();
        let meter = provider.meter("jackin");
        if let Ok(pid) = sysinfo::get_current_pid() {
            use std::sync::atomic::Ordering;

            let cpu_count =
                std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get) as f64;
            let snapshot = start_process_sampler(pid, cpu_count);
            let cpu_snapshot = std::sync::Arc::clone(&snapshot);
            let _cpu_gauge = meter
                // semconv: process.cpu.utilization, unit "1", 0..1 fraction
                // of the CPUs available to the process.
                .f64_observable_gauge(
                    opentelemetry_semantic_conventions::metric::PROCESS_CPU_UTILIZATION,
                )
                .with_unit("1")
                .with_description("Fraction of total host CPU used by the jackin process")
                .with_callback(move |observer| {
                    if cpu_snapshot.valid.load(Ordering::Acquire) {
                        observer.observe(
                            f64::from_bits(
                                cpu_snapshot.cpu_utilization_bits.load(Ordering::Relaxed),
                            ),
                            &[],
                        );
                    }
                })
                .build();
            let _memory_counter = meter
                // semconv: process.memory.usage is an UpDownCounter (rises
                // and falls), not a gauge.
                .i64_observable_up_down_counter(
                    opentelemetry_semantic_conventions::metric::PROCESS_MEMORY_USAGE,
                )
                .with_unit("By")
                .with_description("Resident set size of the jackin process")
                .with_callback(move |observer| {
                    if snapshot.valid.load(Ordering::Acquire) {
                        observer.observe(snapshot.memory_bytes.load(Ordering::Relaxed), &[]);
                    }
                })
                .build();
        }

        if let Some(handle) = app_handle {
            let workers = handle.clone();
            let _worker_gauge = meter
                .u64_observable_gauge("tokio.runtime.workers")
                .with_description("Worker threads driving the tokio runtime")
                .with_callback(move |observer| {
                    observer.observe(workers.metrics().num_workers() as u64, &[]);
                })
                .build();
            let alive = handle.clone();
            let _alive_gauge = meter
                .u64_observable_gauge("tokio.runtime.alive_tasks")
                .with_description("Tasks currently alive in the tokio runtime")
                .with_callback(move |observer| {
                    observer.observe(alive.metrics().num_alive_tasks() as u64, &[]);
                })
                .build();
            let _queue_gauge = meter
                .u64_observable_gauge("tokio.runtime.global_queue.depth")
                .with_description("Tasks waiting in the tokio runtime's global queue")
                .with_callback(move |observer| {
                    observer.observe(handle.metrics().global_queue_depth() as u64, &[]);
                })
                .build();
        }

        Ok(provider)
    }

    pub(super) fn shutdown() {
        let _activation = ACTIVATION_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reap_flush_workers();
        let providers = PROVIDERS.lock().ok().and_then(|mut slot| slot.take());
        let generation = providers.as_ref().map(|providers| providers.generation);
        let runtime = OTEL_RUNTIME.lock().ok().and_then(|mut slot| slot.take());
        if providers.is_none() && runtime.is_none() {
            return;
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut succeeded = providers
            .as_ref()
            .is_none_or(|providers| providers.flush_and_shutdown(deadline));
        if let Some(runtime) = runtime {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                succeeded = false;
            }
            runtime.shutdown_timeout(remaining);
        }
        let timed_out = std::time::Instant::now() >= deadline;
        if let Some(generation) = generation {
            if timed_out {
                health::record_shutdown_timeout(generation);
            }
            health::record_shutdown(generation, succeeded && !timed_out);
        }
        reap_flush_workers();
    }

    fn retain_flush_worker(handle: std::thread::JoinHandle<()>) {
        PENDING_FLUSH_WORKERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(handle);
    }

    fn reap_flush_workers() {
        let mut pending = PENDING_FLUSH_WORKERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut unfinished = Vec::new();
        for handle in pending.drain(..) {
            if handle.is_finished() {
                drop(handle.join());
            } else {
                unfinished.push(handle);
            }
        }
        *pending = unfinished;
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn runtime_creation_count() -> u64 {
        OTEL_RUNTIME_CREATIONS.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(feature = "test-support")]
    pub(super) fn runtime_is_active() -> bool {
        OTEL_RUNTIME
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    #[cfg(test)]
    mod tests;
}

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

    let (def, outcome) = match kind {
        "stage_started" => (&event::LAUNCH_STAGE_STARTED, "success"),
        "stage_done" => (&event::LAUNCH_STAGE_DONE, "success"),
        "stage_failed" => (&event::LAUNCH_STAGE_FAILED, "failure"),
        "stage_skipped" => (&event::LAUNCH_STAGE_SKIPPED, "cancelled"),
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

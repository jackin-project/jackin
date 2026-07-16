// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Direct OTLP subscriber setup with no terminal or local-file sink.

use tracing_subscriber::prelude::*;

mod config;
mod health;
pub use health::{
    TelemetryHealth, TelemetrySignalHealth, record_telemetry_rejection, telemetry_health_snapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationReport {
    pub elapsed: std::time::Duration,
    pub health: TelemetryHealth,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationFailure {
    NoEndpoint,
    Disabled,
    Inactive,
    Export(&'static str),
    Rejected,
}

impl std::fmt::Display for ValidationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoEndpoint => formatter.write_str("no endpoint configured"),
            Self::Disabled => formatter.write_str("OpenTelemetry SDK is disabled"),
            Self::Inactive => formatter.write_str("telemetry providers are not active"),
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
    if !otlp_endpoint_configured() {
        return Err(ValidationFailure::NoEndpoint);
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
    Ok(ValidationReport {
        elapsed: started.elapsed(),
        health,
    })
}

/// Install the global `tracing` subscriber.
///
/// Default build: installs the JSONL diagnostics layer and no terminal sink.
/// With `--features otlp` and a standard OTLP endpoint configured
/// (`OTEL_EXPORTER_OTLP_ENDPOINT`, or the per-signal endpoint vars),
/// installs OTLP span, log, and metric export beside the JSONL layer, with the
/// diagnostics run id stamped on the OTLP resource so an external backend
/// (e.g. Parallax) can answer "show me run `<id>`".
///
/// Returns `Ok(true)` when OTLP export was installed (the backend is the active
/// sink), `Ok(false)` when only the JSONL diagnostics layer is installed (no
/// endpoint configured). Returns `Err` when a configured OTLP endpoint's
/// exporter fails to build or the subscriber is already set; on the exporter
/// failure path the JSONL-only layer is still installed as a fallback so the run
/// file (now the active sink) keeps capturing events.
// `allow`, not `expect`: the body is trivially const only in the default
// (no-otlp) build; the otlp build does non-const setup, so the lint fires in one
// cfg and not the other and a single non-const signature is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceIdentity {
    pub service_name: &'static str,
    pub app_mode: jackin_telemetry::schema::enums::AppMode,
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
}

pub fn init_tracing(debug: bool, run_id: &str) -> anyhow::Result<bool> {
    init_tracing_for(debug, run_id, ServiceIdentity::HOST_ONE_SHOT)
}

pub fn init_tracing_for(
    debug: bool,
    run_id: &str,
    identity: ServiceIdentity,
) -> anyhow::Result<bool> {
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
    let endpoints = otlp::OtlpEndpoints::new(endpoint, endpoint, Some(endpoint));
    otlp::init(false, "wire-conformance", identity, &endpoints)
}

/// Force all three wire-test providers to deliver their current batches.
#[cfg(feature = "test-support")]
pub fn flush_wire_test_export() -> Result<(), ValidationFailure> {
    otlp::validate_flush()
}

/// The first explicitly-requested OTLP protocol jackin cannot honor, when an
/// OTLP endpoint is configured (i.e. export is intended). `None` means the
/// configuration is exportable (grpc or unset) or no endpoint is set. Callers
/// use this to fail fast with a clear operator error before doing any work.
#[must_use]
pub fn unsupported_otlp_protocol() -> Option<String> {
    let env = |key: &str| std::env::var(key).ok();
    match config::resolve_otlp_config(&env) {
        Err(config::OtlpConfigError::UnsupportedProtocol { value, .. }) => Some(value),
        _ => None,
    }
}

/// Flush and shut down the OTLP exporters, if any are active.
///
/// Batch exporters hold the tail of a run in memory; a run that exits without
/// this call silently drops its last spans, log records, and metrics. Invoked
/// from `ActiveRunGuard::drop` so it runs on every exit path out of the run —
/// including `?` error early-returns — rather than only the success path.
/// No-op in default builds and when no endpoint was configured.
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
/// configured. Returns `None` when export is off (the JSONL path is enough).
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
    use opentelemetry::KeyValue;
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
    mod retry;

    /// The three SDK providers for one run, flushed together at shutdown.
    /// Named (not a positional tuple) so the flush sequence can't transpose
    /// tracer/logger/meter — all three expose identical `force_flush`/`shutdown`
    /// signatures, so a tuple destructure in the wrong order would compile
    /// silently. The meter is optional: metrics are best-effort.
    struct OtlpProviders {
        tracer: SdkTracerProvider,
        logger: SdkLoggerProvider,
        meter: Option<SdkMeterProvider>,
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
        fn flush_and_shutdown(&self) {
            health::record_export_attempt();
            #[cfg(test)]
            SHUTDOWN_ORDER
                .lock()
                .expect("shutdown order lock")
                .push("tracer");
            let trace_flush = self.tracer.force_flush();
            health::record_signal_export(health::Signal::Traces, trace_flush.is_ok());
            drop(self.tracer.shutdown());
            #[cfg(test)]
            SHUTDOWN_ORDER
                .lock()
                .expect("shutdown order lock")
                .push("logger");
            let log_flush = self.logger.force_flush();
            health::record_signal_export(health::Signal::Logs, log_flush.is_ok());
            drop(self.logger.shutdown());
            let metric_flush = self.meter.as_ref().map(|meter| {
                #[cfg(test)]
                SHUTDOWN_ORDER
                    .lock()
                    .expect("shutdown order lock")
                    .push("meter");
                let flushed = meter.force_flush();
                health::record_signal_export(health::Signal::Metrics, flushed.is_ok());
                drop(meter.shutdown());
                flushed
            });
            let failed = trace_flush
                .err()
                .or_else(|| log_flush.err())
                .or_else(|| metric_flush.and_then(Result::err));
            if let Some(error) = failed {
                health::record_export_failure();
                // Direct to stderr, not the deferred buffer: this fires at final
                // teardown where the run guard may outlive the terminal session,
                // so a buffered notice could never be drained. The TUI is already
                // gone by now, so stderr can't corrupt it.
                crate::logging::emit_teardown_notice(&format!(
                    "telemetry export failed to reach the backend (run telemetry may be incomplete): {error}"
                ));
            } else {
                health::record_export_success();
            }
            health::record_shutdown(true);
        }
    }

    pub(super) fn validate_flush() -> Result<(), super::ValidationFailure> {
        let providers = PROVIDERS.lock().expect("provider lock");
        let providers = providers
            .as_ref()
            .ok_or(super::ValidationFailure::Inactive)?;
        let trace = providers.tracer.force_flush();
        health::record_signal_export(health::Signal::Traces, trace.is_ok());
        let logs = providers.logger.force_flush();
        health::record_signal_export(health::Signal::Logs, logs.is_ok());
        let metrics = providers
            .meter
            .as_ref()
            .ok_or(super::ValidationFailure::Inactive)?
            .force_flush();
        health::record_signal_export(health::Signal::Metrics, metrics.is_ok());
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

    static PROVIDERS: std::sync::Mutex<Option<OtlpProviders>> = std::sync::Mutex::new(None);
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
    #[cfg(test)]
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
            #[cfg(test)]
            OTEL_RUNTIME_CREATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(runtime)
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(super) struct OtlpEndpoints {
        traces: String,
        logs: String,
        metrics: Option<String>,
        timeout: std::time::Duration,
    }

    impl OtlpEndpoints {
        pub(super) fn from_config(config: &super::config::OtlpConfig) -> Self {
            Self {
                traces: config.traces_endpoint.clone(),
                logs: config.logs_endpoint.clone(),
                metrics: Some(config.metrics_endpoint.clone()),
                timeout: config.timeout,
            }
        }

        /// The per-signal endpoints a single base produces. OTLP/gRPC sends every
        /// signal to the same endpoint verbatim and routes by gRPC service name,
        /// so — unlike OTLP/HTTP — no `/v1/<signal>` path is appended and all
        /// three share `base`.
        fn from_base(base: &str) -> Self {
            Self::new(base, base, Some(base))
        }

        /// The one construction choke point. Every field is run through
        /// [`grpc_endpoint`] here so the "normalized gRPC channel target"
        /// invariant has a single enforcement site rather than being re-asserted
        /// at each caller (where one could silently drift).
        pub(super) fn new(traces: &str, logs: &str, metrics: Option<&str>) -> Self {
            Self {
                traces: grpc_endpoint(traces),
                logs: grpc_endpoint(logs),
                metrics: metrics.map(grpc_endpoint),
                timeout: std::time::Duration::from_secs(5),
            }
        }
    }

    /// Host OTLP endpoints, when configured via the standard OTLP env vars.
    /// `OTEL_EXPORTER_OTLP_ENDPOINT` provides a base for every signal; the
    /// per-signal endpoint vars wrappers commonly inject override it per signal.
    pub(super) fn endpoints() -> Option<OtlpEndpoints> {
        if sdk_disabled() {
            return None;
        }
        resolve_endpoints(
            std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
            std::env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT").ok(),
            std::env::var("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT").ok(),
            std::env::var("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT").ok(),
        )
    }

    fn sdk_disabled() -> bool {
        std::env::var("OTEL_SDK_DISABLED")
            .is_ok_and(|value| value.trim().eq_ignore_ascii_case("true"))
    }

    fn validate_standard_env() -> anyhow::Result<()> {
        if let Ok(sampler) = std::env::var("OTEL_TRACES_SAMPLER")
            && !sampler.trim().is_empty()
            && sampler.trim() != "parentbased_always_on"
        {
            anyhow::bail!(
                "OTEL_TRACES_SAMPLER={} conflicts with required parentbased_always_on",
                sampler.trim()
            );
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
                anyhow::bail!("{var}={} is unsupported; expected gzip", value.trim());
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
        resolve_endpoint(std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
            .map(|endpoint| grpc_endpoint(&endpoint))
    }

    pub(super) fn endpoint_summary() -> Option<String> {
        let endpoints = endpoints()?;
        // A single configured base drives all three signal URLs, so collapse to
        // it; per-signal overrides break the match and are spelled out in full.
        if let Some(base) = base_endpoint()
            && endpoints == OtlpEndpoints::from_base(&base)
        {
            return Some(base);
        }
        Some(format!(
            "traces={}, logs={}, metrics={}",
            endpoints.traces,
            endpoints.logs,
            endpoints.metrics.as_deref().unwrap_or("disabled")
        ))
    }

    /// The configured base endpoint, if any. An exported-but-empty var must not
    /// produce a blank endpoint, so an empty value resolves to `None` and no
    /// OTLP layer is installed.
    fn resolve_endpoint(otel: Option<String>) -> Option<String> {
        otel.filter(|s| !s.is_empty())
    }

    fn resolve_endpoints(
        otel: Option<String>,
        traces: Option<String>,
        logs: Option<String>,
        metrics: Option<String>,
    ) -> Option<OtlpEndpoints> {
        let generic = resolve_endpoint(otel);
        // OTLP/gRPC: a per-signal endpoint var (if set) wins, else the generic
        // base. `OtlpEndpoints::new` applies `grpc_endpoint` normalization; this
        // closure only resolves which raw value to use. No `/v1/<signal>` path is
        // appended (an OTLP/HTTP convention; gRPC routes by service name).
        let signal = |specific: Option<String>| {
            specific
                .filter(|s| !s.is_empty())
                .or_else(|| generic.clone())
        };
        Some(OtlpEndpoints::new(
            &signal(traces)?,
            &signal(logs)?,
            signal(metrics).as_deref(),
        ))
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

    /// Stable source identity only; invocation and session identities remain
    /// signal attributes so repeated executions share Resource identity.
    fn resource(_run_id: &str, identity: ServiceIdentity) -> Resource {
        build_resource_for(identity)
    }

    fn build_resource_for(identity: ServiceIdentity) -> Resource {
        use jackin_telemetry::schema::attrs::{self, std_attrs};

        let executable_name = std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| identity.service_name.to_owned());
        let mut attributes = vec![
            KeyValue::new(std_attrs::SERVICE_NAMESPACE, "jackin"),
            KeyValue::new(std_attrs::SERVICE_NAME, identity.service_name),
            KeyValue::new(std_attrs::SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
            KeyValue::new(
                std_attrs::SERVICE_INSTANCE_ID,
                uuid::Uuid::new_v4().to_string(),
            ),
            KeyValue::new(std_attrs::PROCESS_PID, i64::from(std::process::id())),
            KeyValue::new(std_attrs::PROCESS_EXECUTABLE_NAME, executable_name),
            KeyValue::new(attrs::APP_MODE, identity.app_mode.as_str()),
            KeyValue::new(std_attrs::OS_TYPE, std::env::consts::OS),
            KeyValue::new(std_attrs::PROCESS_RUNTIME_NAME, "rust"),
        ];
        if let Some(version) = sysinfo::System::long_os_version() {
            attributes.push(KeyValue::new(std_attrs::OS_VERSION, version));
        }
        if let Some(version) = option_env!("RUSTC_VERSION") {
            attributes.push(KeyValue::new(std_attrs::PROCESS_RUNTIME_VERSION, version));
        }
        if identity == ServiceIdentity::CAPSULE
            && let Ok(container_id) = std::env::var("HOSTNAME")
            && !container_id.trim().is_empty()
        {
            attributes.push(KeyValue::new(std_attrs::CONTAINER_ID, container_id));
        }
        Resource::builder().with_attributes(attributes).build()
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
        traces_endpoint: &str,
        logs_endpoint: &str,
        timeout: std::time::Duration,
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
        let _runtime_guard = runtime.as_ref().expect("runtime initialized").enter();
        let span_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(traces_endpoint.to_owned())
            .with_timeout(timeout)
            .with_compression(Compression::Gzip)
            .with_retry_policy(retry::policy())
            .build()
            .map_err(|e| anyhow::anyhow!("OTLP span exporter init failed: {e}"))?;
        let log_exporter = opentelemetry_otlp::LogExporter::builder()
            .with_tonic()
            .with_endpoint(logs_endpoint.to_owned())
            .with_timeout(timeout)
            .with_compression(Compression::Gzip)
            .with_retry_policy(retry::policy())
            .build()
            .map_err(|e| anyhow::anyhow!("OTLP log exporter init failed: {e}"))?;

        // Attribute limits: generous but finite (observed max attrs + headroom).
        // Prevents unbounded dimension growth; DroppedAttributesCount must stay 0.
        let span_batch = SpanBatchConfigBuilder::default()
            .with_max_queue_size(2_048)
            .with_max_export_batch_size(512)
            .with_scheduled_delay(std::time::Duration::from_secs(1))
            .with_max_export_timeout(std::time::Duration::from_secs(5))
            .build();
        let log_batch = LogBatchConfigBuilder::default()
            .with_max_queue_size(4_096)
            .with_max_export_batch_size(512)
            .with_scheduled_delay(std::time::Duration::from_secs(1))
            .with_max_export_timeout(std::time::Duration::from_secs(5))
            .build();
        let tracer_provider = SdkTracerProvider::builder()
            .with_sampler(Sampler::ParentBased(Box::new(Sampler::AlwaysOn)))
            .with_max_attributes_per_span(64)
            .with_max_attributes_per_event(32)
            .with_span_processor(GovernedSpanProcessor(
                BatchSpanProcessor::builder(span_exporter, Tokio)
                    .with_batch_config(span_batch)
                    .build(),
            ))
            .with_resource(resource.clone())
            .build();
        let logger_provider = SdkLoggerProvider::builder()
            .with_log_processor(GovernedLogProcessor(
                BatchLogProcessor::builder(log_exporter, Tokio)
                    .with_batch_config(log_batch)
                    .build(),
            ))
            .with_resource(resource)
            .build();
        Ok((tracer_provider, logger_provider, app_handle))
    }

    #[derive(Debug)]
    struct GovernedLogProcessor<P>(P);

    impl<P: opentelemetry_sdk::logs::LogProcessor> opentelemetry_sdk::logs::LogProcessor
        for GovernedLogProcessor<P>
    {
        fn emit(
            &self,
            record: &mut opentelemetry_sdk::logs::SdkLogRecord,
            instrumentation: &opentelemetry::InstrumentationScope,
        ) {
            let governed = instrumentation.name() == jackin_telemetry::TELEMETRY_TARGET
                || record
                    .event_name()
                    .is_some_and(|name| jackin_telemetry::schema::events::ALL.contains(&name));
            if governed
                && record
                    .attributes_iter()
                    .any(|(key, _)| jackin_telemetry::privacy::validate_key(key.as_str()).is_err())
            {
                jackin_telemetry::record_export_rejection(
                    jackin_telemetry::Rejection::UnknownAttribute,
                );
                return;
            }
            self.0.emit(record, instrumentation);
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

        fn set_resource(&mut self, resource: &Resource) {
            self.0.set_resource(resource);
        }
    }

    #[derive(Debug)]
    struct GovernedSpanProcessor<P>(P);

    impl<P: opentelemetry_sdk::trace::SpanProcessor> opentelemetry_sdk::trace::SpanProcessor
        for GovernedSpanProcessor<P>
    {
        fn on_start(
            &self,
            span: &mut opentelemetry_sdk::trace::Span,
            context: &opentelemetry::Context,
        ) {
            self.0.on_start(span, context);
        }

        fn on_end(&self, span: opentelemetry_sdk::trace::SpanData) {
            if jackin_telemetry::schema::spans::ALL.contains(&span.name.as_ref())
                && span.attributes.iter().any(|attribute| {
                    jackin_telemetry::privacy::validate_key(attribute.key.as_str()).is_err()
                })
            {
                jackin_telemetry::record_export_rejection(
                    jackin_telemetry::Rejection::UnknownAttribute,
                );
                return;
            }
            self.0.on_end(span);
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

        fn set_resource(&mut self, resource: &Resource) {
            self.0.set_resource(resource);
        }
    }

    pub(super) fn init(
        debug: bool,
        run_id: &str,
        identity: ServiceIdentity,
        endpoints: &OtlpEndpoints,
    ) -> anyhow::Result<()> {
        let resource = resource(run_id, identity);
        let (tracer_provider, logger_provider, app_handle) = build_otlp_providers(
            resource.clone(),
            &endpoints.traces,
            &endpoints.logs,
            endpoints.timeout,
        )?;
        // Metrics are best-effort: a failed exporter build must never block
        // span/log telemetry or the run itself. Defer reporting the failure —
        // emitting here would predate `try_init()` and the message would hit no
        // subscriber, so the one diagnostic this branch exists to surface would
        // be dropped on the floor.
        let (meter_provider, metric_error) =
            if let Some(metrics_endpoint) = endpoints.metrics.as_deref() {
                match init_metrics(&resource, metrics_endpoint, app_handle) {
                    Ok(provider) => (Some(provider), None),
                    Err(error) => (None, Some(error)),
                }
            } else {
                (None, None)
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
            .with(log_layer.with_filter(EnvFilter::new(log_directive)))
            .try_init()
            .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"));
        if installed.is_ok() {
            let metrics_active = meter_provider.is_some();
            *PROVIDERS.lock().expect("provider lock") = Some(OtlpProviders {
                tracer: tracer_provider,
                logger: logger_provider,
                meter: meter_provider,
            });
            health::set_active_signals(metrics_active);
            if let Some(error) = metric_error {
                // Subscriber is live now, so a `--debug` run captures this.
                tracing::debug!("OTLP metric exporter unavailable: {error}");
            }
        }
        installed
    }

    /// Capsule Resource is stable source identity only (same as host).
    fn capsule_resource() -> Resource {
        build_resource_for(ServiceIdentity::CAPSULE)
    }

    /// Install OTLP export for the capsule. Mirrors `init` but composes no
    /// direct OTLP layers and stamps the
    /// capsule resource; providers come from [`build_otlp_providers`].
    pub(super) fn init_capsule(
        _traceparent: Option<&str>,
        config: &super::config::OtlpConfig,
    ) -> anyhow::Result<()> {
        let resource = capsule_resource();
        let (tracer_provider, logger_provider, app_handle) = build_otlp_providers(
            resource.clone(),
            &config.traces_endpoint,
            &config.logs_endpoint,
            config.timeout,
        )?;
        let meter_provider = init_metrics(&resource, &config.metrics_endpoint, app_handle).ok();

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
        // Surface OTLP exporter/SDK diagnostics (export failures, refused
        // endpoint, gRPC errors) to the capsule's stderr — captured by
        // `docker logs`. The OTLP span/log
        // layers above keep `opentelemetry*=off`, so these diagnostics never
        // feed back through the exporter: no export-error → log → export loop.
        // Without this sink, a failing in-container export is silently dropped
        // (a silent failure), making "no capsule telemetry in the backend"
        // impossible to diagnose.
        let otlp_diag_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(std::io::stderr)
            .with_filter(EnvFilter::new(
                "off,opentelemetry=warn,opentelemetry_sdk=warn,opentelemetry_otlp=warn",
            ));
        let installed = tracing_subscriber::registry()
            .with(
                span_layer
                    .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                        metadata.is_span()
                    }))
                    .with_filter(EnvFilter::new(span_directive)),
            )
            .with(log_layer.with_filter(EnvFilter::new(log_directive)))
            .with(otlp_diag_layer)
            .try_init()
            .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"));
        if installed.is_ok() {
            *PROVIDERS.lock().expect("provider lock") = Some(OtlpProviders {
                tracer: tracer_provider,
                logger: logger_provider,
                meter: meter_provider,
            });
        }
        installed
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
        "jackin_tui",
        "jackin_tui_lookbook",
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
        use opentelemetry::trace::TracerProvider as _;

        let spans = opentelemetry_sdk::trace::InMemorySpanExporter::default();
        let logs = opentelemetry_sdk::logs::InMemoryLogExporter::default();
        let resource = resource(run_id, ServiceIdentity::HOST_ONE_SHOT);
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
            .with(log_layer.with_filter(EnvFilter::new(log_directive)));

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

    /// Capsule-side in-memory bootstrap without the host JSONL layer.
    #[cfg(any(test, feature = "test-support"))]
    pub fn test_capsule_layers(debug: bool) -> (TestExport, impl tracing::Subscriber) {
        use opentelemetry::trace::TracerProvider as _;

        let spans = opentelemetry_sdk::trace::InMemorySpanExporter::default();
        let logs = opentelemetry_sdk::logs::InMemoryLogExporter::default();
        let resource = capsule_resource();
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
            .with(log_layer.with_filter(EnvFilter::new(log_directive)));

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
        let _ = jackin_telemetry::emit_event(
            &jackin_telemetry::event::SESSION_START,
            jackin_telemetry::FieldSet::new(&attrs, None),
        );
    }

    /// Shared process sampler. Both the CPU and memory instruments read from
    /// one instance, and `sample()` refreshes `sysinfo` at most once per
    /// collect cycle. This is load-bearing for CPU correctness: `cpu_usage()`
    /// is the delta since the previous refresh, so refreshing twice per cycle
    /// (once per instrument) would measure CPU over the microseconds between
    /// the two reads — a near-zero, callback-order-dependent value. The gate
    /// (half the 5 s export interval) guarantees one refresh per cycle and a
    /// stable ~5 s delta window regardless of instrument observation order.
    struct ProcSampler {
        system: sysinfo::System,
        pid: sysinfo::Pid,
        last_refresh: Option<std::time::Instant>,
        cached: Option<(f32, u64)>,
    }

    impl ProcSampler {
        fn sample(&mut self) -> Option<(f32, u64)> {
            let stale = self
                .last_refresh
                .is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(2_500));
            if stale {
                self.system.refresh_processes_specifics(
                    sysinfo::ProcessesToUpdate::Some(&[self.pid]),
                    true,
                    sysinfo::ProcessRefreshKind::nothing()
                        .with_cpu()
                        .with_memory(),
                );
                self.cached = self
                    .system
                    .process(self.pid)
                    .map(|process| (process.cpu_usage(), process.memory()));
                self.last_refresh = Some(std::time::Instant::now());
            }
            self.cached
        }
    }

    /// Process and runtime metrics, exported every 5 s: CPU utilization and
    /// memory via `sysinfo`, plus the stable tokio runtime counters (workers,
    /// alive tasks, global queue depth) read from `app_handle` — jackin❯'s *app*
    /// runtime handle, captured by the caller before entering the dedicated
    /// telemetry runtime. Capturing it here would instead read the telemetry
    /// runtime; reading it from the collect thread (no ambient runtime) would
    /// yield `None`.
    fn init_metrics(
        resource: &Resource,
        metrics_endpoint: &str,
        app_handle: Option<tokio::runtime::Handle>,
    ) -> anyhow::Result<SdkMeterProvider> {
        use opentelemetry::metrics::MeterProvider as _;
        use std::sync::Mutex;

        let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_temporality(opentelemetry_sdk::metrics::Temporality::Cumulative)
            .with_endpoint(metrics_endpoint.to_owned())
            .with_compression(Compression::Gzip)
            .with_retry_policy(retry::policy())
            .build()
            .map_err(|e| anyhow::anyhow!("OTLP metric exporter init failed: {e}"))?;
        let reader = PeriodicReader::builder(metric_exporter, Tokio)
            .with_interval(std::time::Duration::from_secs(30))
            .build();
        let governed_view = |instrument: &opentelemetry_sdk::metrics::Instrument| {
            if !jackin_telemetry::schema::metrics::ALL.contains(&instrument.name()) {
                return None;
            }
            let mut stream = opentelemetry_sdk::metrics::Stream::builder()
                .with_cardinality_limit(jackin_telemetry::limits::MAX_CARDINALITY);
            if instrument.kind() == opentelemetry_sdk::metrics::InstrumentKind::Histogram {
                stream = stream.with_aggregation(
                    opentelemetry_sdk::metrics::Aggregation::ExplicitBucketHistogram {
                        boundaries: vec![
                            0.001, 0.005, 0.010, 0.025, 0.050, 0.100, 0.250, 0.500, 1.0, 2.5, 5.0,
                            10.0, 30.0, 60.0,
                        ],
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
            let cpu_count =
                std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get) as f64;
            let sampler = std::sync::Arc::new(Mutex::new(ProcSampler {
                system: sysinfo::System::new(),
                pid,
                last_refresh: None,
                cached: None,
            }));
            let cpu_sampler = std::sync::Arc::clone(&sampler);
            let _ = meter
                // semconv: process.cpu.utilization, unit "1", 0..1 fraction
                // of the CPUs available to the process.
                .f64_observable_gauge(
                    opentelemetry_semantic_conventions::metric::PROCESS_CPU_UTILIZATION,
                )
                .with_unit("1")
                .with_description("Fraction of total host CPU used by the jackin process")
                .with_callback(move |observer| {
                    if let Some((cpu_percent, _)) =
                        cpu_sampler.lock().ok().and_then(|mut s| s.sample())
                    {
                        // `sysinfo` reports percent of one core; semconv
                        // utilization is a 0..1 fraction of all cores.
                        observer.observe(f64::from(cpu_percent) / 100.0 / cpu_count, &[]);
                    }
                })
                .build();
            let _ = meter
                // semconv: process.memory.usage is an UpDownCounter (rises
                // and falls), not a gauge.
                .i64_observable_up_down_counter(
                    opentelemetry_semantic_conventions::metric::PROCESS_MEMORY_USAGE,
                )
                .with_unit("By")
                .with_description("Resident set size of the jackin process")
                .with_callback(move |observer| {
                    if let Some((_, memory_bytes)) =
                        sampler.lock().ok().and_then(|mut s| s.sample())
                    {
                        observer.observe(i64::try_from(memory_bytes).unwrap_or(i64::MAX), &[]);
                    }
                })
                .build();
        }

        if let Some(handle) = app_handle {
            let workers = handle.clone();
            let _ = meter
                .u64_observable_gauge("tokio.runtime.workers")
                .with_description("Worker threads driving the tokio runtime")
                .with_callback(move |observer| {
                    observer.observe(workers.metrics().num_workers() as u64, &[]);
                })
                .build();
            let alive = handle.clone();
            let _ = meter
                .u64_observable_gauge("tokio.runtime.alive_tasks")
                .with_description("Tasks currently alive in the tokio runtime")
                .with_callback(move |observer| {
                    observer.observe(alive.metrics().num_alive_tasks() as u64, &[]);
                })
                .build();
            let _ = meter
                .u64_observable_gauge("tokio.runtime.global_queue.depth")
                .with_description("Tasks waiting in the tokio runtime's global queue")
                .with_callback(move |observer| {
                    observer.observe(handle.metrics().global_queue_depth() as u64, &[]);
                })
                .build();
        }

        jackin_telemetry::install(&meter);

        Ok(provider)
    }

    pub(super) fn shutdown() {
        let providers = PROVIDERS.lock().ok().and_then(|mut slot| slot.take());
        if let Some(providers) = providers {
            providers.flush_and_shutdown();
        }
        if let Ok(mut runtime) = OTEL_RUNTIME.lock() {
            drop(runtime.take());
        }
    }

    #[cfg(test)]
    fn runtime_creation_count() -> u64 {
        OTEL_RUNTIME_CREATIONS.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Thin counter recorder for the operation facade. Plan 042 replaces this.
    pub(super) fn record_operation_metric(
        name: &'static str,
        value: u64,
        attrs: &[(&'static str, String)],
    ) {
        use opentelemetry::KeyValue;
        use opentelemetry::metrics::MeterProvider as _;

        let Ok(providers) = PROVIDERS.lock() else {
            return;
        };
        let Some(meter_provider) = providers
            .as_ref()
            .and_then(|providers| providers.meter.as_ref())
        else {
            return;
        };
        let meter = meter_provider.meter("jackin");
        let counter = meter.u64_counter(name).build();
        let kvs: Vec<KeyValue> = attrs
            .iter()
            .map(|(k, v)| KeyValue::new(*k, v.clone()))
            .collect();
        counter.add(value, &kvs);
    }

    #[cfg(test)]
    mod tests;
}

/// Crate-visible wrapper for [`crate::operation_metric`].
pub(crate) fn record_operation_metric(
    name: &'static str,
    value: u64,
    attrs: &[(&'static str, String)],
) {
    otlp::record_operation_metric(name, value, attrs);
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
        "session_detach" => (&event::CAPSULE_SESSION_DETACH, "expected_close"),
        "clean_shutdown" => (&event::CAPSULE_SESSION_CLEAN_SHUTDOWN, "expected_close"),
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
    let _ = jackin_telemetry::emit_event(def, FieldSet::new(&attrs, Some(message.as_ref())));
}

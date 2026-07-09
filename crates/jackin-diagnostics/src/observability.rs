//! `tracing` subscriber setup for JSONL diagnostics plus optional OTLP export.
//!
//! The default subscriber installs only [`JackinDiagnosticsLayer`]. It has no
//! stdout/stderr sink: diagnostic output must never stream over the operator's
//! full-screen TUI or plain CLI surface. With `--features otlp` and a standard
//! OTLP endpoint configured (`OTEL_EXPORTER_OTLP_ENDPOINT`), an OTLP export
//! layer is added beside the JSONL layer.

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;

const JSONL_TARGET: &str = "jackin_diagnostics::jsonl";

/// OTLP/tracing attribute keys — the single source of truth for jackin❯'s
/// telemetry tag taxonomy. Every key is dotted, never underscored: jackin❯'s own
/// keys use the `jackin.*` namespace, the run id uses `parallax.*` (the
/// reference backend), and `service.*`/`session.*` reuse the OpenTelemetry
/// standard namespaces. Instrumentation sites across the host TUI, launch flow,
/// and capsule reference these constants so a key is spelled exactly once.
pub mod otel_keys {
    // OTel standard namespaces (do not invent jackin equivalents).
    pub const SERVICE_NAME: &str = "service.name";
    pub const SERVICE_VERSION: &str = "service.version";
    /// Standard OpenTelemetry session id — used to group all telemetry from one capsule
    /// session into a single timeline (see the `session` semconv).
    pub const SESSION_ID: &str = "session.id";

    // jackin custom namespace (no OTel standard equivalent exists).
    /// CLI-invocation id; correlates every trace/log/metric of one `jackin` run.
    /// Uses the `parallax.*` namespace (Parallax is the reference backend) so a
    /// single dotted key groups the run — there is no separate `jackin.run.id`.
    pub const RUN_ID: &str = "parallax.run.id";
    /// Process role within a run: `host` or `capsule`.
    pub const COMPONENT: &str = "jackin.component";
    /// TUI screen a span belongs to (`list`, `settings`, `editor`, `create`,
    /// `launch`, `capsule`).
    pub const SCREEN_NAME: &str = "jackin.screen.name";
    /// Screen the operator navigated from (the linked predecessor).
    pub const SCREEN_FROM: &str = "jackin.screen.from";
    pub const WORKSPACE: &str = "jackin.workspace";
    pub const WORKSPACE_KIND: &str = "jackin.workspace.kind";
    pub const AGENT_SELECTED: &str = "jackin.agent.selected";
    pub const AGENTS_ACTIVE: &str = "jackin.agents.active";
    pub const ROLE: &str = "jackin.role";
    pub const PROVIDER: &str = "jackin.provider";
    pub const CONTAINER_ID: &str = "jackin.container.id";
    pub const CONTAINER_NAME: &str = "jackin.container.name";
    pub const LAUNCH_STAGE: &str = "jackin.launch.stage";
    pub const ACTION: &str = "jackin.action";
    /// Capsule tab/pane label.
    pub const TAB_LABEL: &str = "jackin.tab.label";
}

/// Tracing layer that turns marked diagnostics events into run JSONL records.
///
/// `RunDiagnostics` methods emit events with `target = JSONL_TARGET`; this
/// layer is the single JSONL sink. Other tracing events are left for optional
/// exporters such as OTLP.
#[derive(Debug, Default)]
pub struct JackinDiagnosticsLayer;

impl<S> Layer<S> for JackinDiagnosticsLayer
where
    S: Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        if metadata.target() != JSONL_TARGET {
            // The OpenTelemetry SDK reports its own failures (export errors,
            // dropped batches, partial-success) as `tracing` events on
            // `opentelemetry*` targets. They are filtered OUT of the OTLP log
            // bridge (else an export error would itself try to export — a
            // feedback loop), so this layer is the only place they can be made
            // durable: capture WARN+ into the active run as `otlp_internal`.
            // Without this, "telemetry isn't reaching the backend" is invisible.
            if metadata.target().starts_with("opentelemetry")
                && matches!(
                    *metadata.level(),
                    tracing::Level::WARN | tracing::Level::ERROR
                )
            {
                let mut visitor = OtelInternalVisitor::default();
                event.record(&mut visitor);
                if let Some(run) = crate::active_run() {
                    run.record_otlp_internal(metadata.level().as_str(), &visitor.into_message());
                }
            }
        }
    }
}

/// Flattens an OpenTelemetry-internal event's fields into one line. These
/// events carry a `name` (the exporter event tag, e.g. `ExportFailed`) plus
/// ad-hoc fields (`error`, `reason`, …); concatenate them so the run record
/// shows the exporter's own words verbatim rather than just a level.
#[derive(Default)]
struct OtelInternalVisitor {
    name: Option<String>,
    fields: Vec<String>,
}

impl OtelInternalVisitor {
    fn into_message(self) -> String {
        let mut parts = Vec::new();
        if let Some(name) = self.name {
            parts.push(name);
        }
        parts.extend(self.fields);
        if parts.is_empty() {
            "opentelemetry internal event".to_owned()
        } else {
            parts.join(" ")
        }
    }

    fn record_field(&mut self, name: &str, value: String) {
        match name {
            "name" => self.name = Some(value),
            "message" => self.fields.insert(0, value),
            _ => self.fields.push(format!("{name}={value}")),
        }
    }
}

impl Visit for OtelInternalVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_field(field.name(), value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_field(field.name(), format!("{value:?}"));
    }
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
#[allow(clippy::missing_const_for_fn)]
pub fn init_tracing(debug: bool, run_id: &str) -> anyhow::Result<bool> {
    #[cfg(feature = "otlp")]
    {
        if let Some(endpoints) = otlp::endpoints() {
            return match otlp::init(debug, run_id, &endpoints) {
                Ok(()) => Ok(true),
                Err(error) => {
                    // OTLP requested but unavailable: install the JSONL-only
                    // layer so the file fallback still captures events, then
                    // report the failure to the caller (which surfaces it).
                    install_jsonl_only();
                    Err(error)
                }
            };
        }
    }

    // No fmt layer: the operator's terminal must never receive the firehose.
    let _ = (debug, run_id);
    tracing_subscriber::registry()
        .with(JackinDiagnosticsLayer)
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"))?;
    Ok(false)
}

/// Install the JSONL-only subscriber as a fallback. Ignores an
/// already-installed error: that means a subscriber exists, which is all the
/// fallback needs. Only the otlp build reaches this (the failed-exporter path).
#[cfg(feature = "otlp")]
fn install_jsonl_only() {
    drop(
        tracing_subscriber::registry()
            .with(JackinDiagnosticsLayer)
            .try_init(),
    );
}

/// The first explicitly-requested OTLP protocol jackin cannot honor, when an
/// OTLP endpoint is configured (i.e. export is intended). `None` means the
/// configuration is exportable (grpc or unset) or no endpoint is set. Callers
/// use this to fail fast with a clear operator error before doing any work.
#[must_use]
pub fn unsupported_otlp_protocol() -> Option<String> {
    #[cfg(feature = "otlp")]
    {
        otlp::first_unsupported_protocol()
    }
    #[cfg(not(feature = "otlp"))]
    {
        None
    }
}

/// Flush and shut down the OTLP exporters, if any are active.
///
/// Batch exporters hold the tail of a run in memory; a run that exits without
/// this call silently drops its last spans, log records, and metrics. Invoked
/// from `ActiveRunGuard::drop` so it runs on every exit path out of the run —
/// including `?` error early-returns — rather than only the success path.
/// No-op in default builds and when no endpoint was configured.
#[allow(clippy::missing_const_for_fn)]
pub(crate) fn shutdown_otlp() {
    #[cfg(feature = "otlp")]
    otlp::shutdown();
}

/// Flush and shut down the capsule's OTLP exporters at process exit. The public
/// counterpart to the host's guard-driven [`shutdown_otlp`]; the capsule has no
/// `ActiveRunGuard`, so it calls this explicitly before the daemon exits.
#[allow(clippy::missing_const_for_fn)]
pub fn shutdown_capsule_tracing() {
    #[cfg(feature = "otlp")]
    otlp::shutdown();
}

/// Install OTLP export for the in-container capsule process.
///
/// `session_id` groups all of this session's telemetry (standard `session.id`);
/// `run_id` (the host's `parallax.run.id`, propagated via env) joins the session
/// to the host run; `traceparent` (propagated W3C header) links the session
/// back to the launch trace. Returns `Ok(true)` when export was activated,
/// `Ok(false)` when no endpoint is configured (the common, no-op case).
#[allow(clippy::missing_const_for_fn)]
pub fn init_capsule_tracing(
    session_id: &str,
    run_id: Option<&str>,
    traceparent: Option<&str>,
) -> anyhow::Result<bool> {
    #[cfg(feature = "otlp")]
    let activated = match otlp::base_endpoint() {
        Some(endpoint) => {
            otlp::init_capsule(session_id, run_id, traceparent, &endpoint)?;
            true
        }
        None => false,
    };
    #[cfg(not(feature = "otlp"))]
    let activated = {
        let _ = (session_id, run_id, traceparent);
        false
    };
    Ok(activated)
}

/// The configured host OTLP endpoint (`OTEL_EXPORTER_OTLP_ENDPOINT`), or `None`
/// when export is off / not compiled.
#[must_use]
pub fn configured_endpoint() -> Option<String> {
    #[cfg(feature = "otlp")]
    {
        otlp::base_endpoint()
    }
    #[cfg(not(feature = "otlp"))]
    {
        None
    }
}

/// Human-readable host OTLP endpoint configuration for debug banners.
#[must_use]
pub fn configured_endpoint_summary() -> Option<String> {
    #[cfg(feature = "otlp")]
    {
        otlp::endpoint_summary()
    }
    #[cfg(not(feature = "otlp"))]
    {
        None
    }
}

/// Whether the operator set any OTLP endpoint env var (export intended), even if
/// the resulting config is incomplete and so installs no exporter. Lets the
/// caller surface "export configured but disabled" instead of silently treating
/// it as never requested. Always `false` without the `otlp` feature.
#[must_use]
pub fn otlp_endpoint_configured() -> bool {
    #[cfg(feature = "otlp")]
    {
        otlp::any_endpoint_configured()
    }
    #[cfg(not(feature = "otlp"))]
    {
        false
    }
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
    #[cfg(feature = "otlp")]
    {
        otlp::container_endpoint()
    }
    #[cfg(not(feature = "otlp"))]
    {
        None
    }
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
#[cfg(feature = "otlp")]
mod otlp {
    use std::sync::OnceLock;

    use opentelemetry::KeyValue;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::Resource;
    use opentelemetry_sdk::logs::SdkLoggerProvider;
    use opentelemetry_sdk::logs::log_processor_with_async_runtime::BatchLogProcessor;
    use opentelemetry_sdk::metrics::SdkMeterProvider;
    use opentelemetry_sdk::metrics::periodic_reader_with_async_runtime::PeriodicReader;
    use opentelemetry_sdk::runtime::Tokio;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use opentelemetry_sdk::trace::span_processor_with_async_runtime::BatchSpanProcessor;
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::prelude::*;

    use super::JackinDiagnosticsLayer;
    use super::otel_keys as keys;

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
            let trace_flush = self.tracer.force_flush();
            drop(self.tracer.shutdown());
            let log_flush = self.logger.force_flush();
            drop(self.logger.shutdown());
            let metric_flush = self.meter.as_ref().map(|meter| {
                let flushed = meter.force_flush();
                drop(meter.shutdown());
                flushed
            });
            let failed = trace_flush
                .err()
                .or_else(|| log_flush.err())
                .or_else(|| metric_flush.and_then(Result::err));
            if let Some(error) = failed {
                // Direct to stderr, not the deferred buffer: this fires at final
                // teardown where the run guard may outlive the terminal session,
                // so a buffered notice could never be drained. The TUI is already
                // gone by now, so stderr can't corrupt it.
                crate::logging::emit_teardown_notice(&format!(
                    "telemetry export failed to reach the backend (run telemetry may be incomplete): {error}"
                ));
            }
        }
    }

    static PROVIDERS: OnceLock<OtlpProviders> = OnceLock::new();

    /// Dedicated multi-thread tokio runtime that drives OTLP export. Held for the
    /// process lifetime so the async-runtime batch processors (and tonic's h2
    /// connection driver) have a reactor decoupled from jackin❯'s current-thread
    /// main: the `futures_executor::block_on` flush parks the main thread, and
    /// these worker threads keep exporting regardless. One worker is plenty for
    /// a single run's telemetry volume.
    static OTEL_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

    /// Build-or-get the dedicated telemetry runtime. Providers must be built
    /// inside its [`tokio::runtime::Runtime::enter`] guard so their workers spawn
    /// onto it rather than the ambient (current-thread) app runtime.
    fn otel_runtime() -> anyhow::Result<&'static tokio::runtime::Runtime> {
        if let Some(runtime) = OTEL_RUNTIME.get() {
            return Ok(runtime);
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .thread_name("jackin-otel")
            .build()
            .map_err(|e| anyhow::anyhow!("OTLP telemetry runtime init failed: {e}"))?;
        Ok(OTEL_RUNTIME.get_or_init(|| runtime))
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(super) struct OtlpEndpoints {
        traces: String,
        logs: String,
        metrics: Option<String>,
    }

    impl OtlpEndpoints {
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
        fn new(traces: &str, logs: &str, metrics: Option<&str>) -> Self {
            Self {
                traces: grpc_endpoint(traces),
                logs: grpc_endpoint(logs),
                metrics: metrics.map(grpc_endpoint),
            }
        }
    }

    /// Host OTLP endpoints, when configured via the standard OTLP env vars.
    /// `OTEL_EXPORTER_OTLP_ENDPOINT` provides a base for every signal; the
    /// per-signal endpoint vars wrappers commonly inject override it per signal.
    pub(super) fn endpoints() -> Option<OtlpEndpoints> {
        resolve_endpoints(
            std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
            std::env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT").ok(),
            std::env::var("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT").ok(),
            std::env::var("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT").ok(),
        )
    }

    /// The standard OTLP endpoint env vars (generic base + per-signal). Used to
    /// tell "operator configured export" apart from "export not requested" even
    /// when the config is incomplete (e.g. only a metrics endpoint, which can't
    /// build the mandatory traces+logs and so yields no `OtlpEndpoints`).
    const ENDPOINT_VARS: [&str; 4] = [
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
        "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
    ];

    /// Whether any OTLP endpoint var is set to a non-empty value — i.e. the
    /// operator intends export. True even when [`endpoints`] returns `None`
    /// because the config is incomplete; the caller uses the gap to surface a
    /// notice rather than silently disabling export.
    pub(super) fn any_endpoint_configured() -> bool {
        ENDPOINT_VARS
            .iter()
            .any(|var| std::env::var(var).is_ok_and(|value| !value.trim().is_empty()))
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

    /// The first explicitly-requested non-grpc protocol value, but only when an
    /// OTLP endpoint is configured (no endpoint → no export intended → the
    /// protocol vars are moot). Drives the fatal startup check.
    pub(super) fn first_unsupported_protocol() -> Option<String> {
        endpoints()?;
        PROTOCOL_VARS.into_iter().find_map(|var| {
            std::env::var(var)
                .ok()
                .filter(|value| unsupported_protocol(value))
                .map(|value| value.trim().to_owned())
        })
    }

    /// The OTLP resource. `service.name` is always `jackin`; the diagnostics
    /// run id rides as `parallax.run.id` (dotted) so backends can correlate
    /// telemetry with the run JSONL the operator shares. `jackin.component`
    /// marks this process as the host (the in-container capsule stamps
    /// `capsule`).
    fn resource(run_id: &str) -> Resource {
        build_resource(run_id)
    }

    fn build_resource(run_id: &str) -> Resource {
        let attributes = vec![
            KeyValue::new(keys::SERVICE_NAME, "jackin"),
            KeyValue::new(keys::SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
            KeyValue::new(keys::COMPONENT, "host"),
            KeyValue::new(keys::RUN_ID, run_id.to_owned()),
        ];
        Resource::builder().with_attributes(attributes).build()
    }

    pub(super) fn init(debug: bool, run_id: &str, endpoints: &OtlpEndpoints) -> anyhow::Result<()> {
        ensure_grpc_protocol().map_err(|e| anyhow::anyhow!(e))?;
        let runtime = otel_runtime()?;
        // The tokio runtime gauges must report jackin❯'s app runtime, not the
        // dedicated telemetry runtime — capture its handle before entering ours.
        let app_handle = tokio::runtime::Handle::try_current().ok();
        // Build every exporter, processor, and reader inside the dedicated
        // runtime: the async-runtime processors spawn their worker tasks (and
        // tonic spawns its h2 connection driver) onto whichever runtime is
        // entered here, and they must land on the multi-thread telemetry runtime
        // — not jackin❯'s current-thread main, where flush would deadlock.
        let _runtime_guard = runtime.enter();
        let span_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoints.traces.clone())
            .build()
            .map_err(|e| anyhow::anyhow!("OTLP span exporter init failed: {e}"))?;
        let log_exporter = opentelemetry_otlp::LogExporter::builder()
            .with_tonic()
            .with_endpoint(endpoints.logs.clone())
            .build()
            .map_err(|e| anyhow::anyhow!("OTLP log exporter init failed: {e}"))?;

        let resource = resource(run_id);
        let tracer_provider = SdkTracerProvider::builder()
            .with_span_processor(BatchSpanProcessor::builder(span_exporter, Tokio).build())
            .with_resource(resource.clone())
            .build();
        let logger_provider = SdkLoggerProvider::builder()
            .with_log_processor(BatchLogProcessor::builder(log_exporter, Tokio).build())
            .with_resource(resource.clone())
            .build();
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
        let span_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        let log_layer = OpenTelemetryTracingBridge::new(&logger_provider);

        // Scope the export to jackin❯'s own telemetry. Dependency-internal
        // spans/logs stay out of OTLP unless the operator asks for them with
        // `JACKIN_OTEL_INTERNAL=1`.
        let directive = export_filter_directive(export_level(debug));
        let installed = tracing_subscriber::registry()
            .with(JackinDiagnosticsLayer)
            .with(span_layer.with_filter(EnvFilter::new(directive.clone())))
            .with(log_layer.with_filter(EnvFilter::new(directive)))
            .try_init()
            .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"));
        if installed.is_ok() {
            drop(PROVIDERS.set(OtlpProviders {
                tracer: tracer_provider,
                logger: logger_provider,
                meter: meter_provider,
            }));
            if let Some(error) = metric_error {
                // Subscriber is live now, so a `--debug` run captures this.
                tracing::debug!("OTLP metric exporter unavailable: {error}");
            }
        }
        installed
    }

    /// The OTLP resource for the in-container capsule process: marks the
    /// component as `capsule`, carries the standard `session.id` that groups
    /// all of one session's telemetry, and the host `parallax.run.id` so the
    /// session telemetry joins the host run.
    fn capsule_resource(session_id: &str, run_id: Option<&str>) -> Resource {
        let mut attributes = vec![
            KeyValue::new(keys::SERVICE_NAME, "jackin"),
            KeyValue::new(keys::SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
            KeyValue::new(keys::COMPONENT, "capsule"),
            KeyValue::new(keys::SESSION_ID, session_id.to_owned()),
        ];
        if let Some(run_id) = run_id {
            attributes.push(KeyValue::new(keys::RUN_ID, run_id.to_owned()));
        }
        Resource::builder().with_attributes(attributes).build()
    }

    /// Install OTLP export for the capsule. Mirrors `init` but composes no
    /// `JackinDiagnosticsLayer` (the capsule has no JSONL run) and stamps the
    /// capsule resource. The shared preamble (`ensure_grpc_protocol`, the
    /// dedicated `otel_runtime().enter()` guard, the `with_tonic()` exporter and
    /// Batch-processor builds) duplicates `init` because the layer composition
    /// differs structurally; a change to any of that setup must touch both.
    pub(super) fn init_capsule(
        session_id: &str,
        run_id: Option<&str>,
        traceparent: Option<&str>,
        endpoint: &str,
    ) -> anyhow::Result<()> {
        ensure_grpc_protocol().map_err(|e| anyhow::anyhow!(e))?;
        let endpoint = grpc_endpoint(endpoint);
        let runtime = otel_runtime()?;
        let app_handle = tokio::runtime::Handle::try_current().ok();
        let _runtime_guard = runtime.enter();
        let span_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.clone())
            .build()
            .map_err(|e| anyhow::anyhow!("OTLP span exporter init failed: {e}"))?;
        let log_exporter = opentelemetry_otlp::LogExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.clone())
            .build()
            .map_err(|e| anyhow::anyhow!("OTLP log exporter init failed: {e}"))?;

        let resource = capsule_resource(session_id, run_id);
        let tracer_provider = SdkTracerProvider::builder()
            .with_span_processor(BatchSpanProcessor::builder(span_exporter, Tokio).build())
            .with_resource(resource.clone())
            .build();
        let logger_provider = SdkLoggerProvider::builder()
            .with_log_processor(BatchLogProcessor::builder(log_exporter, Tokio).build())
            .with_resource(resource.clone())
            .build();
        let meter_provider = init_metrics(&resource, &endpoint, app_handle).ok();

        let tracer = tracer_provider.tracer("jackin");
        let span_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        let log_layer = OpenTelemetryTracingBridge::new(&logger_provider);

        let directive = export_filter_directive(export_level(capsule_debug()));
        // Surface OTLP exporter/SDK diagnostics (export failures, refused
        // endpoint, gRPC errors) to the capsule's stderr — captured by
        // `docker logs` and mirrored into `multiplexer.log`. The OTLP span/log
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
            .with(span_layer.with_filter(EnvFilter::new(directive.clone())))
            .with(log_layer.with_filter(EnvFilter::new(directive)))
            .with(otlp_diag_layer)
            .try_init()
            .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"));
        if installed.is_ok() {
            drop(PROVIDERS.set(OtlpProviders {
                tracer: tracer_provider,
                logger: logger_provider,
                meter: meter_provider,
            }));
            emit_session_start(session_id, traceparent);
        }
        installed
    }

    /// `JACKIN_DEBUG` truthiness, the same switch the host `--debug` flag sets
    /// and passes into the container.
    fn capsule_debug() -> bool {
        std::env::var("JACKIN_DEBUG").is_ok_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
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
        "jackin_diagnostics::jsonl",
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
        "jackin_runtime",
        "jackin_term",
        "jackin_tui",
        "jackin_tui_lookbook",
        "jackin_usage",
    ];

    fn export_filter_directive(level: &str) -> String {
        export_filter_directive_with_internal(
            level,
            std::env::var("JACKIN_OTEL_INTERNAL")
                .is_ok_and(|value| crate::run::flag_is_truthy(&value)),
        )
    }

    fn export_level(debug: bool) -> &'static str {
        match crate::telemetry_level(debug) {
            crate::TelemetryLevel::Info => "info",
            crate::TelemetryLevel::Debug => "debug",
            crate::TelemetryLevel::Trace => "trace",
        }
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

    #[cfg(test)]
    pub(super) struct TestExport {
        pub(super) spans: opentelemetry_sdk::trace::InMemorySpanExporter,
        pub(super) logs: opentelemetry_sdk::logs::InMemoryLogExporter,
        pub(super) tracer_provider: SdkTracerProvider,
        pub(super) logger_provider: SdkLoggerProvider,
    }

    #[cfg(test)]
    pub(super) fn test_layers(debug: bool, run_id: &str) -> (TestExport, impl tracing::Subscriber) {
        use opentelemetry::trace::TracerProvider as _;

        let spans = opentelemetry_sdk::trace::InMemorySpanExporter::default();
        let logs = opentelemetry_sdk::logs::InMemoryLogExporter::default();
        let resource = resource(run_id);
        let tracer_provider = SdkTracerProvider::builder()
            .with_simple_exporter(spans.clone())
            .with_resource(resource.clone())
            .build();
        let logger_provider = SdkLoggerProvider::builder()
            .with_simple_exporter(logs.clone())
            .with_resource(resource)
            .build();
        let tracer = tracer_provider.tracer("jackin");
        let span_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        let log_layer = OpenTelemetryTracingBridge::new(&logger_provider);
        let directive = export_filter_directive(export_level(debug));
        let subscriber = tracing_subscriber::registry()
            .with(JackinDiagnosticsLayer)
            .with(span_layer.with_filter(EnvFilter::new(directive.clone())))
            .with(log_layer.with_filter(EnvFilter::new(directive)));

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

    /// Emit the session-start marker: a short span in its own trace, linked to
    /// the launch span (from the propagated traceparent), carrying `session.id`.
    /// This is the entry node that joins the launch trace to the session;
    /// per-activity traces share `session.id` rather than nesting under one
    /// long-lived span.
    fn emit_session_start(session_id: &str, traceparent: Option<&str>) {
        use opentelemetry::Context;
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;

        let span = tracing::info_span!("capsule.session", otel.name = "capsule:session");
        drop(span.set_parent(Context::new()));
        span.set_attribute(keys::SESSION_ID, session_id.to_owned());
        span.set_attribute(keys::COMPONENT, "capsule");
        if let Some(ctx) = traceparent.and_then(parse_traceparent) {
            span.add_link(ctx);
        }
        // The span ends here (a marker): the link + session.id are what join
        // the launch trace to the session timeline.
        span.in_scope(|| {
            tracing::info!(target: "jackin_diagnostics::session", "capsule session started");
        });
    }

    /// Parse a W3C `traceparent` header into a remote `SpanContext`.
    fn parse_traceparent(value: &str) -> Option<opentelemetry::trace::SpanContext> {
        use opentelemetry::trace::{SpanContext, SpanId, TraceFlags, TraceId, TraceState};

        let mut parts = value.split('-');
        let version = parts.next()?;
        let trace_id = parts.next()?;
        let span_id = parts.next()?;
        let flags = parts.next()?;
        if version != "00" || parts.next().is_some() {
            return None;
        }
        Some(SpanContext::new(
            TraceId::from_hex(trace_id).ok()?,
            SpanId::from_hex(span_id).ok()?,
            TraceFlags::new(u8::from_str_radix(flags, 16).ok()?),
            true,
            TraceState::default(),
        ))
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
        use opentelemetry::KeyValue;
        use opentelemetry::metrics::MeterProvider as _;
        use std::sync::Mutex;

        let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_endpoint(metrics_endpoint.to_owned())
            .build()
            .map_err(|e| anyhow::anyhow!("OTLP metric exporter init failed: {e}"))?;
        let reader = PeriodicReader::builder(metric_exporter, Tokio)
            .with_interval(std::time::Duration::from_secs(5))
            .build();
        let provider = SdkMeterProvider::builder()
            .with_reader(reader)
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
            drop(
                meter
                    // semconv: process.cpu.utilization, unit "1", 0..1 fraction
                    // of the CPUs available to the process.
                    .f64_observable_gauge("process.cpu.utilization")
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
                    .build(),
            );
            drop(
                meter
                    // semconv: process.memory.usage is an UpDownCounter (rises
                    // and falls), not a gauge.
                    .i64_observable_up_down_counter("process.memory.usage")
                    .with_unit("By")
                    .with_description("Resident set size of the jackin process")
                    .with_callback(move |observer| {
                        if let Some((_, memory_bytes)) =
                            sampler.lock().ok().and_then(|mut s| s.sample())
                        {
                            observer.observe(i64::try_from(memory_bytes).unwrap_or(i64::MAX), &[]);
                        }
                    })
                    .build(),
            );
        }

        if let Some(handle) = app_handle {
            let workers = handle.clone();
            drop(
                meter
                    .u64_observable_gauge("tokio.runtime.workers")
                    .with_description("Worker threads driving the tokio runtime")
                    .with_callback(move |observer| {
                        observer.observe(workers.metrics().num_workers() as u64, &[]);
                    })
                    .build(),
            );
            let alive = handle.clone();
            drop(
                meter
                    .u64_observable_gauge("tokio.runtime.alive.tasks")
                    .with_description("Tasks currently alive in the tokio runtime")
                    .with_callback(move |observer| {
                        observer.observe(alive.metrics().num_alive_tasks() as u64, &[]);
                    })
                    .build(),
            );
            drop(
                meter
                    .u64_observable_gauge("tokio.runtime.global.queue.depth")
                    .with_description("Tasks waiting in the tokio runtime's global queue")
                    .with_callback(move |observer| {
                        observer.observe(handle.metrics().global_queue_depth() as u64, &[]);
                    })
                    .build(),
            );
        }

        drop(
            meter
                .u64_observable_counter("jackin.diagnostics.events")
                .with_description("Diagnostics events recorded during the active jackin run")
                .with_callback(|observer| {
                    let Some(run) = crate::active_run() else {
                        return;
                    };
                    let snapshot = run.domain_metrics_snapshot();
                    for (kind, count) in snapshot.event_counts {
                        observer.observe(count, &[KeyValue::new("kind", kind)]);
                    }
                })
                .build(),
        );
        drop(
            meter
                .u64_observable_counter("jackin.cache.hits")
                .with_description("Cache-hit diagnostics recorded during the active jackin run")
                .with_callback(|observer| {
                    if let Some(run) = crate::active_run() {
                        observer.observe(run.domain_metrics_snapshot().cache_hits, &[]);
                    }
                })
                .build(),
        );
        drop(
            meter
                .u64_observable_counter("jackin.cache.misses")
                .with_description("Cache-miss diagnostics recorded during the active jackin run")
                .with_callback(|observer| {
                    if let Some(run) = crate::active_run() {
                        observer.observe(run.domain_metrics_snapshot().cache_misses, &[]);
                    }
                })
                .build(),
        );

        Ok(provider)
    }

    pub(super) fn shutdown() {
        if let Some(providers) = PROVIDERS.get() {
            providers.flush_and_shutdown();
        }
    }

    #[cfg(test)]
    mod tests;
}

pub(crate) fn emit_jsonl_event(
    run_id: &str,
    kind: &str,
    message: &str,
    stage: Option<&str>,
    detail: Option<&str>,
) {
    emit_jsonl_event_with_level(
        run_id,
        kind,
        message,
        stage,
        detail,
        None,
        JsonlEventLevel::Info,
    );
}

pub(crate) fn emit_jsonl_error(
    run_id: &str,
    kind: &str,
    message: &str,
    stage: Option<&str>,
    detail: Option<&str>,
) {
    emit_jsonl_error_typed(run_id, kind, message, stage, detail, None);
}

pub(crate) fn emit_jsonl_error_typed(
    run_id: &str,
    kind: &str,
    message: &str,
    stage: Option<&str>,
    detail: Option<&str>,
    error_type: Option<&str>,
) {
    emit_jsonl_event_with_level(
        run_id,
        kind,
        message,
        stage,
        detail,
        error_type,
        JsonlEventLevel::Error,
    );
}

enum JsonlEventLevel {
    Info,
    Error,
}

pub(crate) struct EventTaxonomy {
    pub event_name: String,
    pub outcome: &'static str,
    pub component: &'static str,
    pub operation: String,
    pub category: String,
}

pub(crate) fn event_taxonomy(
    kind: &str,
    message: &str,
    stage: Option<&str>,
    detail: Option<&str>,
    error_type: Option<&str>,
    level: &str,
) -> EventTaxonomy {
    let event_name = kind.replace('_', ".");
    EventTaxonomy {
        operation: operation_for(kind, stage, &event_name),
        category: category_for(kind, stage, detail),
        outcome: outcome_for(kind, error_type, level),
        component: component_for(kind, message),
        event_name,
    }
}

fn operation_for(kind: &str, stage: Option<&str>, event_name: &str) -> String {
    match kind {
        "stage_started" | "stage_done" | "stage_failed" | "stage_skipped" => stage.map_or_else(
            || "stage".to_owned(),
            |stage| format!("stage.{}", normalize_taxonomy_value(stage)),
        ),
        "timing_started" | "timing_done" => stage.map_or_else(
            || "timing".to_owned(),
            |stage| format!("timing.{}", normalize_taxonomy_value(stage)),
        ),
        "debug" => "debug".to_owned(),
        _ => event_name.to_owned(),
    }
}

fn category_for(kind: &str, stage: Option<&str>, detail: Option<&str>) -> String {
    match kind {
        "debug" => detail.map_or_else(|| "debug".to_owned(), normalize_taxonomy_value),
        kind if kind.starts_with("docker_") || kind.starts_with("container_") => {
            "docker".to_owned()
        }
        kind if kind.starts_with("stage_") => "launch".to_owned(),
        kind if kind.starts_with("timing_") => stage.map_or_else(
            || "timing".to_owned(),
            |stage| format!("timing.{}", normalize_taxonomy_value(stage)),
        ),
        "subprocess_done" => "process".to_owned(),
        "otlp_internal" => "telemetry".to_owned(),
        "run_summary" => "summary".to_owned(),
        "slow_foreground_wait" => "performance".to_owned(),
        other => other.split_once('_').map_or_else(
            || normalize_taxonomy_value(other),
            |(prefix, _)| normalize_taxonomy_value(prefix),
        ),
    }
}

fn outcome_for(kind: &str, error_type: Option<&str>, level: &str) -> &'static str {
    if error_type.is_some()
        || level.eq_ignore_ascii_case("ERROR")
        || kind.contains("failed")
        || kind.contains("failure")
        || kind.contains("crash")
    {
        "failure"
    } else if kind.contains("skipped") {
        "skipped"
    } else if kind.contains("started") {
        "started"
    } else if kind.contains("cache_miss") {
        "cache_miss"
    } else {
        "success"
    }
}

fn component_for(kind: &str, message: &str) -> &'static str {
    if message.starts_with("[jackin-capsule") || kind.starts_with("capsule_") {
        "capsule"
    } else {
        "host"
    }
}

fn normalize_taxonomy_value(value: &str) -> String {
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

fn emit_jsonl_event_with_level(
    run_id: &str,
    kind: &str,
    message: &str,
    stage: Option<&str>,
    detail: Option<&str>,
    error_type: Option<&str>,
    level: JsonlEventLevel,
) {
    let message = crate::redact::redact_text(message);
    let detail = detail.map(crate::redact::redact_text);
    let detail = detail.as_ref().map(AsRef::as_ref);
    let taxonomy = event_taxonomy(
        kind,
        message.as_ref(),
        stage,
        detail,
        error_type,
        match level {
            JsonlEventLevel::Info => "INFO",
            JsonlEventLevel::Error => "ERROR",
        },
    );
    let span_id = tracing::Span::current()
        .id()
        .map(|id| id.into_u64().to_string());
    let run = crate::run::run_by_id(run_id).or_else(crate::active_run);
    if let Some(run) = run {
        run.record_from_layer(
            kind,
            message.as_ref(),
            stage,
            detail,
            span_id.as_deref(),
            if kind == "debug" && !matches!(level, JsonlEventLevel::Error) {
                "DEBUG"
            } else {
                match level {
                    JsonlEventLevel::Info => "INFO",
                    JsonlEventLevel::Error => "ERROR",
                }
            },
        );
    }

    // The `--debug` firehose is DEBUG-severity so external exporters filter
    // it by level; the JSONL layer ignores levels and records everything.
    // The trailing format message becomes the OTLP log body — without it,
    // exported records carry attributes but an empty body.
    if kind == "debug" && !matches!(level, JsonlEventLevel::Error) {
        emit_debug_jsonl_event(
            run_id,
            kind,
            message.as_ref(),
            stage,
            detail,
            error_type,
            &taxonomy,
        );
    } else if matches!(level, JsonlEventLevel::Error) {
        emit_error_jsonl_event(
            run_id,
            kind,
            message.as_ref(),
            stage,
            detail,
            error_type,
            &taxonomy,
        );
    } else {
        emit_info_jsonl_event(
            run_id,
            kind,
            message.as_ref(),
            stage,
            detail,
            error_type,
            &taxonomy,
        );
    }
}

macro_rules! emit_jsonl_event_fields {
    ($emit:ident, $run_id:expr, $kind:expr, $message:expr, $stage:expr, $detail:expr, $error_type:expr, $taxonomy:expr) => {
        match ($stage, $detail, $error_type) {
            (Some(stage), Some(detail), Some(error_type)) => tracing::$emit!(
                target: JSONL_TARGET,
                run_id = $run_id,
                kind = $kind,
                event.name = $taxonomy.event_name.as_str(),
                event.outcome = $taxonomy.outcome,
                jackin.component = $taxonomy.component,
                jackin.operation = $taxonomy.operation.as_str(),
                jackin.category = $taxonomy.category.as_str(),
                stage = stage,
                detail = detail,
                error_type = error_type,
                "{}", $message
            ),
            (Some(stage), Some(detail), None) => tracing::$emit!(
                target: JSONL_TARGET,
                run_id = $run_id,
                kind = $kind,
                event.name = $taxonomy.event_name.as_str(),
                event.outcome = $taxonomy.outcome,
                jackin.component = $taxonomy.component,
                jackin.operation = $taxonomy.operation.as_str(),
                jackin.category = $taxonomy.category.as_str(),
                stage = stage,
                detail = detail,
                "{}", $message
            ),
            (Some(stage), None, Some(error_type)) => tracing::$emit!(
                target: JSONL_TARGET,
                run_id = $run_id,
                kind = $kind,
                event.name = $taxonomy.event_name.as_str(),
                event.outcome = $taxonomy.outcome,
                jackin.component = $taxonomy.component,
                jackin.operation = $taxonomy.operation.as_str(),
                jackin.category = $taxonomy.category.as_str(),
                stage = stage,
                error_type = error_type,
                "{}", $message
            ),
            (Some(stage), None, None) => tracing::$emit!(
                target: JSONL_TARGET,
                run_id = $run_id,
                kind = $kind,
                event.name = $taxonomy.event_name.as_str(),
                event.outcome = $taxonomy.outcome,
                jackin.component = $taxonomy.component,
                jackin.operation = $taxonomy.operation.as_str(),
                jackin.category = $taxonomy.category.as_str(),
                stage = stage,
                "{}", $message
            ),
            (None, Some(detail), Some(error_type)) => tracing::$emit!(
                target: JSONL_TARGET,
                run_id = $run_id,
                kind = $kind,
                event.name = $taxonomy.event_name.as_str(),
                event.outcome = $taxonomy.outcome,
                jackin.component = $taxonomy.component,
                jackin.operation = $taxonomy.operation.as_str(),
                jackin.category = $taxonomy.category.as_str(),
                detail = detail,
                error_type = error_type,
                "{}", $message
            ),
            (None, Some(detail), None) => tracing::$emit!(
                target: JSONL_TARGET,
                run_id = $run_id,
                kind = $kind,
                event.name = $taxonomy.event_name.as_str(),
                event.outcome = $taxonomy.outcome,
                jackin.component = $taxonomy.component,
                jackin.operation = $taxonomy.operation.as_str(),
                jackin.category = $taxonomy.category.as_str(),
                detail = detail,
                "{}", $message
            ),
            (None, None, Some(error_type)) => tracing::$emit!(
                target: JSONL_TARGET,
                run_id = $run_id,
                kind = $kind,
                event.name = $taxonomy.event_name.as_str(),
                event.outcome = $taxonomy.outcome,
                jackin.component = $taxonomy.component,
                jackin.operation = $taxonomy.operation.as_str(),
                jackin.category = $taxonomy.category.as_str(),
                error_type = error_type,
                "{}", $message
            ),
            (None, None, None) => tracing::$emit!(
                target: JSONL_TARGET,
                run_id = $run_id,
                kind = $kind,
                event.name = $taxonomy.event_name.as_str(),
                event.outcome = $taxonomy.outcome,
                jackin.component = $taxonomy.component,
                jackin.operation = $taxonomy.operation.as_str(),
                jackin.category = $taxonomy.category.as_str(),
                "{}", $message
            ),
        }
    }
}

fn emit_info_jsonl_event(
    run_id: &str,
    kind: &str,
    message: &str,
    stage: Option<&str>,
    detail: Option<&str>,
    error_type: Option<&str>,
    taxonomy: &EventTaxonomy,
) {
    emit_jsonl_event_fields!(
        info, run_id, kind, message, stage, detail, error_type, taxonomy
    );
}

fn emit_debug_jsonl_event(
    run_id: &str,
    kind: &str,
    message: &str,
    stage: Option<&str>,
    detail: Option<&str>,
    error_type: Option<&str>,
    taxonomy: &EventTaxonomy,
) {
    emit_jsonl_event_fields!(
        debug, run_id, kind, message, stage, detail, error_type, taxonomy
    );
}

fn emit_error_jsonl_event(
    run_id: &str,
    kind: &str,
    message: &str,
    stage: Option<&str>,
    detail: Option<&str>,
    error_type: Option<&str>,
    taxonomy: &EventTaxonomy,
) {
    emit_jsonl_event_fields!(
        error, run_id, kind, message, stage, detail, error_type, taxonomy
    );
}

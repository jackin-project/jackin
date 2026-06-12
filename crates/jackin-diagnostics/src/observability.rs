//! `tracing` subscriber setup for JSONL diagnostics plus optional OTLP export.
//!
//! The default subscriber installs only [`JackinDiagnosticsLayer`]. It has no
//! stdout/stderr sink: diagnostic output must never stream over the operator's
//! full-screen TUI or plain CLI surface. With `--features otlp` and
//! `JACKIN_OTLP_ENDPOINT` set, an OTLP export layer is added beside the JSONL
//! layer.

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;

const JSONL_TARGET: &str = "jackin_diagnostics::jsonl";

/// OTLP/tracing attribute keys — the single source of truth for jackin's
/// telemetry tag taxonomy. The `jackin.*` keys are dotted (`namespace.entity.
/// field`, semconv-idiomatic); `service.*` and `session.*` reuse OpenTelemetry standard
/// namespaces rather than inventing jackin-specific equivalents. Instrumentation
/// sites across the host TUI, launch flow, and capsule reference these constants
/// so a key is spelled exactly once.
pub mod otel_keys {
    // OTel standard namespaces (do not invent jackin equivalents).
    pub const SERVICE_NAME: &str = "service.name";
    pub const SERVICE_VERSION: &str = "service.version";
    /// Standard OpenTelemetry session id — used to group all telemetry from one capsule
    /// session into a single timeline (see the `session` semconv).
    pub const SESSION_ID: &str = "session.id";
    pub const SESSION_PREVIOUS_ID: &str = "session.previous_id";

    // jackin custom namespace (no OTel standard equivalent exists).
    /// CLI-invocation id; correlates every trace/log/metric of one `jackin` run.
    pub const RUN_ID: &str = "jackin.run.id";
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
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        if event.metadata().target() != JSONL_TARGET {
            return;
        }
        let mut visitor = DiagnosticsEventVisitor::default();
        event.record(&mut visitor);
        if !visitor.jackin_jsonl {
            return;
        }
        let Some(kind) = visitor.kind.as_deref() else {
            return;
        };
        let Some(message) = visitor.message.as_deref() else {
            return;
        };
        let span_id = ctx.event_scope(event).and_then(|scope| {
            scope
                .from_root()
                .last()
                .map(|span| span.id().into_u64().to_string())
        });
        let run = visitor
            .run_id
            .as_deref()
            .and_then(crate::run::run_by_id)
            .or_else(crate::active_run);
        if let Some(run) = run {
            run.record_from_layer(
                kind,
                message,
                visitor.stage.as_deref(),
                visitor.detail.as_deref(),
                span_id.as_deref(),
            );
        }
    }
}

#[derive(Default)]
struct DiagnosticsEventVisitor {
    jackin_jsonl: bool,
    run_id: Option<String>,
    kind: Option<String>,
    message: Option<String>,
    stage: Option<String>,
    detail: Option<String>,
}

impl Visit for DiagnosticsEventVisitor {
    fn record_bool(&mut self, field: &Field, value: bool) {
        if field.name() == "jackin_jsonl" {
            self.jackin_jsonl = value;
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_owned(field, value.to_owned());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_owned(field, format!("{value:?}"));
    }
}

impl DiagnosticsEventVisitor {
    fn record_owned(&mut self, field: &Field, value: String) {
        match field.name() {
            "run_id" => self.run_id = Some(value),
            "kind" => self.kind = Some(value),
            "diagnostics_message" | "message" => self.message = Some(value),
            "stage" if value != "<none>" => self.stage = Some(value),
            "detail" if value != "<none>" => self.detail = Some(value),
            _ => {}
        }
    }
}

/// Install the global `tracing` subscriber.
///
/// Default build: installs the JSONL diagnostics layer and no terminal sink.
/// With `--features otlp` and an OTLP endpoint configured
/// (`JACKIN_OTLP_ENDPOINT`, falling back to `OTEL_EXPORTER_OTLP_ENDPOINT`),
/// installs OTLP span, log, and metric export beside the JSONL layer, with the
/// diagnostics run id stamped on the OTLP resource so an external backend
/// (e.g. Parallax) can answer "show me run `<id>`".
///
/// Returns `Ok(())` on success; the OTLP path returns an error if the global
/// subscriber is already set (e.g. a test that installs twice).
// `allow`, not `expect`: the body is trivially const only in the default
// (no-otlp) build; the otlp build does non-const setup, so the lint fires in one
// cfg and not the other and a single non-const signature is required.
#[allow(clippy::missing_const_for_fn)]
pub fn init_tracing(debug: bool, run_id: &str) -> anyhow::Result<()> {
    #[cfg(feature = "otlp")]
    {
        if let Some(endpoint) = otlp::endpoint() {
            return otlp::init(debug, run_id, &endpoint);
        }
    }

    // No fmt layer: the operator's terminal must never receive the firehose.
    let _ = (debug, run_id);
    tracing_subscriber::registry()
        .with(JackinDiagnosticsLayer)
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"))
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
    use opentelemetry_sdk::metrics::SdkMeterProvider;
    use opentelemetry_sdk::trace::SdkTracerProvider;
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
        fn flush_and_shutdown(&self) {
            drop(self.tracer.force_flush());
            drop(self.tracer.shutdown());
            drop(self.logger.force_flush());
            drop(self.logger.shutdown());
            if let Some(meter) = &self.meter {
                drop(meter.force_flush());
                drop(meter.shutdown());
            }
        }
    }

    static PROVIDERS: OnceLock<OtlpProviders> = OnceLock::new();

    /// The OTLP endpoint, when configured. `JACKIN_OTLP_ENDPOINT` wins over
    /// the standard `OTEL_EXPORTER_OTLP_ENDPOINT`, which wrappers such as
    /// `parallax run start -- jackin …` inject without jackin'-specific
    /// knowledge.
    pub(super) fn endpoint() -> Option<String> {
        resolve_endpoint(
            std::env::var("JACKIN_OTLP_ENDPOINT").ok(),
            std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
        )
    }

    /// `JACKIN_OTLP_ENDPOINT` wins; an empty value falls through to the next
    /// candidate (an exported-but-empty var must not produce a blank endpoint);
    /// neither set yields `None` and no OTLP layer is installed.
    fn resolve_endpoint(jackin: Option<String>, otel: Option<String>) -> Option<String> {
        [jackin, otel]
            .into_iter()
            .flatten()
            .find(|s| !s.is_empty())
    }

    /// Per-signal OTLP/HTTP URL: a bare base endpoint (`http://host:4318`)
    /// gets the standard signal path appended; an endpoint that already ends
    /// with *this* signal's path is used verbatim. The check is `ends_with` on
    /// the matching `signal_path`, not a loose `contains("/v1/")`: a base of
    /// `http://host/v1/traces` must still get `/v1/logs` and `/v1/metrics`
    /// appended for the other two signals rather than posting all three to the
    /// traces path. (`with_endpoint` is used verbatim by the SDK — it only
    /// auto-appends for the env-var path, which jackin reads itself.)
    fn signal_url(endpoint: &str, signal_path: &str) -> String {
        let trimmed = endpoint.trim_end_matches('/');
        if trimmed.ends_with(signal_path) {
            return trimmed.to_owned();
        }
        format!("{trimmed}/{signal_path}")
    }

    /// The OTLP resource. `service.name` is always `jackin`; the diagnostics
    /// run id rides as `jackin.run.id` (dotted, semconv-idiomatic) so backends
    /// can correlate telemetry with the run JSONL the operator shares.
    /// `jackin.component` marks this process as the host (the in-container
    /// capsule stamps `capsule`). `parallax.run_id` is set to the same id
    /// unless a wrapper already provided one via `OTEL_RESOURCE_ATTRIBUTES`
    /// (then the wrapper's grouping wins and the env detector supplies it).
    fn resource(run_id: &str) -> Resource {
        let wrapper_supplied =
            std::env::var("OTEL_RESOURCE_ATTRIBUTES").is_ok_and(|v| v.contains("parallax.run_id="));
        build_resource(run_id, wrapper_supplied)
    }

    fn build_resource(run_id: &str, wrapper_supplied: bool) -> Resource {
        let mut attributes = vec![
            KeyValue::new(keys::SERVICE_NAME, "jackin"),
            KeyValue::new(keys::SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
            KeyValue::new(keys::RUN_ID, run_id.to_owned()),
            KeyValue::new(keys::COMPONENT, "host"),
        ];
        if !wrapper_supplied {
            attributes.push(KeyValue::new("parallax.run_id", run_id.to_owned()));
        }
        Resource::builder().with_attributes(attributes).build()
    }

    pub(super) fn init(debug: bool, run_id: &str, endpoint: &str) -> anyhow::Result<()> {
        let span_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(signal_url(endpoint, "v1/traces"))
            .build()
            .map_err(|e| anyhow::anyhow!("OTLP span exporter init failed: {e}"))?;
        let log_exporter = opentelemetry_otlp::LogExporter::builder()
            .with_http()
            .with_endpoint(signal_url(endpoint, "v1/logs"))
            .build()
            .map_err(|e| anyhow::anyhow!("OTLP log exporter init failed: {e}"))?;

        let resource = resource(run_id);
        let tracer_provider = SdkTracerProvider::builder()
            .with_batch_exporter(span_exporter)
            .with_resource(resource.clone())
            .build();
        let logger_provider = SdkLoggerProvider::builder()
            .with_batch_exporter(log_exporter)
            .with_resource(resource.clone())
            .build();
        // Metrics are best-effort: a failed exporter build must never block
        // span/log telemetry or the run itself. Defer reporting the failure —
        // emitting here would predate `try_init()` and the message would hit no
        // subscriber, so the one diagnostic this branch exists to surface would
        // be dropped on the floor.
        let (meter_provider, metric_error) = match init_metrics(&resource, endpoint) {
            Ok(provider) => (Some(provider), None),
            Err(error) => (None, Some(error)),
        };

        let tracer = tracer_provider.tracer("jackin");
        let span_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        let log_layer = OpenTelemetryTracingBridge::new(&logger_provider);

        let level = if debug { "debug" } else { "info" };
        // Scope the export to jackin's own telemetry. Silencing the OTLP/HTTP
        // transport stack stops the log bridge from re-exporting the exporter's
        // own request logs (a feedback loop under `--debug`) and keeps the
        // backend free of dependency-internal spans the operator never asked for.
        let directive = format!(
            "{level},hyper=off,h2=off,tower=off,tonic=off,reqwest=off,\
             opentelemetry=off,opentelemetry_sdk=off,opentelemetry_otlp=off"
        );
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
    /// alive tasks, global queue depth) read from the runtime handle captured
    /// here at init. Observations run on the exporter's collect thread, so
    /// the handle is captured eagerly — `Handle::current()` would panic
    /// there.
    fn init_metrics(resource: &Resource, endpoint: &str) -> anyhow::Result<SdkMeterProvider> {
        use opentelemetry::metrics::MeterProvider as _;
        use std::sync::Mutex;

        let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_http()
            .with_endpoint(signal_url(endpoint, "v1/metrics"))
            .build()
            .map_err(|e| anyhow::anyhow!("OTLP metric exporter init failed: {e}"))?;
        let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(metric_exporter)
            .with_interval(std::time::Duration::from_secs(5))
            .build();
        let provider = SdkMeterProvider::builder()
            .with_reader(reader)
            .with_resource(resource.clone())
            .build();
        let meter = provider.meter("jackin");

        if let Ok(pid) = sysinfo::get_current_pid() {
            let cpu_count = std::thread::available_parallelism()
                .map_or(1, std::num::NonZeroUsize::get) as f64;
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
                            // sysinfo reports percent of one core; semconv
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

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
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
                    .u64_observable_gauge("tokio.runtime.alive_tasks")
                    .with_description("Tasks currently alive in the tokio runtime")
                    .with_callback(move |observer| {
                        observer.observe(alive.metrics().num_alive_tasks() as u64, &[]);
                    })
                    .build(),
            );
            drop(
                meter
                    .u64_observable_gauge("tokio.runtime.global_queue_depth")
                    .with_description("Tasks waiting in the tokio runtime's global queue")
                    .with_callback(move |observer| {
                        observer.observe(handle.metrics().global_queue_depth() as u64, &[]);
                    })
                    .build(),
            );
        }

        Ok(provider)
    }

    pub(super) fn shutdown() {
        if let Some(providers) = PROVIDERS.get() {
            providers.flush_and_shutdown();
        }
    }

    #[cfg(test)]
    mod tests {
        use opentelemetry::Key;

        use super::keys;
        use super::{build_resource, resolve_endpoint, signal_url};

        fn attr(resource: &opentelemetry_sdk::Resource, key: &'static str) -> Option<String> {
            resource
                .get(&Key::from_static_str(key))
                .map(|value| value.to_string())
        }

        #[test]
        fn bare_endpoint_gets_signal_path() {
            assert_eq!(
                signal_url("http://127.0.0.1:4318", "v1/traces"),
                "http://127.0.0.1:4318/v1/traces"
            );
            assert_eq!(
                signal_url("http://127.0.0.1:4318/", "v1/logs"),
                "http://127.0.0.1:4318/v1/logs"
            );
        }

        #[test]
        fn trailing_slashes_are_all_trimmed_before_append() {
            assert_eq!(
                signal_url("http://127.0.0.1:4318//", "v1/traces"),
                "http://127.0.0.1:4318/v1/traces"
            );
        }

        #[test]
        fn matching_signal_path_is_used_verbatim() {
            assert_eq!(
                signal_url("http://otlp.internal/v1/traces", "v1/traces"),
                "http://otlp.internal/v1/traces"
            );
        }

        #[test]
        fn non_matching_signal_path_still_appends() {
            // A traces-specific base reused for the logs signal must not be
            // sent to the traces path: `ends_with` only short-circuits the
            // signal that actually matches, so logs get their own path.
            assert_eq!(
                signal_url("http://otlp.internal/v1/traces", "v1/logs"),
                "http://otlp.internal/v1/traces/v1/logs"
            );
        }

        #[test]
        fn endpoint_precedence_and_empty_filtering() {
            // JACKIN wins over OTEL.
            assert_eq!(
                resolve_endpoint(Some("http://jk:4318".into()), Some("http://otel:4318".into())),
                Some("http://jk:4318".into())
            );
            // OTEL is the fallback.
            assert_eq!(
                resolve_endpoint(None, Some("http://otel:4318".into())),
                Some("http://otel:4318".into())
            );
            // An exported-but-empty JACKIN var falls through to OTEL.
            assert_eq!(
                resolve_endpoint(Some(String::new()), Some("http://otel:4318".into())),
                Some("http://otel:4318".into())
            );
            // Empty on both → None (no malformed exporter against "").
            assert_eq!(resolve_endpoint(Some(String::new()), Some(String::new())), None);
            // Neither set → None (no OTLP layer installed).
            assert_eq!(resolve_endpoint(None, None), None);
        }

        #[test]
        fn resource_carries_service_name_run_id_and_component() {
            let resource = build_resource("jk-run-0a1b2c", false);
            assert_eq!(attr(&resource, keys::SERVICE_NAME), Some("jackin".into()));
            assert_eq!(attr(&resource, keys::RUN_ID), Some("jk-run-0a1b2c".into()));
            assert_eq!(attr(&resource, keys::COMPONENT), Some("host".into()));
            // No wrapper → jackin supplies its own parallax grouping key.
            assert_eq!(
                attr(&resource, "parallax.run_id"),
                Some("jk-run-0a1b2c".into())
            );
        }

        #[test]
        fn wrapper_supplied_parallax_id_is_not_double_stamped() {
            // A wrapper injected parallax.run_id via OTEL_RESOURCE_ATTRIBUTES;
            // jackin must not add its own, letting the wrapper's grouping win.
            let resource = build_resource("jk-run-x", true);
            assert_eq!(attr(&resource, "parallax.run_id"), None);
            // jackin's own keys still ride.
            assert_eq!(attr(&resource, keys::RUN_ID), Some("jk-run-x".into()));
        }
    }
}

pub(crate) fn emit_jsonl_event(
    run_id: &str,
    kind: &str,
    message: &str,
    stage: Option<&str>,
    detail: Option<&str>,
) {
    let stage = stage.unwrap_or("<none>");
    let detail = detail.unwrap_or("<none>");
    // The `--debug` firehose is DEBUG-severity so external exporters filter
    // it by level; the JSONL layer ignores levels and records everything.
    // The trailing format message becomes the OTLP log body — without it,
    // exported records carry attributes but an empty body.
    if kind == "debug" {
        tracing::debug!(
            target: JSONL_TARGET,
            jackin_jsonl = true,
            run_id = run_id,
            kind = kind,
            diagnostics_message = message,
            stage = stage,
            detail = detail,
            "{message}"
        );
    } else {
        tracing::info!(
            target: JSONL_TARGET,
            jackin_jsonl = true,
            run_id = run_id,
            kind = kind,
            diagnostics_message = message,
            stage = stage,
            detail = detail,
            "{message}"
        );
    }
}

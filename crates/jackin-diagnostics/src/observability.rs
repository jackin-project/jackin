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
/// installs OTLP span + log export beside the JSONL layer, with the
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
/// Batch exporters hold the tail of a run in memory; a short CLI run that
/// exits without this call silently drops its last spans and log records.
/// No-op in default builds and when no endpoint was configured.
#[allow(clippy::missing_const_for_fn)]
pub fn shutdown_otlp() {
    #[cfg(feature = "otlp")]
    otlp::shutdown();
}

/// OTLP export: spans (stage timings) and logs (the diagnostics event
/// stream) to one endpoint. Only compiled with `--features otlp`; entirely
/// absent from default builds so there is zero link-time cost. No `fmt`
/// layer is attached: OTLP export is a separate sink from the operator's
/// screen, which stays free of the firehose.
#[cfg(feature = "otlp")]
mod otlp {
    use std::sync::OnceLock;

    use opentelemetry::KeyValue;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::Resource;
    use opentelemetry_sdk::logs::SdkLoggerProvider;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::prelude::*;

    use super::JackinDiagnosticsLayer;

    static PROVIDERS: OnceLock<(SdkTracerProvider, SdkLoggerProvider)> = OnceLock::new();

    /// The OTLP endpoint, when configured. `JACKIN_OTLP_ENDPOINT` wins over
    /// the standard `OTEL_EXPORTER_OTLP_ENDPOINT`, which wrappers such as
    /// `parallax run start -- jackin …` inject without jackin'-specific
    /// knowledge.
    pub(super) fn endpoint() -> Option<String> {
        ["JACKIN_OTLP_ENDPOINT", "OTEL_EXPORTER_OTLP_ENDPOINT"]
            .iter()
            .find_map(|key| std::env::var(key).ok().filter(|s| !s.is_empty()))
    }

    /// Per-signal OTLP/HTTP URL: a bare base endpoint (`http://host:4318`)
    /// gets the standard signal path appended; an endpoint that already
    /// names a `/v1/…` path is used verbatim.
    fn signal_url(endpoint: &str, signal_path: &str) -> String {
        if endpoint.contains("/v1/") {
            return endpoint.to_owned();
        }
        format!("{}/{signal_path}", endpoint.trim_end_matches('/'))
    }

    /// The OTLP resource. `service.name` is always `jackin`; the diagnostics
    /// run id rides as `jackin.run_id` so backends can correlate telemetry
    /// with the run JSONL the operator shares. `parallax.run_id` is set to
    /// the same id unless a wrapper already provided one via
    /// `OTEL_RESOURCE_ATTRIBUTES` (then the wrapper's grouping wins and the
    /// env detector supplies it).
    fn resource(run_id: &str) -> Resource {
        let mut attributes = vec![
            KeyValue::new("service.name", "jackin"),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("jackin.run_id", run_id.to_owned()),
        ];
        let wrapper_supplied =
            std::env::var("OTEL_RESOURCE_ATTRIBUTES").is_ok_and(|v| v.contains("parallax.run_id="));
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
            .with_resource(resource)
            .build();

        let tracer = tracer_provider.tracer("jackin");
        let span_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        let log_layer = OpenTelemetryTracingBridge::new(&logger_provider);

        let level = if debug { "debug" } else { "info" };
        let installed = tracing_subscriber::registry()
            .with(JackinDiagnosticsLayer)
            .with(span_layer.with_filter(EnvFilter::new(level)))
            .with(log_layer.with_filter(EnvFilter::new(level)))
            .try_init()
            .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"));
        if installed.is_ok() {
            drop(PROVIDERS.set((tracer_provider, logger_provider)));
        }
        installed
    }

    pub(super) fn shutdown() {
        if let Some((tracer_provider, logger_provider)) = PROVIDERS.get() {
            drop(tracer_provider.force_flush());
            drop(tracer_provider.shutdown());
            drop(logger_provider.force_flush());
            drop(logger_provider.shutdown());
        }
    }

    #[cfg(test)]
    mod tests {
        use opentelemetry::Key;

        use super::{resource, signal_url};

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
        fn explicit_signal_path_is_used_verbatim() {
            assert_eq!(
                signal_url("http://otlp.internal/v1/traces", "v1/traces"),
                "http://otlp.internal/v1/traces"
            );
        }

        #[test]
        fn resource_carries_service_name_and_run_id() {
            let resource = resource("jk-run-0a1b2c");
            assert_eq!(
                resource.get(&Key::from_static_str("service.name")),
                Some("jackin".into())
            );
            assert_eq!(
                resource.get(&Key::from_static_str("jackin.run_id")),
                Some("jk-run-0a1b2c".into())
            );
            assert_eq!(
                resource.get(&Key::from_static_str("parallax.run_id")),
                Some("jk-run-0a1b2c".into())
            );
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

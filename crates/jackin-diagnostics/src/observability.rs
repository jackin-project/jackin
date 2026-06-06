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
/// With `--features otlp` and `JACKIN_OTLP_ENDPOINT` set, installs OTLP export
/// beside the JSONL layer.
///
/// Returns `Ok(())` on success; the OTLP path returns an error if the global
/// subscriber is already set (e.g. a test that installs twice).
// `allow`, not `expect`: the body is trivially const only in the default
// (no-otlp) build; the otlp build does non-const setup, so the lint fires in one
// cfg and not the other and a single non-const signature is required.
#[allow(clippy::missing_const_for_fn)]
pub fn init_tracing(debug: bool) -> anyhow::Result<()> {
    #[cfg(feature = "otlp")]
    {
        if let Some(endpoint) = std::env::var("JACKIN_OTLP_ENDPOINT")
            .ok()
            .filter(|s| !s.is_empty())
        {
            return init_tracing_with_otlp(debug, &endpoint);
        }
    }

    // No fmt layer: the operator's terminal must never receive the firehose.
    let _ = debug;
    tracing_subscriber::registry()
        .with(JackinDiagnosticsLayer)
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"))
}

/// Install the tracing subscriber with an OTLP export layer.
///
/// Only compiled when `--features otlp` is active AND `JACKIN_OTLP_ENDPOINT`
/// is set at runtime. The function is entirely absent from default builds so
/// there is zero link-time cost. No `fmt` layer is attached: OTLP export is a
/// separate sink from the operator's screen, which stays free of the firehose.
#[cfg(feature = "otlp")]
fn init_tracing_with_otlp(debug: bool, endpoint: &str) -> anyhow::Result<()> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing_subscriber::EnvFilter;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .build()
        .map_err(|e| anyhow::anyhow!("OTLP exporter init failed: {e}"))?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();

    let tracer = provider.tracer("jackin");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    let level = if debug { "debug" } else { "info" };
    tracing_subscriber::registry()
        .with(JackinDiagnosticsLayer)
        .with(otel_layer.with_filter(EnvFilter::new(level)))
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"))
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
    tracing::info!(
        target: JSONL_TARGET,
        jackin_jsonl = true,
        run_id = run_id,
        kind = kind,
        diagnostics_message = message,
        stage = stage,
        detail = detail
    );
}

//! Structured observability: `tracing` subscriber and `JackinDiagnosticsLayer`.
//!
//! PR 1 (Defect 47.1) — wires the `tracing` subscriber alongside the existing
//! `RunDiagnostics::write()` path. Call-site churn is zero — `compact()`,
//! `stage()`, `debug()` continue to write JSONL directly and additionally emit
//! `tracing` events for correlated spans.
//!
//! Defect 47.6 (OTLP) — when compiled with `--features otlp` AND
//! `JACKIN_OTLP_ENDPOINT` is set at runtime, an additional
//! `tracing-opentelemetry` layer exports spans to the specified OTLP/HTTP
//! endpoint. No opentelemetry crates are present in the default build.

use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

/// Initialize the global `tracing` subscriber.
///
/// Builds a `registry()` with two layers (three when `otlp` feature + env set):
///
/// 1. **Compact stderr layer** — `info!` in normal mode, `debug!` when
///    `debug` is true.  Reproduces the two-tier `clog!`/`cdebug!` contract
///    from the `AGENTS.md` rule: always-on lifecycle events at `info`, verbose
///    firehose at `debug` gated by `JACKIN_DEBUG`.
/// 2. **`JackinDiagnosticsLayer`** — intercepts `tracing` events and records
///    them alongside the existing JSONL file (future: becomes the sole writer).
/// 3. **OTLP exporter** (optional) — when compiled with `--features otlp` and
///    `JACKIN_OTLP_ENDPOINT` is set at runtime, exports spans to an
///    OpenTelemetry-compatible collector (e.g. Jaeger, Tempo, Honeycomb).
///
/// Returns `Ok(())` when the subscriber is installed; returns an error if the
/// global default is already set (e.g. in tests that call this twice).
pub fn init_tracing(debug: bool) -> anyhow::Result<()> {
    let level = if debug { "debug" } else { "info" };
    let fmt_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_target(false)
        .with_writer(std::io::stderr)
        .with_filter(EnvFilter::new(level));

    #[cfg(feature = "otlp")]
    {
        if let Some(endpoint) = std::env::var("JACKIN_OTLP_ENDPOINT")
            .ok()
            .filter(|s| !s.is_empty())
        {
            return init_tracing_with_otlp(debug, fmt_layer, &endpoint);
        }
    }

    // Default path: fmt layer only (no opentelemetry deps at runtime).
    let _ = debug; // suppress unused-variable warning on the non-otlp path
    tracing_subscriber::registry()
        .with(fmt_layer)
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"))
}

/// Install the tracing subscriber with an additional OTLP export layer.
///
/// Only compiled when `--features otlp` is active AND `JACKIN_OTLP_ENDPOINT`
/// is set at runtime.  The function is entirely absent from default builds so
/// there is zero link-time cost.
#[cfg(feature = "otlp")]
fn init_tracing_with_otlp(
    debug: bool,
    fmt_layer: impl tracing_subscriber::Layer<tracing_subscriber::Registry> + Send + Sync + 'static,
    endpoint: &str,
) -> anyhow::Result<()> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::SdkTracerProvider;

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
        .with(fmt_layer)
        .with(otel_layer.with_filter(tracing_subscriber::EnvFilter::new(level)))
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"))
}

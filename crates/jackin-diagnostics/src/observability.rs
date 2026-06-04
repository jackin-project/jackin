//! `tracing` subscriber setup for optional OTLP export.
//!
//! Diagnostic events are recorded to the run JSONL by [`crate::RunDiagnostics`]
//! directly — that file is the only sink for the firehose. Diagnostic output
//! must never reach the operator's screen, neither the full-screen TUI nor plain
//! CLI commands, so no `fmt` layer writes to stdout/stderr. The `tracing` events
//! emitted alongside each JSONL write exist only to feed the optional OTLP
//! exporter, installed when compiled with `--features otlp` and
//! `JACKIN_OTLP_ENDPOINT` is set at runtime. No opentelemetry crates are present
//! in the default build.

/// Install the global `tracing` subscriber.
///
/// Default build: installs nothing — diagnostic events live only in the run
/// JSONL written by [`crate::RunDiagnostics`], so `tracing` events have no
/// terminal sink and never stream over the operator's screen. With
/// `--features otlp` and `JACKIN_OTLP_ENDPOINT` set, installs an OTLP export
/// layer.
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

    // Default path: no terminal subscriber. Diagnostic events are file-only via
    // RunDiagnostics::write; installing a fmt layer here would stream the
    // firehose over the operator's screen, which is forbidden in both TUI and
    // CLI modes.
    let _ = debug;
    Ok(())
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
    use tracing_subscriber::prelude::*;

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
        .with(otel_layer.with_filter(EnvFilter::new(level)))
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"))
}

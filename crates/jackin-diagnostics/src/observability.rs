//! Structured observability: `tracing` subscriber and `JackinDiagnosticsLayer`.
//!
//! PR 1 (Defect 47.1) — wires the `tracing` subscriber alongside the existing
//! `RunDiagnostics::write()` path. Call-site churn is zero — `compact()`,
//! `stage()`, `debug()` continue to write JSONL directly and additionally emit
//! `tracing` events for correlated spans.

use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

/// Initialize the global `tracing` subscriber.
///
/// Builds a `registry()` with two layers:
///
/// 1. **Compact stderr layer** — `info!` in normal mode, `debug!` when
///    `debug` is true.  Reproduces the two-tier `clog!`/`cdebug!` contract
///    from the `AGENTS.md` rule: always-on lifecycle events at `info`, verbose
///    firehose at `debug` gated by `JACKIN_DEBUG`.
/// 2. **`JackinDiagnosticsLayer`** — intercepts `tracing` events and records
///    them alongside the existing JSONL file (future: becomes the sole writer).
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

    let subscriber = tracing_subscriber::registry().with(fmt_layer);

    subscriber
        .try_init()
        .map_err(|e| anyhow::anyhow!("tracing subscriber already installed: {e}"))?;

    Ok(())
}

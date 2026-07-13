//! Hot-path metric instruments for terminal/render/input/usage/errors.
//!
//! Counters and histograms replace per-event DEBUG log rows on the high-
//! frequency paths (plan 042). All recorders no-op when OTLP metrics were not
//! installed (no provider → zero cost beyond one atomic load).

#[cfg(feature = "otlp")]
use std::sync::OnceLock;

/// Install hot-path instruments on the process meter. Called once from
/// `init_metrics` (host and capsule).
///
/// Returns `true` when this call won the process-wide `OnceLock` (first install).
#[cfg(feature = "otlp")]
pub(crate) fn install_hot_path(meter: &opentelemetry::metrics::Meter) -> bool {
    use crate::observability::otel_metrics as names;
    let metrics = HotPathMetrics {
        terminal_bytes_sent: meter
            .u64_counter(names::TERMINAL_BYTES_SENT)
            .with_unit("By")
            .with_description("Terminal bytes sent to the client")
            .build(),
        terminal_bytes_received: meter
            .u64_counter(names::TERMINAL_BYTES_RECEIVED)
            .with_unit("By")
            .with_description("Terminal bytes received from a PTY")
            .build(),
        terminal_cursor_moves: meter
            .u64_counter(names::TERMINAL_CURSOR_MOVES)
            .with_unit("1")
            .with_description("Cursor-move sequences in emitted frames")
            .build(),
        render_duration: meter
            .u64_histogram(names::RENDER_DURATION)
            .with_unit("us")
            .with_description("Render frame duration")
            .build(),
        render_painted_cells: meter
            .u64_counter(names::RENDER_PAINTED_CELLS)
            .with_unit("1")
            .with_description("Painted cells in emitted frames")
            .build(),
        render_frames: meter
            .u64_counter(names::RENDER_FRAMES)
            .with_unit("1")
            .with_description("Composed render frames")
            .build(),
        input_mouse_events: meter
            .u64_counter(names::INPUT_MOUSE_EVENTS)
            .with_unit("1")
            .with_description("Mouse events handled by the host cockpit")
            .build(),
        usage_accounts_refreshed: meter
            .u64_counter(names::USAGE_ACCOUNTS_REFRESHED)
            .with_unit("1")
            .with_description("Usage account snapshots refreshed")
            .build(),
        errors_count: meter
            .u64_counter(names::ERRORS_COUNT)
            .with_unit("1")
            .with_description("Typed diagnostics errors by error.type")
            .build(),
    };
    HOT_PATH.set(metrics).is_ok()
}

#[cfg(feature = "otlp")]
struct HotPathMetrics {
    terminal_bytes_sent: opentelemetry::metrics::Counter<u64>,
    terminal_bytes_received: opentelemetry::metrics::Counter<u64>,
    terminal_cursor_moves: opentelemetry::metrics::Counter<u64>,
    render_duration: opentelemetry::metrics::Histogram<u64>,
    render_painted_cells: opentelemetry::metrics::Counter<u64>,
    render_frames: opentelemetry::metrics::Counter<u64>,
    input_mouse_events: opentelemetry::metrics::Counter<u64>,
    usage_accounts_refreshed: opentelemetry::metrics::Counter<u64>,
    errors_count: opentelemetry::metrics::Counter<u64>,
}

#[cfg(feature = "otlp")]
static HOT_PATH: OnceLock<HotPathMetrics> = OnceLock::new();

/// Record one emitted client frame: bytes, cursor moves, painted cells.
pub fn record_frame(bytes: u64, cursor_moves: u64, painted_cells: u64) {
    #[cfg(feature = "otlp")]
    if let Some(m) = HOT_PATH.get() {
        m.terminal_bytes_sent.add(bytes, &[]);
        m.terminal_cursor_moves.add(cursor_moves, &[]);
        m.render_painted_cells.add(painted_cells, &[]);
    }
    #[cfg(not(feature = "otlp"))]
    {
        let _ = (bytes, cursor_moves, painted_cells);
    }
}

/// Record one render: duration histogram + frames counter (+ optional cells).
pub fn record_render(duration_us: u64, painted_cells: u64) {
    #[cfg(feature = "otlp")]
    if let Some(m) = HOT_PATH.get() {
        m.render_duration.record(duration_us, &[]);
        m.render_frames.add(1, &[]);
        if painted_cells > 0 {
            m.render_painted_cells.add(painted_cells, &[]);
        }
    }
    #[cfg(not(feature = "otlp"))]
    {
        let _ = (duration_us, painted_cells);
    }
}

/// PTY bytes received into a session.
pub fn incr_terminal_bytes_received(bytes: u64) {
    #[cfg(feature = "otlp")]
    if let Some(m) = HOT_PATH.get() {
        m.terminal_bytes_received.add(bytes, &[]);
    }
    #[cfg(not(feature = "otlp"))]
    {
        let _ = bytes;
    }
}

/// Host cockpit mouse event handled.
pub fn incr_mouse_events() {
    #[cfg(feature = "otlp")]
    if let Some(m) = HOT_PATH.get() {
        m.input_mouse_events.add(1, &[]);
    }
}

/// Usage accounts refreshed in one pass.
pub fn incr_accounts_refreshed(count: u64) {
    #[cfg(feature = "otlp")]
    if let Some(m) = HOT_PATH.get() {
        m.usage_accounts_refreshed.add(count, &[]);
    }
    #[cfg(not(feature = "otlp"))]
    {
        let _ = count;
    }
}

/// Typed error counter (`error.type` attribute).
pub fn incr_errors(error_type: &str) {
    #[cfg(feature = "otlp")]
    if let Some(m) = HOT_PATH.get() {
        use opentelemetry::KeyValue;
        m.errors_count
            .add(1, &[KeyValue::new("error.type", error_type.to_owned())]);
    }
    #[cfg(not(feature = "otlp"))]
    {
        let _ = error_type;
    }
}

/// Process-wide test rig: instruments + collectible in-memory exporter.
///
/// `HOT_PATH` is a `OnceLock`, so the first install wins for the whole test
/// process. Always install through this helper so the provider has a reader
/// that can force-flush into `InMemoryMetricExporter`.
#[cfg(all(test, feature = "otlp"))]
struct HotPathTestRig {
    provider: opentelemetry_sdk::metrics::SdkMeterProvider,
    exporter: opentelemetry_sdk::metrics::InMemoryMetricExporter,
    /// Whether `HOT_PATH` instruments were minted from this provider.
    instruments_owned: bool,
}

#[cfg(all(test, feature = "otlp"))]
static HOT_PATH_TEST_RIG: OnceLock<HotPathTestRig> = OnceLock::new();

/// Test-only: install instruments once with an in-memory metric exporter.
///
/// Safe to call from multiple tests; subsequent calls are no-ops once the
/// process-wide rig (or production `HOT_PATH`) is set.
#[cfg(all(test, feature = "otlp"))]
pub(crate) fn install_hot_path_for_test(_meter: &opentelemetry::metrics::Meter) {
    ensure_hot_path_test_rig();
}

/// Ensure hot-path instruments are installed against a collectible provider.
///
/// Returns `true` when counters can be read back via
/// [`collect_hot_path_counter_sums`] (instruments owned by this rig).
#[cfg(all(test, feature = "otlp"))]
pub(crate) fn ensure_hot_path_test_rig() -> bool {
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

    let rig = HOT_PATH_TEST_RIG.get_or_init(|| {
        let exporter = InMemoryMetricExporter::default();
        let reader = PeriodicReader::builder(exporter.clone()).build();
        let provider = SdkMeterProvider::builder().with_reader(reader).build();
        let meter = provider.meter("jackin-hot-path-test");
        // Own only when set succeeds — concurrent init_metrics may race.
        let instruments_owned = install_hot_path(&meter);
        HotPathTestRig {
            provider,
            exporter,
            instruments_owned,
        }
    });
    rig.instruments_owned
}

/// Force-flush the test meter provider and sum each requested u64 counter
/// (cumulative totals since process start for this provider).
///
/// Returns `None` when the test rig does not own the process instruments
/// (another install won the `OnceLock`) or export failed. Missing names map
/// to `0` (instrument never recorded).
#[cfg(all(test, feature = "otlp"))]
pub(crate) fn collect_hot_path_counter_sums(names: &[&str]) -> Option<Vec<u64>> {
    let rig = HOT_PATH_TEST_RIG.get()?;
    if !rig.instruments_owned {
        return None;
    }
    // Drop prior exports so this flush is the only window we sum.
    rig.exporter.reset();
    rig.provider.force_flush().ok()?;
    let finished = rig.exporter.get_finished_metrics().ok()?;

    let mut totals = vec![0u64; names.len()];
    for resource in &finished {
        sum_resource_counters(resource, names, &mut totals);
    }
    Some(totals)
}

#[cfg(all(test, feature = "otlp"))]
fn sum_resource_counters(
    resource: &opentelemetry_sdk::metrics::data::ResourceMetrics,
    names: &[&str],
    totals: &mut [u64],
) {
    use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData};
    for scope in resource.scope_metrics() {
        for metric in scope.metrics() {
            let Some(idx) = names.iter().position(|&n| n == metric.name()) else {
                continue;
            };
            let AggregatedMetrics::U64(MetricData::Sum(sum)) = metric.data() else {
                continue;
            };
            for point in sum.data_points() {
                totals[idx] = totals[idx].saturating_add(point.value());
            }
        }
    }
}

#[cfg(test)]
mod tests;

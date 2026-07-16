//! Governed hot-path metric wrappers.
//!
//! Instrument definitions, privacy validation, and cardinality limits live in
//! `jackin-telemetry`; this composition-root crate only exposes product-facing
//! recording helpers.

use jackin_telemetry::{Attr, Value, counter, histogram, metric};

/// Record one emitted client frame: bytes, cursor moves, and painted cells.
pub fn record_frame(bytes: u64, cursor_moves: u64, painted_cells: u64) {
    let output = [Attr {
        key: jackin_telemetry::schema::attrs::STREAM_DIRECTION,
        value: Value::Str("output"),
    }];
    let _bytes_result = counter(&metric::TERMINAL_BYTES).add(bytes, &output);
    let _cursor_result = counter(&metric::TERMINAL_CURSOR_MOVES).add(cursor_moves, &[]);
    let _cells_result = counter(&metric::TERMINAL_RENDER_CELLS).add(painted_cells, &[]);
    let _frames_result = counter(&metric::TERMINAL_RENDER_FRAMES).add(1, &[]);
}

/// Record one render using the registry's seconds unit.
pub fn record_render(duration_us: u64, painted_cells: u64) {
    let _duration_result =
        histogram(&metric::TERMINAL_RENDER_DURATION).record(duration_us as f64 / 1_000_000.0, &[]);
    if painted_cells > 0 {
        let _cells_result = counter(&metric::TERMINAL_RENDER_CELLS).add(painted_cells, &[]);
    }
}

/// Record bytes read from a PTY.
pub fn incr_terminal_bytes_received(bytes: u64) {
    let input = [Attr {
        key: jackin_telemetry::schema::attrs::STREAM_DIRECTION,
        value: Value::Str("input"),
    }];
    let _bytes_result = counter(&metric::TERMINAL_BYTES).add(bytes, &input);
}

/// Record one semantic mouse input without coordinates or payload.
pub fn incr_mouse_events() {
    let _mouse_result = counter(&metric::TERMINAL_INPUT_MOUSE).add(1, &[]);
}

/// Docker inspection is represented by its governed connection/HTTP boundary.
pub fn incr_docker_inspect() {}

/// Database activity is represented by governed database operation spans.
pub fn incr_db_statement(_statement_name: &'static str) {}

/// Account reconciliation is represented by its governed background cycle.
pub fn incr_accounts_refreshed(_count: u64) {}

/// Errors are represented by the owning operation and typed error event.
pub fn incr_errors(_error_type: &str) {}

#[cfg(test)]
struct TestRig {
    provider: opentelemetry_sdk::metrics::SdkMeterProvider,
    exporter: opentelemetry_sdk::metrics::InMemoryMetricExporter,
}

#[cfg(test)]
static TEST_RIG: std::sync::OnceLock<TestRig> = std::sync::OnceLock::new();

#[cfg(test)]
pub(crate) fn ensure_hot_path_test_rig() -> bool {
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

    TEST_RIG.get_or_init(|| {
        let exporter = InMemoryMetricExporter::default();
        let reader = PeriodicReader::builder(exporter.clone()).build();
        let provider = SdkMeterProvider::builder().with_reader(reader).build();
        jackin_telemetry::install(&provider.meter("jackin-telemetry-test"));
        TestRig { provider, exporter }
    });
    true
}

#[cfg(test)]
pub(crate) fn collect_hot_path_metric_count() -> Option<usize> {
    ensure_hot_path_test_rig();
    let rig = TEST_RIG.get()?;
    rig.exporter.reset();
    rig.provider.force_flush().ok()?;
    let metrics = rig.exporter.get_finished_metrics().ok()?;
    Some(
        metrics
            .iter()
            .flat_map(opentelemetry_sdk::metrics::data::ResourceMetrics::scope_metrics)
            .map(|scope| scope.metrics().count())
            .sum(),
    )
}

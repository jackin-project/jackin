//! Hot-path metric instruments for terminal/render/input/usage/errors.
//!
//! Counters and histograms replace per-event DEBUG log rows on the high-
//! frequency paths (plan 042). All recorders no-op when OTLP metrics were not
//! installed (no provider → zero cost beyond one atomic load).

#[cfg(feature = "otlp")]
use std::sync::OnceLock;

/// Install hot-path instruments on the process meter. Called once from
/// `init_metrics` (host and capsule).
#[cfg(feature = "otlp")]
pub(crate) fn install_hot_path(meter: &opentelemetry::metrics::Meter) {
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
    drop(HOT_PATH.set(metrics));
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

/// Test-only: install instruments on an arbitrary meter (in-memory exporter).
#[cfg(all(test, feature = "otlp"))]
pub(crate) fn install_hot_path_for_test(meter: &opentelemetry::metrics::Meter) {
    // Reset is not supported; tests that need a fresh OnceLock must run first
    // or accept a no-op if already set. Prefer calling this once per process.
    if HOT_PATH.get().is_none() {
        install_hot_path(meter);
    }
}

#[cfg(all(test, feature = "otlp"))]
mod tests;

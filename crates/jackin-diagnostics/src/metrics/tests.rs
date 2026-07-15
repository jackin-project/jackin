//! Hot-path metric instrument recording tests.

use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::metrics::SdkMeterProvider;

use super::{
    collect_hot_path_counter_sums, ensure_hot_path_test_rig, incr_errors, incr_mouse_events,
    install_hot_path_for_test, record_frame, record_render,
};
use crate::observability::otel_metrics as names;

const HOT_PATH_COUNTERS: &[&str] = &[
    names::TERMINAL_BYTES_SENT,
    names::RENDER_FRAMES,
    names::RENDER_PAINTED_CELLS,
    names::TERMINAL_CURSOR_MOVES,
];

#[test]
fn hot_path_record_paths_do_not_panic() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let provider = SdkMeterProvider::builder().build();
    let meter = provider.meter("jackin-test");
    install_hot_path_for_test(&meter);

    record_frame(100, 3, 50);
    record_render(250, 10);
    incr_mouse_events();
    incr_errors("process_spawn_error");
    drop(provider);
}

#[test]
fn simulated_frames_emit_no_send_render_debug_rows() {
    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    assert!(
        ensure_hot_path_test_rig(),
        "hot-path test rig must own instruments so OTel counters can be collected"
    );

    // Snapshot cumulative counters, then drive 100 frame emissions through the
    // converted path (record_frame + record_render). Counters must advance by
    // the exact recorded totals; DEBUG send:/render: rows are demoted to
    // `ctrace_payload` in the capsule (asserted separately via source contract).
    let before = collect_hot_path_counter_sums(HOT_PATH_COUNTERS)
        .expect("force_flush into InMemoryMetricExporter must succeed");

    const N: u64 = 100;
    const BYTES_PER: u64 = 64;
    const CURSOR_PER: u64 = 1;
    const CELLS_PER_FRAME: u64 = 8;
    const CELLS_PER_RENDER: u64 = 8;
    const DURATION_US: u64 = 100;

    for _ in 0..N {
        record_frame(BYTES_PER, CURSOR_PER, CELLS_PER_FRAME);
        record_render(DURATION_US, CELLS_PER_RENDER);
    }

    let after = collect_hot_path_counter_sums(HOT_PATH_COUNTERS)
        .expect("force_flush into InMemoryMetricExporter must succeed after recording");

    let delta = |i: usize| after[i].saturating_sub(before[i]);

    assert_eq!(delta(0), N * BYTES_PER, "jackin.terminal.bytes_sent delta");
    assert_eq!(
        delta(1),
        N,
        "jackin.render.frames delta (one per record_render)"
    );
    // record_frame adds CELLS_PER_FRAME; record_render adds CELLS_PER_RENDER when > 0.
    assert_eq!(
        delta(2),
        N * (CELLS_PER_FRAME + CELLS_PER_RENDER),
        "jackin.render.painted_cells delta"
    );
    assert_eq!(
        delta(3),
        N * CURSOR_PER,
        "jackin.terminal.cursor_moves delta"
    );
}

/// Capsule hot paths must not reintroduce per-frame DEBUG firehose rows.
/// Source contract: demotion targets are `ctrace_payload!`, never `cdebug!("send:`
/// / `cdebug!("render:`.
#[test]
fn capsule_hot_paths_have_no_send_render_cdebug() {
    let client_writer = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../jackin-capsule/src/client_writer.rs"
    ));
    let compositor = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../jackin-capsule/src/daemon/compositor.rs"
    ));

    assert!(
        !client_writer.contains("cdebug!(\"send:"),
        "client_writer must not emit per-frame cdebug!(\"send: …) rows"
    );
    assert!(
        !compositor.contains("cdebug!(\"render:"),
        "compositor must not emit per-frame cdebug!(\"render: …) rows"
    );
    assert!(
        client_writer.contains("ctrace_payload!") && client_writer.contains("send:"),
        "client_writer should keep send: detail at ctrace_payload tier"
    );
    assert!(
        compositor.contains("ctrace_payload!") && compositor.contains("render:"),
        "compositor should keep render: detail at ctrace_payload tier"
    );
    assert!(
        client_writer.contains("record_frame")
            || client_writer.contains("jackin_diagnostics::record_frame"),
        "client_writer must record frame metrics"
    );
    assert!(
        compositor.contains("record_render")
            || compositor.contains("jackin_diagnostics::record_render"),
        "compositor must record render metrics"
    );
}

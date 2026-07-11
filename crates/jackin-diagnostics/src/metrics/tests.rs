//! Hot-path metric instrument recording tests.

use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::metrics::SdkMeterProvider;

use super::{
    incr_errors, incr_mouse_events, install_hot_path_for_test, record_frame, record_render,
};

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

    // 100 frame emissions through the converted path — counters advance with
    // no DEBUG log rows (send:/render: demoted to ctrace_payload in capsule).
    for _ in 0..100 {
        record_frame(64, 1, 8);
        record_render(100, 8);
    }
}

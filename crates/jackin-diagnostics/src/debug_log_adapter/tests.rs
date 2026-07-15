use jackin_core::DebugLogSink;

use super::DiagnosticsDebugLog;
use crate::{
    DIAGNOSTICS_TEST_LOCK,
    logging::{begin_debug_buffering, drain_debug_buffer_for_test},
};

#[test]
fn diagnostics_debug_log_sink_emits_exact_debug_line_format() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    drop(drain_debug_buffer_for_test());
    begin_debug_buffering();

    DiagnosticsDebugLog.log("launch", "container ready");

    assert_eq!(
        drain_debug_buffer_for_test(),
        vec!["[jackin debug launch] container ready"]
    );
}

use jackin_core::OperatorNoticeSink;

use super::DiagnosticsOperatorNotice;
use crate::{
    DIAGNOSTICS_TEST_LOCK,
    logging::{begin_debug_buffering, drain_debug_buffer_for_test},
    terminal::set_rich_surface_active,
};

#[test]
fn diagnostics_operator_notice_sink_buffers_exact_operator_line_under_rich_surface() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    drop(drain_debug_buffer_for_test());
    set_rich_surface_active(false);
    begin_debug_buffering();
    set_rich_surface_active(true);

    DiagnosticsOperatorNotice.notice("warning", "jackin: warning: OTLP export failed");

    set_rich_surface_active(false);
    assert_eq!(
        drain_debug_buffer_for_test(),
        vec!["jackin: warning: OTLP export failed"]
    );
}

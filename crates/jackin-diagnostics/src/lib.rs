//! jackin-diagnostics: compact/debug telemetry and observability plumbing.
//!
//! **Architecture Invariant:** T2.
//! Entry point: [`telemetry_debug!`] — compact always-on telemetry macro.

pub mod build_log;
pub mod logging;
pub mod metrics;
pub mod observability;
#[cfg(feature = "test-support")]
mod observability_test_support;
pub mod operation;
pub mod operator_notice;
pub mod redact;
pub mod run;
pub mod screen;
pub mod secret_scrub;
mod stage;
pub mod terminal;

pub use logging::{
    TelemetryLevel, TelemetrySink, begin_debug_buffering, drain_debug_buffer_for_test,
    emit_compact_line, emit_debug_line, emit_operator_notice, end_debug_buffering,
    format_debug_line, is_debug_mode, set_config_telemetry, set_debug_mode, sink_level,
    telemetry_level, telemetry_level_name,
};
pub use metrics::{
    incr_accounts_refreshed, incr_db_statement, incr_docker_inspect, incr_errors,
    incr_mouse_events, incr_terminal_bytes_received, record_frame, record_render,
};
pub use observability::{
    ContainerOtlp, ServiceIdentity, TelemetryHealth, TelemetrySignalHealth, ValidationFailure,
    ValidationReport, backend_query_hint, configured_endpoint, configured_endpoint_summary,
    container_otlp, init_capsule_tracing, init_tracing, init_tracing_for,
    record_telemetry_rejection, shutdown_capsule_tracing, telemetry_health_snapshot,
    unsupported_otlp_protocol, validate_delivery,
};
#[cfg(feature = "test-support")]
pub use observability::{flush_wire_test_export, init_wire_test_export};
pub use operation::{
    OperationGuard, OperationLevel, enter_operation, operation_error, operation_log,
    operation_metric, operation_record_exit_code, operation_set_i64_attr, operation_span,
};
pub use run::{
    ActiveRunGuard, RunDiagnostics, active_debug, active_run, active_run_for_paths,
    active_subprocess_done, active_timing_done, active_timing_started, install_host_panic_hook,
    mint_session_id,
};
pub use screen::current_screen_name;
pub use secret_scrub::scrub_secrets;
pub use stage::DiagnosticStage;
pub use terminal::{
    host_screen_owned, reassert_alt_screen, rich_surface_active, rich_terminal_owned,
    set_host_screen_owned, set_rich_surface_active, set_terminal_title, shorten_home,
};

#[cfg(test)]
pub(crate) static DIAGNOSTICS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests;

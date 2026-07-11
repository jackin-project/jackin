//! jackin-diagnostics: compact/debug telemetry and observability plumbing.
//!
//! **Architecture Invariant:** T2.
//! Entry point: [`debug_log!`] — compact always-on telemetry macro.

pub mod build_log;
pub mod debug_log;
pub mod logging;
pub mod observability;
pub mod operator_notice;
pub mod redact;
pub mod run;
pub mod screen;
pub mod secret_scrub;
pub mod summary;
pub mod terminal;

pub use logging::{
    TelemetryLevel, begin_debug_buffering, drain_debug_buffer_for_test, emit_compact_line,
    emit_debug_line, emit_operator_notice, end_debug_buffering, format_debug_line, is_debug_mode,
    set_config_telemetry, set_debug_mode, telemetry_level,
};
pub use observability::{
    ContainerOtlp, backend_query_hint, configured_endpoint, configured_endpoint_summary,
    container_otlp, init_capsule_tracing, init_tracing, otel_events, otel_keys, otel_metrics,
    shutdown_capsule_tracing, unsupported_otlp_protocol,
};
pub use run::{
    ActiveRunGuard, RunDiagnostics, active_debug, active_run, active_run_for_paths,
    active_subprocess_done, active_timing_done, active_timing_started, install_host_panic_hook,
    mint_session_id, prune_all_runs, prune_old_runs,
};
pub use screen::{
    Screen, ScreenGuard, carry_link_forward, current_traceparent, enter_screen, launch_trace,
    record_action, record_capsule_activity, set_agent_selected, set_agents_active, set_provider,
    set_workspace, set_workspace_kind,
};
pub use secret_scrub::scrub_secrets;
pub use summary::{
    BuildContextSnapshotSummary, CacheEventSummary, DiagnosticsSummary, DockerBuildStepSummary,
    ImageBuildSourceSummary, LaunchPlanEventSummary, PrewarmedDindAdoptionSummary,
    SkippedTimingSummary, summarize_reader, summarize_run_file,
};
pub use terminal::{
    host_screen_owned, reassert_alt_screen, rich_surface_active, rich_terminal_owned,
    set_host_screen_owned, set_rich_surface_active, set_terminal_title, shorten_home,
};

#[cfg(test)]
pub(crate) static DIAGNOSTICS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Verbose-trace helper for `--debug` runs. No-op when the flag is off.
///
/// `category` is a short tag (`isolation`, `worktree`, etc.) that keeps shared
/// logs greppable. Use `format!`-style trailing args:
///
/// ```ignore
/// debug_log!("isolation", "git worktree add -b {branch} {path}");
/// ```
#[macro_export]
macro_rules! debug_log {
    ($category:expr, $($arg:tt)*) => {
        if $crate::is_debug_mode() {
            $crate::emit_debug_line($category, &::std::format!($($arg)*));
        }
    };
}

#[cfg(test)]
mod tests;

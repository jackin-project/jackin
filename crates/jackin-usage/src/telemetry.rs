//! OTLP export for the in-container capsule session.
//!
//! Runtime-gated on the OTLP endpoint env the host injects
//! (`OTEL_EXPORTER_OTLP_ENDPOINT`); a no-op when unset. When active, the
//! session's telemetry carries a `session.id` (grouping the whole session into
//! one timeline), the host `parallax.run.id` (joining it to the host run), and a
//! link back to the launch trace via the propagated `TRACEPARENT`. The
//! capsule's `clog!`/`cdebug!` lines are bridged to OTLP logs so the existing
//! two-tier breadcrumbs appear in the backend, correlated by `session.id`.

use std::sync::atomic::{AtomicBool, Ordering};

static OTLP_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Initialise capsule OTLP export. Reads the session/run identity and launch
/// traceparent from the env the host injected. Call once at daemon start; hold
/// the returned guard for the daemon's lifetime so the session tail flushes on
/// every graceful exit path.
pub fn init() -> FlushGuard {
    let session_id = jackin_diagnostics::mint_session_id();
    let run_id = std::env::var("JACKIN_RUN_ID").ok();
    let traceparent = std::env::var("TRACEPARENT").ok();
    match jackin_diagnostics::init_capsule_tracing(
        &session_id,
        run_id.as_deref(),
        traceparent.as_deref(),
    ) {
        Ok(true) => {
            OTLP_ACTIVE.store(true, Ordering::Relaxed);
            crate::clog!("otlp export active: session_id={session_id}");
        }
        Ok(false) => {}
        Err(error) => crate::clog!("otlp export disabled: {error}"),
    }
    FlushGuard
}

/// Flushes the OTLP exporters on drop, so the session tail is not lost on a
/// graceful daemon exit. A SIGKILL still skips it, which is why per-activity
/// telemetry exports as it happens rather than waiting on a session span.
#[derive(Debug)]
#[must_use = "hold the guard for the daemon's lifetime"]
pub struct FlushGuard;

impl Drop for FlushGuard {
    fn drop(&mut self) {
        shutdown();
    }
}

/// Whether OTLP export was activated.
#[must_use]
pub fn otlp_active() -> bool {
    OTLP_ACTIVE.load(Ordering::Relaxed)
}

#[cfg(test)]
pub(crate) fn set_otlp_active_for_test(active: bool) {
    OTLP_ACTIVE.store(active, Ordering::Relaxed);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Bridge a capsule log line into OTLP logs at the supplied severity. No-op
/// unless export is active.
pub fn bridge_log(level: BridgeLevel, message: &str) {
    if !otlp_active() {
        return;
    }
    match level {
        BridgeLevel::Debug => tracing::debug!(target: "jackin_capsule", "{message}"),
        BridgeLevel::Info => tracing::info!(target: "jackin_capsule", "{message}"),
        BridgeLevel::Warn => tracing::warn!(target: "jackin_capsule", "{message}"),
        BridgeLevel::Error => tracing::error!(target: "jackin_capsule", "{message}"),
    }
}

/// Flush and shut down the OTLP exporters before the daemon exits, so the tail
/// of the session is not lost.
pub fn shutdown() {
    if otlp_active() {
        jackin_diagnostics::shutdown_capsule_tracing();
    }
}

#[cfg(test)]
mod tests;

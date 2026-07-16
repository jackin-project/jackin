// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! OTLP export for the in-container capsule session.
//!
//! Runtime-gated on the OTLP endpoint env the host injects
//! (`OTEL_EXPORTER_OTLP_ENDPOINT`); a no-op when unset. When active, the
//! session's telemetry carries a `session.id` (grouping the whole session into
//! one timeline) and a link back to the launch trace via W3C propagation. All
//! emitted signals pass through the shared governed facade.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static OTLP_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Session and trace context captured at daemon start.
static SESSION_CONTEXT: OnceLock<SessionContext> = OnceLock::new();

#[derive(Debug, Clone)]
struct SessionContext {
    session_id: String,
    traceparent: Option<String>,
}

/// Capsule session correlation context captured by [`init`] for local operator
/// output, such as the compact startup banner.
#[must_use]
pub fn session_context() -> Option<(String, Option<String>)> {
    SESSION_CONTEXT
        .get()
        .map(|ctx| (ctx.session_id.clone(), ctx.traceparent.clone()))
}

/// Initialise capsule OTLP export. Reads the session/invocation identity and launch
/// traceparent from the env the host injected. Call once at daemon start; hold
/// the returned guard for the daemon's lifetime so the session tail flushes on
/// every graceful exit path.
pub fn init() -> FlushGuard {
    if let Ok(value) = std::env::var("JACKIN_INVOCATION_ID")
        && let Ok(id) = jackin_telemetry::identity::InvocationId::parse(&value)
    {
        let _invocation_result = jackin_telemetry::identity::set_current_invocation(id);
    }
    let session = jackin_telemetry::identity::begin_session();
    let session_id = session.current.to_string();
    let traceparent = std::env::var("TRACEPARENT").ok();
    drop(SESSION_CONTEXT.set(SessionContext {
        session_id: session_id.clone(),
        traceparent: traceparent.clone(),
    }));
    match jackin_diagnostics::init_capsule_tracing(traceparent.as_deref()) {
        Ok(true) => {
            OTLP_ACTIVE.store(true, Ordering::Relaxed);
            let attrs = [jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::SESSION_ID,
                value: jackin_telemetry::Value::Str(&session_id),
            }];
            let _event_result = jackin_telemetry::emit_event(
                &jackin_telemetry::event::SESSION_START,
                jackin_telemetry::FieldSet::new(&attrs, None),
            );
            jackin_diagnostics::telemetry_info!(
                "capsule",
                "otlp export active: session_id={session_id}"
            );
        }
        Ok(false) => {}
        Err(error) => {
            jackin_diagnostics::telemetry_info!("capsule", "otlp export disabled: {error}");
        }
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

/// Flush and shut down the OTLP exporters before the daemon exits, so the tail
/// of the session is not lost.
pub fn shutdown() {
    if otlp_active() {
        if let Some(session) = jackin_telemetry::identity::current_session() {
            let session_id = session.current.to_string();
            let attrs = [jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::SESSION_ID,
                value: jackin_telemetry::Value::Str(&session_id),
            }];
            let _event_result = jackin_telemetry::emit_event(
                &jackin_telemetry::event::SESSION_END,
                jackin_telemetry::FieldSet::new(&attrs, None),
            );
            jackin_telemetry::identity::end_session(session.current);
        }
        jackin_diagnostics::shutdown_capsule_tracing();
        OTLP_ACTIVE.store(false, Ordering::Relaxed);
    }
}

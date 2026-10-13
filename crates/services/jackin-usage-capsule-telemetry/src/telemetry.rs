// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! OTLP export and bounded lifecycle telemetry for the in-container Capsule.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use jackin_telemetry::ResultTelemetryExt as _;
use jackin_telemetry::propagation::{Carrier, ExtractOutcome};

static OTLP_ACTIVE: AtomicBool = AtomicBool::new(false);
static SESSION_CONTEXT: OnceLock<SessionContext> = OnceLock::new();

#[derive(Debug, Clone)]
struct SessionContext {
    session_id: String,
    traceparent: Option<String>,
}

#[must_use]
pub fn session_context() -> Option<(String, Option<String>)> {
    SESSION_CONTEXT
        .get()
        .map(|ctx| (ctx.session_id.clone(), ctx.traceparent.clone()))
}

/// Claim Capsule identity before any fallible daemon startup, install export,
/// and start a bounded startup operation. The caller marks listener readiness
/// through [`FlushGuard::listener_ready`].
pub fn init() -> Result<FlushGuard, jackin_telemetry::identity::SessionOwnershipError> {
    let invocation_id = std::env::var("JACKIN_INVOCATION_ID").ok();
    if let Some(value) = invocation_id.as_deref()
        && let Ok(id) = jackin_telemetry::identity::InvocationId::parse(value)
    {
        let _invocation_result = jackin_telemetry::identity::set_current_invocation(id);
    }
    let mut session = jackin_telemetry::identity::SessionGuard::claim(
        jackin_telemetry::identity::SessionKind::Capsule,
    )?;
    let session_id = session.context().current.to_string();
    let traceparent = std::env::var("TRACEPARENT").ok();
    let tracestate = std::env::var("TRACESTATE").ok();
    drop(SESSION_CONTEXT.set(SessionContext {
        session_id: session_id.clone(),
        traceparent: traceparent.clone(),
    }));

    let active = match jackin_diagnostics::init_capsule_tracing(traceparent.as_deref())
        .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::ConfigError)
    {
        Ok(true) => {
            OTLP_ACTIVE.store(true, Ordering::Relaxed);
            true
        }
        Ok(false) => false,
        Err(_) => false,
    };
    session.start();
    let startup =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::APP_STARTUP, &[]).ok();
    if let (Some(startup), ExtractOutcome::Parent(parent)) = (
        startup.as_ref(),
        jackin_telemetry::propagation::extract(&LaunchCarrier {
            traceparent: traceparent.as_deref(),
            tracestate: tracestate.as_deref(),
            invocation_id: invocation_id.as_deref(),
        }),
    ) {
        let _link_result = startup.link(&parent);
    }
    Ok(FlushGuard {
        session: Some(session),
        startup,
        active,
    })
}

struct LaunchCarrier<'a> {
    traceparent: Option<&'a str>,
    tracestate: Option<&'a str>,
    invocation_id: Option<&'a str>,
}

impl Carrier for LaunchCarrier<'_> {
    fn version(&self) -> u16 {
        jackin_telemetry::propagation::VERSION
    }
    fn traceparent(&self) -> Option<&str> {
        self.traceparent
    }
    fn tracestate(&self) -> Option<&str> {
        self.tracestate
    }
    fn invocation_id(&self) -> Option<&str> {
        self.invocation_id
    }
    fn session_id(&self) -> Option<&str> {
        None
    }
    fn job_id(&self) -> Option<&str> {
        None
    }
    fn set_trace(&mut self, _traceparent: String, _tracestate: Option<String>) {}
    fn set_product_ids(
        &mut self,
        _invocation_id: Option<String>,
        _session_id: Option<String>,
        _job_id: Option<String>,
    ) {
    }
}

/// Owns paired Capsule session/startup lifecycle and exporter shutdown.
#[derive(Debug)]
#[must_use = "hold the guard for the daemon's lifetime"]
pub struct FlushGuard {
    session: Option<jackin_telemetry::identity::SessionGuard>,
    startup: Option<jackin_telemetry::OperationGuard>,
    active: bool,
}

impl FlushGuard {
    /// Complete bounded startup only once the Capsule listener is ready.
    pub fn listener_ready(&mut self) {
        if let Some(startup) = self.startup.take() {
            startup.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
        }
    }

    /// Record a terminal daemon failure only when startup already completed.
    ///
    /// Before listener readiness, the startup operation owns the failure in
    /// `Drop`; afterwards that bounded operation is gone, so the process owner
    /// emits one body-free typed error instead.
    pub fn daemon_failed(&self) {
        if self.daemon_failure_needs_terminal_event() {
            let _error = jackin_telemetry::record_error(
                jackin_telemetry::schema::enums::ErrorType::LaunchFailed,
            );
        }
    }

    fn daemon_failure_needs_terminal_event(&self) -> bool {
        self.startup.is_none()
    }
}

impl Drop for FlushGuard {
    fn drop(&mut self) {
        if let Some(startup) = self.startup.take() {
            startup.complete(
                jackin_telemetry::schema::enums::OutcomeValue::Error,
                Some(jackin_telemetry::schema::enums::ErrorType::LaunchFailed),
            );
        }
        drop(self.session.take());
        if self.active {
            jackin_diagnostics::shutdown_capsule_tracing();
            OTLP_ACTIVE.store(false, Ordering::Relaxed);
        }
    }
}

#[must_use]
pub fn otlp_active() -> bool {
    OTLP_ACTIVE.load(Ordering::Relaxed)
}

/// Best-effort emergency exporter shutdown used by the panic hook.
pub fn shutdown() {
    if otlp_active() {
        jackin_diagnostics::shutdown_capsule_tracing();
        OTLP_ACTIVE.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests;

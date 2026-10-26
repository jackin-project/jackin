// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

#[derive(Debug)]
#[must_use = "hold the guard for the daemon's lifetime"]
pub struct FlushGuard {
    session: Option<jackin_telemetry::identity::SessionGuard>,
    startup: Option<jackin_telemetry::OperationGuard>,
    active: bool,
}

impl FlushGuard {
    pub fn listener_ready(&mut self) {
        if let Some(startup) = self.startup.take() {
            startup.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
        }
    }

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

pub fn shutdown() {
    if otlp_active() {
        jackin_diagnostics::shutdown_capsule_tracing();
        OTLP_ACTIVE.store(false, Ordering::Relaxed);
    }
}

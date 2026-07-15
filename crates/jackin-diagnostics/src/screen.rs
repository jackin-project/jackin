// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Per-screen tracing: each TUI screen the operator visits becomes its own
//! trace.
//!
//! A screen is entered with [`enter_screen`], which starts a *detached* root
//! span — a fresh trace id, no parent — and links it back to the screen the
//! operator came from. The result is "separate but connected traces": a
//! backend renders one trace per screen and lets the operator jump along the
//! links, while every screen of one invocation shares the `parallax.run.id`
//! span attribute (the cross-trace grouping glue; never on the OTLP Resource).
//! See the run-telemetry trace-model reference for the full picture.
//!
//! Spans live as long as the returned [`ScreenGuard`]. Because the host TUI is
//! a single-threaded runtime that yields across `.await`, the screen span is
//! *not* held entered across the event loop; instead [`ScreenGuard::in_scope`]
//! enters it around each synchronous dispatch so per-event child spans nest
//! under the right screen. The current screen is tracked in a thread-local,
//! which is sound only because host TUI navigation happens on one thread.

use tracing::Span;

#[cfg(feature = "otlp")]
static CAPSULE_SCREEN_ACTIVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(feature = "otlp")]
use opentelemetry::trace::{SpanContext, TraceContextExt as _};
#[cfg(feature = "otlp")]
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

use crate::observability::otel_keys;

/// A distinct TUI surface. The string is both the span name and the
/// `jackin.screen.name` attribute value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Main workspace-selection list.
    List,
    /// Global settings.
    Settings,
    /// Edit an existing workspace.
    Editor,
    /// Create-workspace flow.
    Create,
    /// Launch flow (selection resolved → container/agent starting).
    Launch,
    /// In-container capsule session.
    Capsule,
}

impl Screen {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Settings => "settings",
            Self::Editor => "editor",
            Self::Create => "create",
            Self::Launch => "launch",
            Self::Capsule => "capsule",
        }
    }
}

#[cfg(feature = "otlp")]
#[derive(Clone, Debug)]
struct ScreenLink {
    name: &'static str,
    span: Span,
    ctx: SpanContext,
}

#[cfg(feature = "otlp")]
thread_local! {
    static CURRENT: std::cell::RefCell<Option<ScreenLink>> =
        const { std::cell::RefCell::new(None) };
    /// A link snapshotted by [`carry_link_forward`] so the next screen entered
    /// after the current one's guard is dropped still links back to it. Needed
    /// for the console→launch handoff: the list screen ends when `run_console`
    /// returns, but the launch flow that starts afterwards must still link to
    /// it.
    static PENDING_LINK: std::cell::RefCell<Option<(&'static str, SpanContext)>> =
        const { std::cell::RefCell::new(None) };
}

/// Active for the lifetime of a screen. Dropping it ends the screen span and
/// restores the previous screen as current (screens nest like a stack).
#[derive(Debug)]
#[must_use = "the screen span ends as soon as the guard is dropped"]
pub struct ScreenGuard {
    span: Span,
    #[cfg(feature = "otlp")]
    previous: Option<ScreenLink>,
}

impl ScreenGuard {
    /// Run a synchronous block with the screen span entered, so any `tracing`
    /// spans/events it creates nest under this screen. Never hold the entered
    /// state across an `.await`; call this around each synchronous dispatch.
    pub fn in_scope<R>(&self, f: impl FnOnce() -> R) -> R {
        self.span.in_scope(f)
    }

    /// A clone of the screen span, for instrumenting an async operation so its
    /// child spans nest under this screen across `.await` points
    /// (`future.instrument(guard.span())`).
    #[must_use]
    pub fn span(&self) -> Span {
        self.span.clone()
    }
}

#[cfg(feature = "otlp")]
impl Drop for ScreenGuard {
    fn drop(&mut self) {
        CURRENT.with(|cell| *cell.borrow_mut() = self.previous.take());
    }
}

/// Enter `screen`, starting a fresh trace linked to the previous screen.
pub fn enter_screen(screen: Screen) -> ScreenGuard {
    let span = tracing::info_span!("screen", otel.name = screen.as_str());

    #[cfg(feature = "otlp")]
    let guard = {
        use opentelemetry::Context;

        // Detach into a new trace: each screen is its own trace root, not a
        // child of whatever span happened to be on the stack.
        drop(span.set_parent(Context::new()));
        span.set_attribute(otel_keys::SCREEN_NAME, screen.as_str());
        span.set_attribute(otel_keys::COMPONENT, "host");
        if let Some(run) = crate::active_run() {
            span.set_attribute(otel_keys::RUN_ID, run.run_id().to_owned());
        }

        let previous = CURRENT.with(|cell| cell.borrow().clone());
        // Link to the live previous screen, or — when there is none because the
        // previous screen's guard already dropped (console→launch handoff) — to
        // the snapshot left by carry_link_forward().
        let link = previous
            .as_ref()
            .map(|prev| (prev.name, prev.ctx.clone()))
            .or_else(|| PENDING_LINK.with(|cell| cell.borrow_mut().take()));
        if let Some((from_name, ctx)) = &link {
            span.add_link(ctx.clone());
            span.set_attribute(otel_keys::SCREEN_FROM, *from_name);
        }

        let span_ctx = span.context().span().span_context().clone();
        CURRENT.with(|cell| {
            *cell.borrow_mut() = Some(ScreenLink {
                name: screen.as_str(),
                span: span.clone(),
                ctx: span_ctx,
            });
        });

        ScreenGuard { span, previous }
    };

    #[cfg(not(feature = "otlp"))]
    let guard = ScreenGuard { span };

    guard
}

/// Tag the current screen span with the selected workspace.
pub fn set_workspace(name: &str) {
    set_current_attr(otel_keys::WORKSPACE, name);
}

/// Tag the current screen span with how the workspace was chosen
/// (`named` / `current-dir`).
pub fn set_workspace_kind(kind: &str) {
    set_current_attr(otel_keys::WORKSPACE_KIND, kind);
}

/// Record a feature-decision event for the selected agent (plan 007).
/// Does **not** stamp agent identity onto generic span attributes.
pub fn set_agent_selected(agent: &str) {
    emit_feature_decision("agent.selected", agent, "selected");
}

/// Record a feature-decision event for active agents (plan 007).
pub fn set_agents_active(agents: &[&str]) {
    emit_feature_decision("agents.active", &agents.join(","), "active");
}

/// Record a feature-decision event for the resolved provider (plan 007).
/// Does **not** stamp provider identity onto generic span attributes.
pub fn set_provider(provider: &str) {
    emit_feature_decision("provider.selected", provider, "selected");
}

/// Current screen name for stamping logs/metrics (`jackin.screen.name`).
#[must_use]
pub fn current_screen_name() -> Option<&'static str> {
    #[cfg(feature = "otlp")]
    {
        CURRENT
            .with(|cell| cell.borrow().as_ref().map(|link| link.name))
            .or_else(|| {
                CAPSULE_SCREEN_ACTIVE
                    .load(std::sync::atomic::Ordering::Relaxed)
                    .then_some(Screen::Capsule.as_str())
            })
    }
    #[cfg(not(feature = "otlp"))]
    {
        None
    }
}

fn emit_feature_decision(key: &str, provider_or_agent: &str, variant: &str) {
    tracing::event!(
        target: "jackin_diagnostics",
        tracing::Level::INFO,
        "event.name" = "feature.decision",
        "feature.key" = key,
        "feature.provider" = provider_or_agent,
        "feature.variant" = variant,
        "jackin.component" = "host",
        "event.outcome" = "success",
        "feature decision"
    );
}

/// Record a discrete operator action (selection, input, confirm, dismiss) as a
/// timestamped event on the current screen span. `target` is the thing acted
/// on, when there is one.
pub fn record_action(action: &str, target: Option<&str>) {
    #[cfg(feature = "otlp")]
    {
        use opentelemetry::KeyValue;
        with_current(|link| {
            let mut attrs = vec![KeyValue::new(otel_keys::ACTION, action.to_owned())];
            if let Some(target) = target {
                attrs.push(KeyValue::new("jackin.action.target", target.to_owned()));
            }
            link.span.add_event("user.action", attrs);
        });
    }
    #[cfg(not(feature = "otlp"))]
    let _ = (action, target);
}

/// Run a launch future under its own `launch` screen trace, tagging the
/// workspace, agent, and provider, so the launch's per-stage spans nest into
/// one trace linked back to the screen that triggered it. `future.instrument`
/// carries the span across the launch's `.await` points. A transparent
/// envelope unless OTLP is active.
pub async fn launch_trace<F>(
    workspace: Option<&str>,
    agent_slug: Option<&str>,
    provider: Option<&str>,
    fut: F,
) -> F::Output
where
    F: Future,
{
    use tracing::Instrument as _;

    let guard = enter_screen(Screen::Launch);
    if let Some(workspace) = workspace {
        set_workspace(workspace);
    }
    if let Some(agent_slug) = agent_slug {
        set_agent_selected(agent_slug);
    }
    if let Some(provider) = provider {
        set_provider(provider);
    }
    let span = guard.span();
    let output = fut.instrument(span).await;
    drop(guard);
    output
}

/// Record a capsule activity — a pane/tab/agent spawn — as a short span in its
/// own trace. The resource's `session.id` rides on it (so it lands on the
/// session timeline) along with the tab label and agent. Used inside the
/// capsule, where each tab is a distinct surface the operator works in.
pub fn record_capsule_activity(label: &str, agent: Option<&str>) {
    #[cfg(feature = "otlp")]
    {
        use opentelemetry::Context;

        use tracing_opentelemetry::OpenTelemetrySpanExt as _;
        CAPSULE_SCREEN_ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
        let span = tracing::info_span!("capsule.tab", otel.name = "capsule:tab");
        drop(span.set_parent(Context::new()));
        span.set_attribute(otel_keys::TAB_LABEL, label.to_owned());
        span.set_attribute(otel_keys::SCREEN_NAME, Screen::Capsule.as_str());
        span.set_attribute(otel_keys::COMPONENT, "capsule");
        // Agent identity is a feature-decision event, not a default span attr.
        if let Some(agent) = agent {
            emit_feature_decision("agent.selected", agent, "tab");
        }
        span.in_scope(|| {
            tracing::event!(
                target: "jackin_capsule",
                tracing::Level::INFO,
                "event.name" = "capsule.tab",
                "jackin.component" = "capsule",
                "jackin.screen.name" = Screen::Capsule.as_str(),
                "event.outcome" = "success",
                "tab spawned: {label}"
            );
        });
    }
    #[cfg(not(feature = "otlp"))]
    let _ = (label, agent);
}

#[cfg(all(test, feature = "otlp"))]
pub(crate) fn reset_capsule_screen_for_test() {
    CAPSULE_SCREEN_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
}

/// Snapshot the current screen as the link target for the next screen entered
/// after this one's guard is dropped. Call it before leaving a screen whose
/// successor starts in a different stack frame (the console list handing off to
/// the launch flow that begins after `run_console` returns).
pub fn carry_link_forward() {
    #[cfg(feature = "otlp")]
    CURRENT.with(|cell| {
        if let Some(link) = cell.borrow().as_ref() {
            let snapshot = (link.name, link.ctx.clone());
            PENDING_LINK.with(|pending| *pending.borrow_mut() = Some(snapshot));
        }
    });
}

/// The W3C `traceparent` of the current screen span, for injecting into a
/// spawned subprocess (the container/capsule) so its telemetry links back.
/// `None` when no screen is active or OTLP is not compiled in.
#[must_use]
pub fn current_traceparent() -> Option<String> {
    #[cfg(feature = "otlp")]
    {
        CURRENT.with(|cell| {
            cell.borrow()
                .as_ref()
                .filter(|link| link.ctx.is_valid())
                .map(|link| format_traceparent(&link.ctx))
        })
    }
    #[cfg(not(feature = "otlp"))]
    None
}

#[cfg(feature = "otlp")]
fn set_current_attr(key: &'static str, value: &str) {
    let value = value.to_owned();
    with_current(|link| link.span.set_attribute(key, value.clone()));
}

#[cfg(not(feature = "otlp"))]
fn set_current_attr(key: &'static str, value: &str) {
    let _ = (key, value);
}

#[cfg(feature = "otlp")]
fn with_current(f: impl FnOnce(&ScreenLink)) {
    CURRENT.with(|cell| {
        if let Some(link) = cell.borrow().as_ref() {
            f(link);
        }
    });
}

/// Format a span context as a W3C `traceparent` header value.
#[cfg(feature = "otlp")]
fn format_traceparent(ctx: &SpanContext) -> String {
    format!(
        "00-{}-{}-{:02x}",
        ctx.trace_id(),
        ctx.span_id(),
        ctx.trace_flags().to_u8()
    )
}

#[cfg(test)]
mod tests;

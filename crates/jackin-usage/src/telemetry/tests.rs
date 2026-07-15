use std::sync::{Arc, Mutex, OnceLock};

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;

use super::{BridgeLevel, bridge_log, set_otlp_active_for_test};

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Clone, Default)]
struct CaptureLayer {
    events: Arc<Mutex<Vec<CapturedEvent>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapturedEvent {
    level: tracing::Level,
    target: String,
    message: String,
    event_name: Option<String>,
    component: Option<String>,
}

impl<S> Layer<S> for CaptureLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        self.events.lock().unwrap().push(CapturedEvent {
            level: *event.metadata().level(),
            target: event.metadata().target().to_owned(),
            message: visitor.message.unwrap_or_default(),
            event_name: visitor.event_name,
            component: visitor.component,
        });
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
    event_name: Option<String>,
    component: Option<String>,
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "event.name" => self.event_name = Some(value.to_owned()),
            "jackin.component" => self.component = Some(value.to_owned()),
            "message" => self.message = Some(value.to_owned()),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" && self.message.is_none() {
            self.message = Some(format!("{value:?}").trim_matches('"').to_owned());
        }
    }
}

#[test]
fn bridge_log_uses_requested_severity_and_target() {
    let _guard = test_lock().lock().unwrap();
    set_otlp_active_for_test(true);
    let layer = CaptureLayer::default();
    let events = Arc::clone(&layer.events);
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        bridge_log(BridgeLevel::Trace, "trace line");
        bridge_log(BridgeLevel::Debug, "debug line");
        bridge_log(BridgeLevel::Info, "info line");
        bridge_log(BridgeLevel::Warn, "warn line");
        bridge_log(BridgeLevel::Error, "error line");
    });
    set_otlp_active_for_test(false);

    let events = events.lock().unwrap().clone();
    assert_eq!(events.len(), 5);
    for event in &events {
        assert_eq!(event.target, "jackin_capsule");
        assert!(
            !event.message.starts_with('['),
            "bridge body must be prefix-free: {}",
            event.message
        );
        assert_eq!(event.component.as_deref(), Some("capsule"));
        assert!(
            event
                .event_name
                .as_deref()
                .is_some_and(|n| n.starts_with("capsule."))
        );
    }
    assert_eq!(events[0].level, tracing::Level::TRACE);
    assert_eq!(events[0].message, "trace line");
    assert_eq!(events[0].event_name.as_deref(), Some("capsule.trace"));
    assert_eq!(events[1].level, tracing::Level::DEBUG);
    assert_eq!(events[1].message, "debug line");
    assert_eq!(events[2].level, tracing::Level::INFO);
    assert_eq!(events[2].message, "info line");
    assert_eq!(events[2].event_name.as_deref(), Some("capsule.log"));
    assert_eq!(events[3].level, tracing::Level::WARN);
    assert_eq!(events[3].message, "warn line");
    assert_eq!(events[4].level, tracing::Level::ERROR);
    assert_eq!(events[4].message, "error line");
    assert_eq!(events[4].event_name.as_deref(), Some("capsule.error"));
}

#[test]
fn bridge_log_is_suppressed_when_otlp_inactive() {
    let _guard = test_lock().lock().unwrap();
    set_otlp_active_for_test(false);
    let layer = CaptureLayer::default();
    let events = Arc::clone(&layer.events);
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        bridge_log(BridgeLevel::Error, "error line");
    });

    assert!(events.lock().unwrap().is_empty());
}

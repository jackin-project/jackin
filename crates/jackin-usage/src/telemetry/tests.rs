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
        });
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: Option<String>,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
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
    assert_eq!(
        events,
        vec![
            CapturedEvent {
                level: tracing::Level::TRACE,
                target: "jackin_capsule".to_owned(),
                message: "trace line".to_owned(),
            },
            CapturedEvent {
                level: tracing::Level::DEBUG,
                target: "jackin_capsule".to_owned(),
                message: "debug line".to_owned(),
            },
            CapturedEvent {
                level: tracing::Level::INFO,
                target: "jackin_capsule".to_owned(),
                message: "info line".to_owned(),
            },
            CapturedEvent {
                level: tracing::Level::WARN,
                target: "jackin_capsule".to_owned(),
                message: "warn line".to_owned(),
            },
            CapturedEvent {
                level: tracing::Level::ERROR,
                target: "jackin_capsule".to_owned(),
                message: "error line".to_owned(),
            },
        ]
    );
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

use super::*;
use opentelemetry::trace::TracerProvider as _;
use tracing_subscriber::prelude::*;

#[test]
fn screen_sequence_is_monotonic_and_visits_end() {
    let mut tracker = ScreenVisitTracker::new();
    tracker
        .enter(schema::enums::ScreenId::WorkspaceList)
        .unwrap();
    assert_eq!(tracker.sequence(), 1);
    tracker
        .enter(schema::enums::ScreenId::WorkspaceEditor)
        .unwrap();
    assert_eq!(tracker.sequence(), 2);
    tracker.exit(schema::enums::TransitionReason::Back).unwrap();
    assert_eq!(tracker.current_screen(), None);
}

#[test]
fn widget_focus_replaces_prior_focus() {
    let mut tracker = WidgetFocusTracker::default();
    tracker.focus("general").unwrap();
    tracker.focus("mounts").unwrap();
    tracker.unfocus().unwrap();
    assert!(tracker.current.is_none());
}

#[test]
fn pending_action_owns_effect_and_one_render_until_taken() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));

    tracing::subscriber::with_default(subscriber, || {
        let attrs = [Attr {
            key: schema::attrs::UI_ACTION_NAME,
            value: Value::Str(schema::enums::UiActionName::TabSwitch.as_str()),
        }];
        remember_action_parent(
            crate::root_operation(&crate::operation::UI_ACTION, &attrs).unwrap(),
        );
        in_pending_action_scope(|| {
            assert_eq!(
                tracing::Span::current().metadata().unwrap().name(),
                "ui.action"
            );
        });
        let parent = take_action_parent().expect("pending action");
        parent.in_scope(|| {
            crate::operation(&crate::operation::UI_RENDER, &[])
                .unwrap()
                .complete(schema::enums::OutcomeValue::Success, None);
        });
        drop(parent);
    });
    provider.force_flush().unwrap();
    let spans = exporter.get_finished_spans().unwrap();
    let action = spans.iter().find(|span| span.name == "ui.action").unwrap();
    let render = spans.iter().find(|span| span.name == "ui.render").unwrap();
    assert_eq!(render.parent_span_id, action.span_context.span_id());
}

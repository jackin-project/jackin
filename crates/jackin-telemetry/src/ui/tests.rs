use super::*;
use opentelemetry::trace::TracerProvider as _;
use tracing_subscriber::prelude::*;

fn span_attr<'a>(
    span: &'a opentelemetry_sdk::trace::SpanData,
    key: &str,
) -> Option<std::borrow::Cow<'a, str>> {
    span.attributes
        .iter()
        .find(|attribute| attribute.key.as_str() == key)
        .map(|attribute| attribute.value.as_str())
}

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
    assert_eq!(tracker.sequence(), 3);
    tracker.exit(schema::enums::TransitionReason::Back).unwrap();
    assert_eq!(tracker.sequence(), 4);
    assert_eq!(tracker.current_screen(), None);
}

#[test]
fn lifecycle_sequence_increases_for_every_enter_and_exit() {
    let mut tracker = ScreenVisitTracker::new();
    tracker
        .enter(schema::enums::ScreenId::WorkspaceList)
        .unwrap();
    assert_eq!(tracker.sequence(), 1);
    tracker
        .transition(
            schema::enums::ScreenId::WorkspaceEditor,
            schema::enums::TransitionReason::Action,
            None,
        )
        .unwrap();
    assert_eq!(tracker.sequence(), 3);
    tracker
        .transition(
            schema::enums::ScreenId::WorkspaceList,
            schema::enums::TransitionReason::Back,
            None,
        )
        .unwrap();
    assert_eq!(tracker.sequence(), 5);
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

#[test]
fn transition_exports_prior_and_destination_under_action() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));

    tracing::subscriber::with_default(subscriber, || {
        let action_attrs = [Attr {
            key: schema::attrs::UI_ACTION_NAME,
            value: Value::Str(schema::enums::UiActionName::WorkspaceOpen.as_str()),
        }];
        remember_action_parent(
            crate::root_operation(&crate::operation::UI_ACTION, &action_attrs).unwrap(),
        );
        let parent = take_action_parent().expect("pending action");
        let mut tracker = ScreenVisitTracker::new();
        tracker
            .enter(schema::enums::ScreenId::WorkspaceList)
            .unwrap();
        tracker
            .transition(
                schema::enums::ScreenId::WorkspaceEditor,
                schema::enums::TransitionReason::Action,
                Some(&parent),
            )
            .unwrap();
        drop(parent);
    });
    provider.force_flush().unwrap();

    let spans = exporter.get_finished_spans().unwrap();
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.name == "ui.screen.transition")
            .count(),
        1,
        "initial screen entry must not create a transition"
    );
    let action = spans.iter().find(|span| span.name == "ui.action").unwrap();
    let transition = spans
        .iter()
        .find(|span| span.name == "ui.screen.transition")
        .unwrap();
    assert_eq!(transition.parent_span_id, action.span_context.span_id());
    assert_eq!(
        span_attr(transition, schema::attrs::UI_SCREEN_PREVIOUS_ID).as_deref(),
        Some("workspace.list")
    );
    assert_eq!(
        span_attr(transition, schema::attrs::std_attrs::APP_SCREEN_ID).as_deref(),
        Some("workspace.editor")
    );
    assert_eq!(
        span_attr(transition, schema::attrs::UI_TRANSITION_REASON).as_deref(),
        Some("action")
    );
}

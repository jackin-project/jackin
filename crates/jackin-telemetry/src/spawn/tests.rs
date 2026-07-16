use super::*;
use opentelemetry::trace::{SpanId, SpanKind, TracerProvider as _};
use tracing_subscriber::prelude::*;

#[tokio::test(flavor = "current_thread")]
async fn spawn_helpers_execute_on_current_thread_runtime() {
    assert_eq!(spawn_joined(async { 42 }).await.unwrap(), 42);
    let handle = spawn_stream("test.stream", std::future::pending::<()>());
    handle.abort();
    assert!(handle.await.unwrap_err().is_cancelled());
}

#[test]
fn thread_helper_executes_work() {
    assert_eq!(thread_joined(|| 42).join().unwrap(), 42);
}

#[tokio::test(flavor = "current_thread")]
async fn prewarm_job_exports_linked_roots_with_shared_job_id() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    let default = tracing::subscriber::set_default(subscriber);

    spawn_prewarm_job(crate::schema::enums::JobType::ImagePrewarm, async {})
        .await
        .unwrap();
    drop(default);
    provider.force_flush().expect("flush prewarm spans");

    let spans = exporter.get_finished_spans().expect("export prewarm spans");
    let producer = spans
        .iter()
        .find(|span| span.name == crate::schema::spans::PREWARM_SCHEDULE)
        .expect("producer span");
    let consumer = spans
        .iter()
        .find(|span| span.name == crate::schema::spans::PREWARM_ATTEMPT)
        .expect("consumer span");
    let job_id = |span: &opentelemetry_sdk::trace::SpanData| {
        span.attributes
            .iter()
            .find(|attribute| attribute.key.as_str() == crate::schema::attrs::JOB_ID)
            .map(|attribute| attribute.value.as_str().into_owned())
            .expect("job.id attribute")
    };

    assert_eq!(producer.span_kind, SpanKind::Producer);
    assert_eq!(consumer.span_kind, SpanKind::Consumer);
    assert_eq!(producer.parent_span_id, SpanId::INVALID);
    assert_eq!(consumer.parent_span_id, SpanId::INVALID);
    assert_ne!(
        producer.span_context.trace_id(),
        consumer.span_context.trace_id()
    );
    assert_eq!(job_id(producer), job_id(consumer));
    assert_eq!(consumer.links.len(), 1);
    assert_eq!(
        consumer.links[0].span_context.span_id(),
        producer.span_context.span_id()
    );
    assert_eq!(
        consumer.links[0].span_context.trace_id(),
        producer.span_context.trace_id()
    );
}

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

#[tokio::test(flavor = "current_thread")]
async fn joined_and_ownership_only_helpers_have_distinct_context() {
    let default = tracing::subscriber::set_default(tracing_subscriber::registry());
    let parent = tracing::info_span!("spawn.parent");
    let parent_id = parent.id().expect("parent id");
    let entered = parent.enter();
    let joined = spawn_joined(async { Span::current().id() });
    let cycle = spawn_cycle("test.cycle", async { Span::current().id() });
    let stream = spawn_stream("test.stream", async { Span::current().id() });
    drop(entered);
    drop(parent);

    assert_eq!(joined.await.unwrap(), Some(parent_id));
    assert_eq!(cycle.await.unwrap(), None);
    assert_eq!(stream.await.unwrap(), None);
    drop(default);
}

#[tokio::test(flavor = "current_thread")]
async fn handle_blocking_and_local_helpers_execute() {
    let handle = Handle::current();
    assert_eq!(spawn_joined_on(&handle, async { 7 }).await.unwrap(), 7);
    assert_eq!(joined_blocking_on(&handle, || 8).await.unwrap(), 8);

    let local = LocalSet::new();
    let task = spawn_local_joined_on(&local, async { 9 });
    assert_eq!(local.run_until(task).await.unwrap(), 9);
    let result = local
        .run_until(async { spawn_local_joined(async { 10 }).await.unwrap() })
        .await;
    assert_eq!(result, 10);

    let mut joined = JoinSet::new();
    joined.spawn_joined_on_handle(&handle, async { 11 });
    assert_eq!(joined.join_next().await.unwrap().unwrap(), 11);
    joined.spawn_joined_blocking_on(|| 12);
    assert_eq!(joined.join_next().await.unwrap().unwrap(), 12);

    let mut local_joined = JoinSet::new();
    local_joined.spawn_local_joined_on_set(&local, async { 13 });
    assert_eq!(
        local
            .run_until(local_joined.join_next())
            .await
            .unwrap()
            .unwrap(),
        13
    );
}

#[test]
fn thread_helper_executes_work() {
    assert_eq!(thread_joined(|| 42).join().unwrap(), 42);
    assert_eq!(
        thread_joined_named("joined-test".to_owned(), || {
            (thread::current().name().map(str::to_owned), 43)
        })
        .unwrap()
        .join()
        .unwrap(),
        (Some("joined-test".to_owned()), 43)
    );
    let borrowed = String::from("borrowed");
    thread::scope(|scope| {
        assert_eq!(
            thread_scoped_joined(scope, || borrowed.len())
                .join()
                .unwrap(),
            borrowed.len()
        );
    });
}

#[tokio::test(flavor = "current_thread")]
async fn cycle_does_not_retain_the_caller_span_lifetime() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    let default = tracing::subscriber::set_default(subscriber);

    let parent = tracing::info_span!("caller.lifetime");
    let entered = parent.enter();
    let handle = spawn_cycle("test.cycle", std::future::pending::<()>());
    drop(entered);
    drop(parent);
    provider.force_flush().expect("flush caller span");
    assert!(
        exporter
            .get_finished_spans()
            .expect("export caller span")
            .iter()
            .any(|span| span.name == "caller.lifetime")
    );
    handle.abort();
    drop(default);
}

#[tokio::test(flavor = "current_thread")]
async fn detached_helper_exports_a_linked_root() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    let default = tracing::subscriber::set_default(subscriber);
    let parent = tracing::info_span!("detached.parent");
    let parent_context = parent.context().span().span_context().clone();
    let entered = parent.enter();
    spawn_detached(&crate::operation::PROCESS_COMMAND, async {})
        .await
        .unwrap();
    drop(entered);
    drop(parent);
    drop(default);
    provider.force_flush().expect("flush detached spans");

    let spans = exporter
        .get_finished_spans()
        .expect("export detached spans");
    let detached = spans
        .iter()
        .find(|span| span.name == crate::schema::spans::PROCESS_COMMAND)
        .expect("detached root");
    assert_eq!(detached.parent_span_id, SpanId::INVALID);
    assert_eq!(detached.links.len(), 1);
    assert_eq!(
        detached.links[0].span_context.span_id(),
        parent_context.span_id()
    );
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

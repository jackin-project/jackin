use super::*;
use opentelemetry::trace::{SpanId, SpanKind, TracerProvider as _};
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
    let joined_on = spawn_joined_on(&Handle::current(), async { Span::current().id() });
    let blocking = joined_blocking(|| Span::current().id());
    let thread = thread_joined(|| Span::current().id());
    let local = LocalSet::new();
    let local_joined = spawn_local_joined_on(&local, async { Span::current().id() });
    let mut tasks = JoinSet::new();
    tasks.spawn_joined_on(async { Span::current().id() });
    let cycle = spawn_cycle("test.cycle", async { Span::current().id() });
    let stream = spawn_stream("test.stream", async { Span::current().id() });
    drop(entered);
    drop(parent);

    assert_eq!(joined.await.unwrap(), Some(parent_id.clone()));
    assert_eq!(joined_on.await.unwrap(), Some(parent_id.clone()));
    assert_eq!(blocking.await.unwrap(), Some(parent_id.clone()));
    assert_eq!(thread.join().unwrap(), Some(parent_id.clone()));
    assert_eq!(
        local.run_until(local_joined).await.unwrap(),
        Some(parent_id.clone())
    );
    assert_eq!(tasks.join_next().await.unwrap().unwrap(), Some(parent_id));
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn helpers_execute_on_multi_thread_runtime() {
    let handle = Handle::current();
    assert_eq!(spawn_joined_on(&handle, async { 21 }).await.unwrap(), 21);
    assert_eq!(joined_blocking(|| 22).await.unwrap(), 22);
    let mut tasks = JoinSet::new();
    tasks.spawn_joined_on(async { 23 });
    assert_eq!(tasks.join_next().await.unwrap().unwrap(), 23);
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
        assert_eq!(
            thread_scoped_joined_named(scope, "scoped-joined".to_owned(), || borrowed.len())
                .unwrap()
                .join()
                .unwrap(),
            borrowed.len()
        );
        assert_eq!(
            thread_scoped_stream(scope, "scoped-stream", || borrowed.len())
                .join()
                .unwrap(),
            borrowed.len()
        );
    });
    assert_eq!(thread_stream("stream", || 44).join().unwrap(), 44);
    assert_eq!(
        thread_stream_named("stream-named".to_owned(), || 45)
            .unwrap()
            .join()
            .unwrap(),
        45
    );
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
    spawn_detached(&crate::operation::PROCESS_COMMAND, async {}, |()| {
        DetachedCompletion::success()
    })
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
async fn detached_helpers_preserve_outputs_and_classify_outcomes() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    let default = tracing::subscriber::set_default(subscriber);

    assert_eq!(
        spawn_detached_on(
            &Handle::current(),
            &crate::operation::PROCESS_COMMAND,
            async { 31 },
            |_| DetachedCompletion::success(),
        )
        .await
        .unwrap(),
        31
    );
    assert_eq!(
        detached_blocking(
            &crate::operation::PROCESS_COMMAND,
            || 32,
            |_| DetachedCompletion::failure(crate::schema::enums::ErrorType::LaunchFailed),
        )
        .await
        .unwrap(),
        32
    );
    assert_eq!(
        thread_detached(
            &crate::operation::PROCESS_COMMAND,
            || 33,
            |_| DetachedCompletion::error(crate::schema::enums::ErrorType::RpcError),
        )
        .join()
        .unwrap(),
        33
    );
    spawn_detached_with_completion(&crate::operation::PROCESS_COMMAND, async {
        DetachedCompletion::timeout()
    })
    .await
    .unwrap();
    let mut tasks = JoinSet::new();
    tasks.spawn_detached_on(&crate::operation::PROCESS_COMMAND, async { 34 }, |_| {
        DetachedCompletion::success()
    });
    assert_eq!(tasks.join_next().await.unwrap().unwrap(), 34);

    drop(default);
    provider.force_flush().expect("flush detached outcomes");
    let spans = exporter.get_finished_spans().expect("detached outcomes");
    let outcomes = spans
        .iter()
        .filter_map(|span| span_attr(span, crate::schema::attrs::OUTCOME))
        .collect::<Vec<_>>();
    assert!(outcomes.iter().any(|value| value == "success"));
    assert!(outcomes.iter().any(|value| value == "failure"));
    assert!(outcomes.iter().any(|value| value == "error"));
    assert!(outcomes.iter().any(|value| value == "timeout"));
}

#[tokio::test(flavor = "current_thread")]
async fn detached_helpers_record_panic_and_abort() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    let default = tracing::subscriber::set_default(subscriber);

    let panic_task = spawn_detached(
        &crate::operation::PROCESS_COMMAND,
        async { panic!("detached panic") },
        |&()| DetachedCompletion::success(),
    );
    assert!(panic_task.await.unwrap_err().is_panic());

    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let aborted = spawn_detached(
        &crate::operation::PROCESS_COMMAND,
        async move {
            let _send_result = started_tx.send(());
            std::future::pending::<()>().await;
        },
        |()| DetachedCompletion::success(),
    );
    started_rx.await.expect("detached task started");
    aborted.abort();
    assert!(aborted.await.unwrap_err().is_cancelled());

    drop(default);
    provider.force_flush().expect("flush panic and abort");
    let spans = exporter
        .get_finished_spans()
        .expect("panic and abort spans");
    assert!(spans.iter().any(|span| {
        span_attr(span, crate::schema::attrs::OUTCOME).as_deref() == Some("error")
            && span_attr(span, crate::schema::attrs::std_attrs::ERROR_TYPE).as_deref()
                == Some("panic")
    }));
    assert!(spans.iter().any(|span| {
        span_attr(span, crate::schema::attrs::OUTCOME).as_deref() == Some("cancellation")
    }));
}

#[tokio::test(flavor = "current_thread")]
async fn detached_links_unsampled_context_but_ignores_invalid_context() {
    use opentelemetry::trace::{SpanContext, TraceFlags, TraceId, TraceState};

    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    let default = tracing::subscriber::set_default(subscriber);

    spawn_detached(&crate::operation::PROCESS_COMMAND, async {}, |()| {
        DetachedCompletion::success()
    })
    .await
    .unwrap();

    let unsampled = SpanContext::new(
        TraceId::from(1_u128),
        SpanId::from(2_u64),
        TraceFlags::default(),
        true,
        TraceState::default(),
    );
    let parent = tracing::info_span!("unsampled.parent");
    drop(parent.set_parent(opentelemetry::Context::new().with_remote_span_context(unsampled)));
    let entered = parent.enter();
    spawn_detached(&crate::operation::PROCESS_COMMAND, async {}, |()| {
        DetachedCompletion::success()
    })
    .await
    .unwrap();
    drop(entered);
    drop(parent);
    drop(default);
    provider.force_flush().expect("flush link validity");

    let spans = exporter.get_finished_spans().expect("link validity spans");
    let detached = spans
        .iter()
        .filter(|span| span.name == crate::schema::spans::PROCESS_COMMAND)
        .collect::<Vec<_>>();
    assert_eq!(detached.len(), 2);
    assert!(detached.iter().any(|span| span.links.is_empty()));
    assert!(
        detached
            .iter()
            .any(|span| { span.links.len() == 1 && !span.links[0].span_context.is_sampled() })
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

    spawn_prewarm_job(
        crate::schema::enums::JobType::ImagePrewarm,
        async {},
        |()| DetachedCompletion::success(),
    )
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
    assert_eq!(
        span_attr(consumer, crate::schema::attrs::OUTCOME).as_deref(),
        Some("success")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn prewarm_job_classifies_failure_error_timeout_panic_and_abort() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    let default = tracing::subscriber::set_default(subscriber);

    for completion in [
        DetachedCompletion::failure(crate::schema::enums::ErrorType::LaunchFailed),
        DetachedCompletion::error(crate::schema::enums::ErrorType::RpcError),
        DetachedCompletion::timeout(),
    ] {
        spawn_prewarm_job(
            crate::schema::enums::JobType::ImagePrewarm,
            async {},
            move |()| completion,
        )
        .await
        .unwrap();
    }

    let panic_task = spawn_prewarm_job(
        crate::schema::enums::JobType::ImagePrewarm,
        async { panic!("prewarm panic") },
        |&()| DetachedCompletion::success(),
    );
    assert!(panic_task.await.unwrap_err().is_panic());

    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let aborted = spawn_prewarm_job(
        crate::schema::enums::JobType::SidecarPrewarm,
        async move {
            let _send_result = started_tx.send(());
            std::future::pending::<()>().await;
        },
        |()| DetachedCompletion::success(),
    );
    started_rx.await.expect("prewarm task started");
    aborted.abort();
    assert!(aborted.await.unwrap_err().is_cancelled());

    drop(default);
    provider.force_flush().expect("flush prewarm outcomes");
    let spans = exporter.get_finished_spans().expect("prewarm outcomes");
    let attempts = spans
        .iter()
        .filter(|span| span.name == crate::schema::spans::PREWARM_ATTEMPT)
        .collect::<Vec<_>>();
    for outcome in ["failure", "error", "timeout", "cancellation"] {
        assert!(attempts.iter().any(|span| {
            span_attr(span, crate::schema::attrs::OUTCOME).as_deref() == Some(outcome)
        }));
    }
    assert!(attempts.iter().any(|span| {
        span_attr(span, crate::schema::attrs::OUTCOME).as_deref() == Some("error")
            && span_attr(span, crate::schema::attrs::std_attrs::ERROR_TYPE).as_deref()
                == Some("panic")
    }));
}

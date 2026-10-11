// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
async fn prewarm_job_exports_one_consumer_per_attempt() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    let default = tracing::subscriber::set_default(subscriber);

    spawn_prewarm_job_attempts(
        crate::schema::enums::JobType::ImagePrewarm,
        |attempts| async move {
            attempts
                .run(async { true }, |_| DetachedCompletion::success())
                .await;
            attempts
                .run(async { false }, |_| {
                    DetachedCompletion::failure(crate::schema::enums::ErrorType::LaunchFailed)
                })
                .await;
        },
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
    let consumers = spans
        .iter()
        .filter(|span| span.name == crate::schema::spans::PREWARM_ATTEMPT)
        .collect::<Vec<_>>();
    assert_eq!(consumers.len(), 2);
    for consumer in consumers {
        assert_eq!(consumer.links.len(), 1);
        assert_eq!(
            consumer.links[0].span_context.span_id(),
            producer.span_context.span_id()
        );
        assert_eq!(
            span_attr(consumer, crate::schema::attrs::JOB_ID),
            span_attr(producer, crate::schema::attrs::JOB_ID)
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn prewarm_job_classifies_skip_failure_error_timeout_panic_and_abort() {
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));
    let default = tracing::subscriber::set_default(subscriber);

    for completion in [
        DetachedCompletion::skip(),
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
    for outcome in ["skip", "failure", "error", "timeout", "cancellation"] {
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

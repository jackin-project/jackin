// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

#[tokio::test(flavor = "current_thread")]
async fn joined_blocking_preserves_default_dispatcher_without_active_span() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[derive(Clone)]
    struct EventSeen(Arc<AtomicBool>);

    impl<S> tracing_subscriber::Layer<S> for EventSeen
    where
        S: tracing::Subscriber,
    {
        fn on_event(
            &self,
            _event: &tracing::Event<'_>,
            _context: tracing_subscriber::layer::Context<'_, S>,
        ) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let seen = Arc::new(AtomicBool::new(false));
    let subscriber = tracing_subscriber::registry().with(EventSeen(Arc::clone(&seen)));
    let _default = tracing::subscriber::set_default(subscriber);
    let joined = joined_blocking(|| tracing::info!("joined blocking event"));

    joined.await.expect("join blocking work");
    assert!(seen.load(Ordering::SeqCst));
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
    let pr_cycle_attrs = [crate::Attr {
        key: crate::schema::attrs::BACKGROUND_CYCLE_NAME,
        value: crate::Value::Str(crate::schema::enums::BackgroundCycleName::PrContext.as_str()),
    }];
    assert_eq!(
        detached_blocking_with_attrs(
            &crate::operation::BACKGROUND_CYCLE,
            &pr_cycle_attrs,
            || 320,
            |_| DetachedCompletion::recovered_degradation(),
        )
        .await
        .unwrap(),
        320
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
    let branch_cycle_attrs = [crate::Attr {
        key: crate::schema::attrs::BACKGROUND_CYCLE_NAME,
        value: crate::Value::Str(crate::schema::enums::BackgroundCycleName::BranchContext.as_str()),
    }];
    assert_eq!(
        thread_detached_named_with_attrs(
            "test-branch-context".to_owned(),
            &crate::operation::BACKGROUND_CYCLE,
            &branch_cycle_attrs,
            || 330,
            |_| DetachedCompletion::success(),
        )
        .unwrap()
        .join()
        .unwrap(),
        330
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
    let cycle_names = spans
        .iter()
        .filter(|span| span.name == crate::schema::spans::BACKGROUND_CYCLE)
        .filter_map(|span| span_attr(span, crate::schema::attrs::BACKGROUND_CYCLE_NAME))
        .collect::<Vec<_>>();
    assert!(cycle_names.iter().any(|value| value == "pr_context"));
    assert!(cycle_names.iter().any(|value| value == "branch_context"));
    let pr_cycle = spans
        .iter()
        .find(|span| {
            span.name == crate::schema::spans::BACKGROUND_CYCLE
                && span_attr(span, crate::schema::attrs::BACKGROUND_CYCLE_NAME).as_deref()
                    == Some("pr_context")
        })
        .expect("PR background cycle");
    assert_eq!(
        span_attr(pr_cycle, crate::schema::attrs::OUTCOME).as_deref(),
        Some("success")
    );
    assert_eq!(
        span_attr(pr_cycle, crate::schema::attrs::std_attrs::ERROR_TYPE).as_deref(),
        Some("recovered_degradation")
    );
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

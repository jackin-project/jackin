// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{future::Future, thread, time::Instant};

use opentelemetry::trace::TraceContextExt as _;
use tokio::{
    runtime::Handle,
    task::{JoinHandle, JoinSet, LocalSet},
};
use tracing::{Instrument as _, Span, instrument::WithSubscriber as _};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

use crate::operation::{SpanDef, root_operation};

#[derive(Clone, Copy, Debug)]
pub struct DetachedCompletion {
    pub outcome: crate::schema::enums::OutcomeValue,
    pub error_type: Option<crate::schema::enums::ErrorType>,
}

impl DetachedCompletion {
    #[must_use]
    pub const fn success() -> Self {
        Self {
            outcome: crate::schema::enums::OutcomeValue::Success,
            error_type: None,
        }
    }

    #[must_use]
    pub const fn failure(error_type: crate::schema::enums::ErrorType) -> Self {
        Self {
            outcome: crate::schema::enums::OutcomeValue::Failure,
            error_type: Some(error_type),
        }
    }

    #[must_use]
    pub const fn error(error_type: crate::schema::enums::ErrorType) -> Self {
        Self {
            outcome: crate::schema::enums::OutcomeValue::Error,
            error_type: Some(error_type),
        }
    }

    #[must_use]
    pub const fn timeout() -> Self {
        Self {
            outcome: crate::schema::enums::OutcomeValue::Timeout,
            error_type: Some(crate::schema::enums::ErrorType::Timeout),
        }
    }

    #[must_use]
    pub const fn skip() -> Self {
        Self {
            outcome: crate::schema::enums::OutcomeValue::Skip,
            error_type: None,
        }
    }

    #[must_use]
    pub const fn recovered_degradation() -> Self {
        Self {
            outcome: crate::schema::enums::OutcomeValue::Success,
            error_type: Some(crate::schema::enums::ErrorType::RecoveredDegradation),
        }
    }
}

struct DetachedGuard(Option<crate::operation::OperationGuard>);

impl DetachedGuard {
    fn new(
        def: &'static SpanDef,
        attrs: &[crate::Attr<'_>],
        parent: &opentelemetry::trace::SpanContext,
    ) -> Self {
        let operation = root_operation(def, attrs).ok();
        if parent.is_valid()
            && let Some(operation) = &operation
        {
            let _link_result = operation.link(parent);
        }
        Self(operation)
    }

    fn span(&self) -> Span {
        self.0
            .as_ref()
            .map_or_else(Span::none, |operation| operation.span().clone())
    }

    fn complete(mut self, completion: DetachedCompletion) {
        if let Some(operation) = self.0.take() {
            if matches!(
                completion.outcome,
                crate::schema::enums::OutcomeValue::Failure
                    | crate::schema::enums::OutcomeValue::Error
                    | crate::schema::enums::OutcomeValue::Timeout
            ) && let Some(error_type) = completion.error_type
            {
                operation.span().in_scope(|| {
                    let _error = crate::record_error(error_type);
                });
            }
            operation.complete(completion.outcome, completion.error_type);
        }
    }
}

impl Drop for DetachedGuard {
    fn drop(&mut self) {
        let Some(operation) = self.0.take() else {
            return;
        };
        if thread::panicking() {
            operation.span().in_scope(|| {
                let _error = crate::record_error(crate::schema::enums::ErrorType::Panic);
            });
            operation.complete(
                crate::schema::enums::OutcomeValue::Error,
                Some(crate::schema::enums::ErrorType::Panic),
            );
        } else {
            operation.complete(crate::schema::enums::OutcomeValue::Cancellation, None);
        }
    }
}

#[derive(Debug)]
struct PrewarmAttemptMetrics {
    job_type: crate::schema::enums::JobType,
    started_at: Instant,
    completed: bool,
}

impl PrewarmAttemptMetrics {
    fn start(job_type: crate::schema::enums::JobType) -> Self {
        let attrs = [crate::Attr {
            key: crate::schema::attrs::JOB_TYPE,
            value: crate::Value::Str(job_type.as_str()),
        }];
        let _active = crate::up_down_counter(&crate::metric::PREWARM_ACTIVE).add(1, &attrs);
        Self {
            job_type,
            started_at: Instant::now(),
            completed: false,
        }
    }

    fn finish(&mut self, completion: DetachedCompletion) {
        self.completed = true;
        let active_attrs = [crate::Attr {
            key: crate::schema::attrs::JOB_TYPE,
            value: crate::Value::Str(self.job_type.as_str()),
        }];
        let _active = crate::up_down_counter(&crate::metric::PREWARM_ACTIVE).add(-1, &active_attrs);
        let mut duration_attrs = vec![
            active_attrs[0],
            crate::Attr {
                key: crate::schema::attrs::OUTCOME,
                value: crate::Value::Str(completion.outcome.as_str()),
            },
        ];
        if let Some(error_type) = completion.error_type {
            duration_attrs.push(crate::Attr {
                key: crate::schema::attrs::std_attrs::ERROR_TYPE,
                value: crate::Value::Str(error_type.as_str()),
            });
        }
        let _duration = crate::histogram(&crate::metric::PREWARM_DURATION)
            .record(self.started_at.elapsed().as_secs_f64(), &duration_attrs);
    }
}

impl Drop for PrewarmAttemptMetrics {
    fn drop(&mut self) {
        if !self.completed {
            let completion = if thread::panicking() {
                DetachedCompletion::error(crate::schema::enums::ErrorType::Panic)
            } else {
                DetachedCompletion {
                    outcome: crate::schema::enums::OutcomeValue::Cancellation,
                    error_type: None,
                }
            };
            self.finish(completion);
        }
    }
}

pub fn spawn_joined<F>(fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(fut.instrument(Span::current()).with_current_subscriber())
}

pub fn spawn_cycle<F>(_name: &'static str, fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(fut)
}

pub fn spawn_stream<F>(_name: &'static str, fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(fut)
}

pub fn spawn_joined_on<F>(handle: &Handle, fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    handle.spawn(fut.instrument(Span::current()).with_current_subscriber())
}

pub fn spawn_local_joined<F>(fut: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    tokio::task::spawn_local(fut.instrument(Span::current()).with_current_subscriber())
}

pub fn spawn_local_joined_on<F>(local_set: &LocalSet, fut: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    local_set.spawn_local(fut.instrument(Span::current()).with_current_subscriber())
}

pub fn spawn_detached<F, C>(def: &'static SpanDef, fut: F, classify: C) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
    C: FnOnce(&F::Output) -> DetachedCompletion + Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    let guard = DetachedGuard::new(def, &[], &parent);
    tokio::spawn(run_detached(guard, fut, classify).with_current_subscriber())
}

pub fn spawn_detached_on<F, C>(
    handle: &Handle,
    def: &'static SpanDef,
    fut: F,
    classify: C,
) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
    C: FnOnce(&F::Output) -> DetachedCompletion + Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    let guard = DetachedGuard::new(def, &[], &parent);
    handle.spawn(run_detached(guard, fut, classify).with_current_subscriber())
}

/// Spawn detached work with a bounded, registry-validated operation shape.
pub fn spawn_detached_with_attrs<F, C>(
    def: &'static SpanDef,
    attrs: &[crate::Attr<'_>],
    fut: F,
    classify: C,
) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
    C: FnOnce(&F::Output) -> DetachedCompletion + Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    let guard = DetachedGuard::new(def, attrs, &parent);
    tokio::spawn(run_detached(guard, fut, classify).with_current_subscriber())
}

async fn run_detached<F, C>(guard: DetachedGuard, fut: F, classify: C) -> F::Output
where
    F: Future,
    C: FnOnce(&F::Output) -> DetachedCompletion,
{
    let output = fut.instrument(guard.span()).await;
    guard.complete(classify(&output));
    output
}

pub fn spawn_detached_with_completion<F>(def: &'static SpanDef, fut: F) -> JoinHandle<()>
where
    F: Future<Output = DetachedCompletion> + Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    let guard = DetachedGuard::new(def, &[], &parent);
    tokio::spawn(
        async move {
            let completion = fut.instrument(guard.span()).await;
            guard.complete(completion);
        }
        .with_current_subscriber(),
    )
}

/// Schedule detached prewarm work as a PRODUCER decision linked to one
/// CONSUMER attempt with a shared durable job identity.
pub fn spawn_prewarm_job<F, C>(
    job_type: crate::schema::enums::JobType,
    fut: F,
    classify: C,
) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
    C: FnOnce(&F::Output) -> DetachedCompletion + Send + 'static,
{
    spawn_prewarm_job_attempts(job_type, |attempts| async move {
        attempts.run(fut, classify).await
    })
}

#[derive(Clone, Debug)]
pub struct PrewarmJobAttempts {
    job_id: String,
    job_type: crate::schema::enums::JobType,
    producer_context: Option<opentelemetry::trace::SpanContext>,
}

impl PrewarmJobAttempts {
    pub async fn run<F, C>(&self, fut: F, classify: C) -> F::Output
    where
        F: Future + Send,
        F::Output: Send,
        C: FnOnce(&F::Output) -> DetachedCompletion,
    {
        let mut metrics = PrewarmAttemptMetrics::start(self.job_type);
        let attrs = [
            crate::Attr {
                key: crate::schema::attrs::JOB_ID,
                value: crate::Value::Str(&self.job_id),
            },
            crate::Attr {
                key: crate::schema::attrs::JOB_TYPE,
                value: crate::Value::Str(self.job_type.as_str()),
            },
        ];
        let consumer = root_operation(&crate::operation::PREWARM_ATTEMPT, &attrs).ok();
        if let Some(producer_context) = self.producer_context.as_ref()
            && producer_context.is_valid()
            && let Some(consumer) = &consumer
        {
            let _link_result = consumer.link(producer_context);
        }
        let guard = DetachedGuard(consumer);
        let output = fut.instrument(guard.span()).await;
        let completion = classify(&output);
        guard.complete(completion);
        metrics.finish(completion);
        output
    }
}

pub fn spawn_prewarm_job_attempts<F, Fut>(
    job_type: crate::schema::enums::JobType,
    work: F,
) -> JoinHandle<Fut::Output>
where
    F: FnOnce(PrewarmJobAttempts) -> Fut + Send + 'static,
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    let job_id = uuid::Uuid::new_v4().to_string();
    let attrs = [
        crate::Attr {
            key: crate::schema::attrs::JOB_ID,
            value: crate::Value::Str(&job_id),
        },
        crate::Attr {
            key: crate::schema::attrs::JOB_TYPE,
            value: crate::Value::Str(job_type.as_str()),
        },
    ];
    let producer_context = root_operation(&crate::operation::PREWARM_SCHEDULE, &attrs)
        .ok()
        .map(|producer| {
            let context = producer.span().context().span().span_context().clone();
            producer.complete(crate::schema::enums::OutcomeValue::Success, None);
            context
        });
    let metric_attrs = [crate::Attr {
        key: crate::schema::attrs::JOB_TYPE,
        value: crate::Value::Str(job_type.as_str()),
    }];
    let _counter_result = crate::counter(&crate::metric::PREWARM_JOBS).add(1, &metric_attrs);

    let attempts = PrewarmJobAttempts {
        job_id,
        job_type,
        producer_context,
    };
    tokio::spawn(work(attempts).with_current_subscriber())
}

pub fn joined_blocking<F, R>(work: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let span = Span::current();
    tokio::task::spawn_blocking(move || in_span_scope(span, work))
}

pub fn joined_blocking_on<F, R>(handle: &Handle, work: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let span = Span::current();
    handle.spawn_blocking(move || in_span_scope(span, work))
}

pub fn detached_blocking<F, C, R>(def: &'static SpanDef, work: F, classify: C) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    C: FnOnce(&R) -> DetachedCompletion + Send + 'static,
    R: Send + 'static,
{
    detached_blocking_with_attrs(def, &[], work, classify)
}

/// Run detached blocking work with a bounded, registry-validated operation shape.
pub fn detached_blocking_with_attrs<F, C, R>(
    def: &'static SpanDef,
    attrs: &[crate::Attr<'_>],
    work: F,
    classify: C,
) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    C: FnOnce(&R) -> DetachedCompletion + Send + 'static,
    R: Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    let guard = DetachedGuard::new(def, attrs, &parent);
    tokio::task::spawn_blocking(move || {
        let result = in_span_scope(guard.span(), work);
        guard.complete(classify(&result));
        result
    })
}

pub fn stream_blocking<F, R>(_name: &'static str, work: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    tokio::task::spawn_blocking(work)
}

pub fn thread_joined<F, R>(work: F) -> thread::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let span = Span::current();
    let dispatcher = tracing::dispatcher::get_default(Clone::clone);
    thread::spawn(move || {
        tracing::dispatcher::with_default(&dispatcher, || in_span_scope(span, work))
    })
}

pub fn thread_stream<F, R>(_name: &'static str, work: F) -> thread::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    thread::spawn(work)
}

pub fn thread_joined_named<F, R>(name: String, work: F) -> std::io::Result<thread::JoinHandle<R>>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let span = Span::current();
    let dispatcher = tracing::dispatcher::get_default(Clone::clone);
    thread::Builder::new()
        .name(name)
        .spawn(move || tracing::dispatcher::with_default(&dispatcher, || in_span_scope(span, work)))
}

pub fn thread_stream_named<F, R>(name: String, work: F) -> std::io::Result<thread::JoinHandle<R>>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    thread::Builder::new().name(name).spawn(work)
}

pub fn thread_scoped_joined<'scope, F, R>(
    scope: &'scope thread::Scope<'scope, '_>,
    work: F,
) -> thread::ScopedJoinHandle<'scope, R>
where
    F: FnOnce() -> R + Send + 'scope,
    R: Send + 'scope,
{
    let span = Span::current();
    scope.spawn(move || in_span_scope(span, work))
}

pub fn thread_scoped_joined_named<'scope, F, R>(
    scope: &'scope thread::Scope<'scope, '_>,
    name: String,
    work: F,
) -> std::io::Result<thread::ScopedJoinHandle<'scope, R>>
where
    F: FnOnce() -> R + Send + 'scope,
    R: Send + 'scope,
{
    let span = Span::current();
    thread::Builder::new()
        .name(name)
        .spawn_scoped(scope, move || in_span_scope(span, work))
}

pub fn thread_scoped_stream<'scope, F, R>(
    scope: &'scope thread::Scope<'scope, '_>,
    _name: &'static str,
    work: F,
) -> thread::ScopedJoinHandle<'scope, R>
where
    F: FnOnce() -> R + Send + 'scope,
    R: Send + 'scope,
{
    scope.spawn(work)
}

pub fn thread_detached<F, C, R>(
    def: &'static SpanDef,
    work: F,
    classify: C,
) -> thread::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    C: FnOnce(&R) -> DetachedCompletion + Send + 'static,
    R: Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    let guard = DetachedGuard::new(def, &[], &parent);
    thread::spawn(move || {
        let result = in_span_scope(guard.span(), work);
        guard.complete(classify(&result));
        result
    })
}

pub fn thread_detached_named<F, C, R>(
    name: String,
    def: &'static SpanDef,
    work: F,
    classify: C,
) -> std::io::Result<thread::JoinHandle<R>>
where
    F: FnOnce() -> R + Send + 'static,
    C: FnOnce(&R) -> DetachedCompletion + Send + 'static,
    R: Send + 'static,
{
    thread_detached_named_with_attrs(name, def, &[], work, classify)
}

/// Run a named detached thread with a bounded, registry-validated operation shape.
pub fn thread_detached_named_with_attrs<F, C, R>(
    name: String,
    def: &'static SpanDef,
    attrs: &[crate::Attr<'_>],
    work: F,
    classify: C,
) -> std::io::Result<thread::JoinHandle<R>>
where
    F: FnOnce() -> R + Send + 'static,
    C: FnOnce(&R) -> DetachedCompletion + Send + 'static,
    R: Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    let guard = std::sync::Arc::new(std::sync::Mutex::new(Some(DetachedGuard::new(
        def, attrs, &parent,
    ))));
    let worker_guard = std::sync::Arc::clone(&guard);
    let spawned = thread::Builder::new().name(name).spawn(move || {
        let guard = worker_guard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let Some(guard) = guard else {
            let _error =
                crate::record_error(crate::schema::enums::ErrorType::TelemetryInstrumentationFault);
            return work();
        };
        let result = in_span_scope(guard.span(), work);
        guard.complete(classify(&result));
        result
    });
    if spawned.is_err()
        && let Some(guard) = guard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    {
        guard.complete(DetachedCompletion::error(
            crate::schema::enums::ErrorType::ProcessSpawnError,
        ));
    }
    spawned
}

pub trait JoinSetExt<T: Send + 'static> {
    fn spawn_joined_on<F>(&mut self, fut: F) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + Send + 'static;

    fn spawn_joined_on_handle<F>(&mut self, handle: &Handle, fut: F) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + Send + 'static;

    fn spawn_local_joined_on<F>(&mut self, fut: F) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + 'static;

    fn spawn_local_joined_on_set<F>(
        &mut self,
        local_set: &LocalSet,
        fut: F,
    ) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + 'static;

    fn spawn_joined_blocking_on<F>(&mut self, work: F) -> tokio::task::AbortHandle
    where
        F: FnOnce() -> T + Send + 'static;

    fn spawn_detached_on<F, C>(
        &mut self,
        def: &'static SpanDef,
        fut: F,
        classify: C,
    ) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + Send + 'static,
        C: FnOnce(&T) -> DetachedCompletion + Send + 'static;
}

impl<T: Send + 'static> JoinSetExt<T> for JoinSet<T> {
    fn spawn_joined_on<F>(&mut self, fut: F) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + Send + 'static,
    {
        self.spawn(fut.instrument(Span::current()).with_current_subscriber())
    }

    fn spawn_joined_on_handle<F>(&mut self, handle: &Handle, fut: F) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + Send + 'static,
    {
        self.spawn_on(
            fut.instrument(Span::current()).with_current_subscriber(),
            handle,
        )
    }

    fn spawn_local_joined_on<F>(&mut self, fut: F) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + 'static,
    {
        self.spawn_local(fut.instrument(Span::current()).with_current_subscriber())
    }

    fn spawn_local_joined_on_set<F>(
        &mut self,
        local_set: &LocalSet,
        fut: F,
    ) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + 'static,
    {
        self.spawn_local_on(
            fut.instrument(Span::current()).with_current_subscriber(),
            local_set,
        )
    }

    fn spawn_joined_blocking_on<F>(&mut self, work: F) -> tokio::task::AbortHandle
    where
        F: FnOnce() -> T + Send + 'static,
    {
        let span = Span::current();
        self.spawn_blocking(move || in_span_scope(span, work))
    }

    fn spawn_detached_on<F, C>(
        &mut self,
        def: &'static SpanDef,
        fut: F,
        classify: C,
    ) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + Send + 'static,
        C: FnOnce(&T) -> DetachedCompletion + Send + 'static,
    {
        let parent = Span::current().context().span().span_context().clone();
        let guard = DetachedGuard::new(def, &[], &parent);
        self.spawn(run_detached(guard, fut, classify).with_current_subscriber())
    }
}

fn in_span_scope<F, R>(span: Span, work: F) -> R
where
    F: FnOnce() -> R,
{
    let dispatch = Span::with_subscriber(&span, |(_, dispatch)| dispatch.clone());
    if let Some(dispatch) = dispatch {
        tracing::dispatcher::with_default(&dispatch, || span.in_scope(work))
    } else {
        work()
    }
}

#[cfg(test)]
mod tests;

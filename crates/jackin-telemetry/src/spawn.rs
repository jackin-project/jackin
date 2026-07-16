// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{future::Future, thread};

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
    let _counter_result = crate::counter(&crate::metric::PREWARM_JOBS).add(1, &attrs);

    tokio::spawn(
        async move {
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
            let Ok(consumer) = root_operation(&crate::operation::PREWARM_ATTEMPT, &attrs) else {
                return fut.await;
            };
            if let Some(producer_context) = producer_context.as_ref()
                && producer_context.is_valid()
            {
                let _link_result = consumer.link(producer_context);
            }
            let guard = DetachedGuard(Some(consumer));
            let output = fut.instrument(guard.span()).await;
            guard.complete(classify(&output));
            output
        }
        .with_current_subscriber(),
    )
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
    let parent = Span::current().context().span().span_context().clone();
    let guard = DetachedGuard::new(def, &[], &parent);
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
    thread::spawn(move || in_span_scope(span, work))
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
    thread::Builder::new()
        .name(name)
        .spawn(move || in_span_scope(span, work))
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
    let parent = Span::current().context().span().span_context().clone();
    let guard = DetachedGuard::new(def, &[], &parent);
    thread::Builder::new().name(name).spawn(move || {
        let result = in_span_scope(guard.span(), work);
        guard.complete(classify(&result));
        result
    })
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

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{future::Future, thread};

use opentelemetry::trace::TraceContextExt as _;
use tokio::{
    runtime::Handle,
    task::{JoinHandle, JoinSet, LocalSet},
};
use tracing::{Instrument as _, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

use crate::operation::{SpanDef, root_operation};

#[derive(Clone, Copy, Debug)]
pub struct DetachedCompletion {
    pub outcome: crate::schema::enums::OutcomeValue,
    pub error_type: Option<crate::schema::enums::ErrorType>,
}

pub fn spawn_joined<F>(fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(fut.instrument(Span::current()))
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
    handle.spawn(fut.instrument(Span::current()))
}

pub fn spawn_local_joined<F>(fut: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    tokio::task::spawn_local(fut.instrument(Span::current()))
}

pub fn spawn_local_joined_on<F>(local_set: &LocalSet, fut: F) -> JoinHandle<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    local_set.spawn_local(fut.instrument(Span::current()))
}

pub fn spawn_detached<F>(def: &'static SpanDef, fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    tokio::spawn(async move {
        let Ok(guard) = root_operation(def, &[]) else {
            return fut.await;
        };
        if parent.is_valid() {
            let _link_result = guard.link(&parent);
        }
        let output = fut.instrument(guard.span().clone()).await;
        guard.complete(crate::schema::enums::OutcomeValue::Success, None);
        output
    })
}

pub fn spawn_detached_with_completion<F>(def: &'static SpanDef, fut: F) -> JoinHandle<()>
where
    F: Future<Output = DetachedCompletion> + Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    tokio::spawn(async move {
        let Ok(guard) = root_operation(def, &[]) else {
            let _ = fut.await;
            return;
        };
        if parent.is_valid() {
            let _link_result = guard.link(&parent);
        }
        let completion = fut.instrument(guard.span().clone()).await;
        guard.complete(completion.outcome, completion.error_type);
    })
}

/// Schedule detached prewarm work as a PRODUCER decision linked to one
/// CONSUMER attempt with a shared durable job identity.
pub fn spawn_prewarm_job<F>(
    job_type: crate::schema::enums::JobType,
    fut: F,
) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
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

    tokio::spawn(async move {
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
        let output = fut.instrument(consumer.span().clone()).await;
        consumer.complete(crate::schema::enums::OutcomeValue::Success, None);
        output
    })
}

pub fn joined_blocking<F, R>(work: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let span = Span::current();
    tokio::task::spawn_blocking(move || span.in_scope(work))
}

pub fn joined_blocking_on<F, R>(handle: &Handle, work: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let span = Span::current();
    handle.spawn_blocking(move || span.in_scope(work))
}

pub fn detached_blocking<F, R>(def: &'static SpanDef, work: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    tokio::task::spawn_blocking(move || {
        let Ok(guard) = root_operation(def, &[]) else {
            return work();
        };
        if parent.is_valid() {
            let _link_result = guard.link(&parent);
        }
        let result = guard.span().in_scope(work);
        guard.complete(crate::schema::enums::OutcomeValue::Success, None);
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
    thread::spawn(move || span.in_scope(work))
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
        .spawn(move || span.in_scope(work))
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
    scope.spawn(move || span.in_scope(work))
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
        .spawn_scoped(scope, move || span.in_scope(work))
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

pub fn thread_detached<F, R>(def: &'static SpanDef, work: F) -> thread::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    thread::spawn(move || {
        let Ok(guard) = root_operation(def, &[]) else {
            return work();
        };
        if parent.is_valid() {
            let _link_result = guard.link(&parent);
        }
        let result = guard.span().in_scope(work);
        guard.complete(crate::schema::enums::OutcomeValue::Success, None);
        result
    })
}

pub fn thread_detached_named<F, R>(
    name: String,
    def: &'static SpanDef,
    work: F,
) -> std::io::Result<thread::JoinHandle<R>>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    thread::Builder::new().name(name).spawn(move || {
        let Ok(guard) = root_operation(def, &[]) else {
            return work();
        };
        if parent.is_valid() {
            let _link_result = guard.link(&parent);
        }
        let result = guard.span().in_scope(work);
        guard.complete(crate::schema::enums::OutcomeValue::Success, None);
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
}

impl<T: Send + 'static> JoinSetExt<T> for JoinSet<T> {
    fn spawn_joined_on<F>(&mut self, fut: F) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + Send + 'static,
    {
        self.spawn(fut.instrument(Span::current()))
    }

    fn spawn_joined_on_handle<F>(&mut self, handle: &Handle, fut: F) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + Send + 'static,
    {
        self.spawn_on(fut.instrument(Span::current()), handle)
    }

    fn spawn_local_joined_on<F>(&mut self, fut: F) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + 'static,
    {
        self.spawn_local(fut.instrument(Span::current()))
    }

    fn spawn_local_joined_on_set<F>(
        &mut self,
        local_set: &LocalSet,
        fut: F,
    ) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + 'static,
    {
        self.spawn_local_on(fut.instrument(Span::current()), local_set)
    }

    fn spawn_joined_blocking_on<F>(&mut self, work: F) -> tokio::task::AbortHandle
    where
        F: FnOnce() -> T + Send + 'static,
    {
        let span = Span::current();
        self.spawn_blocking(move || span.in_scope(work))
    }
}

#[cfg(test)]
mod tests;

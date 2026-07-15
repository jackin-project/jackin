// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{future::Future, thread};

use opentelemetry::trace::TraceContextExt as _;
use tokio::task::{JoinHandle, JoinSet};
use tracing::{Instrument as _, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

use crate::operation::{SpanDef, operation_root};

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
    spawn_joined(fut)
}

pub fn spawn_stream<F>(_name: &'static str, fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    spawn_joined(fut)
}

pub fn spawn_detached<F>(def: &'static SpanDef, fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    tokio::spawn(async move {
        let guard = operation_root(def, &[]).expect("registered detached span definition");
        if parent.is_valid() {
            guard
                .link(&parent)
                .expect("first detached link is within limit");
        }
        let output = fut.instrument(guard.span().clone()).await;
        guard.complete(crate::schema::enums::OutcomeValue::Success, None);
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

pub fn detached_blocking<F, R>(def: &'static SpanDef, work: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let parent = Span::current().context().span().span_context().clone();
    tokio::task::spawn_blocking(move || {
        let guard = operation_root(def, &[]).expect("registered detached span definition");
        if parent.is_valid() {
            guard
                .link(&parent)
                .expect("first detached link is within limit");
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
    joined_blocking(work)
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
    thread_joined(work)
}

pub trait JoinSetExt<T: Send + 'static> {
    fn spawn_joined_on<F>(&mut self, fut: F) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + Send + 'static;
}

impl<T: Send + 'static> JoinSetExt<T> for JoinSet<T> {
    fn spawn_joined_on<F>(&mut self, fut: F) -> tokio::task::AbortHandle
    where
        F: Future<Output = T> + Send + 'static,
    {
        self.spawn(fut.instrument(Span::current()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

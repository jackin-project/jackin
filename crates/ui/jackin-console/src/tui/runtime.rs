// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;

#[derive(Debug)]
pub struct ConsoleFramePacer {
    presenter: termrock::runtime::Presenter,
    clock: termrock::runtime::FrameClock,
}

impl ConsoleFramePacer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            presenter: termrock::runtime::Presenter::new()
                .min_draw_interval(std::time::Duration::ZERO),
            clock: termrock::runtime::FrameClock::start(),
        }
    }

    pub fn tick(&mut self) -> termrock::runtime::FrameTick {
        self.clock.tick()
    }

    pub const fn mark_dirty(&mut self) {
        self.presenter.mark_dirty();
    }

    pub fn should_draw(&self, now: termrock::runtime::Instant) -> bool {
        self.presenter.should_draw(now)
    }

    pub const fn begin_draw(&mut self, now: termrock::runtime::Instant) {
        self.presenter.begin_draw(now);
    }

    pub const fn end_draw(&mut self, now: termrock::runtime::Instant) {
        self.presenter.end_draw(now);
    }
}

impl Default for ConsoleFramePacer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionPoll<T> {
    Ready(T),
    Pending,
    Closed,
}

#[derive(Debug)]
enum BlockingSource<T> {
    Receiver(tokio::sync::oneshot::Receiver<T>),
    Ready(termrock::runtime::ReadySubscription<T>),
}

#[derive(Debug)]
pub struct BlockingSubscription<T>(BlockingSource<T>);

impl<T> BlockingSubscription<T> {
    pub fn poll_next(&mut self) -> SubscriptionPoll<T> {
        match &mut self.0 {
            BlockingSource::Receiver(rx) => match rx.try_recv() {
                Ok(value) => SubscriptionPoll::Ready(value),
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => SubscriptionPoll::Pending,
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => SubscriptionPoll::Closed,
            },
            BlockingSource::Ready(ready) => match ready.poll_next() {
                termrock::runtime::ReadySubscriptionPoll::Ready(value) => {
                    SubscriptionPoll::Ready(value)
                }
                _ => SubscriptionPoll::Closed,
            },
        }
    }
}

pub fn ready_blocking_subscription<T>(value: T) -> BlockingSubscription<T> {
    BlockingSubscription(BlockingSource::Ready(
        termrock::runtime::ready_subscription(value),
    ))
}

pub fn spawn_blocking_subscription<T, F>(worker: F) -> BlockingSubscription<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    spawn_named_blocking_subscription("jackin-console-blocking-subscription", worker)
}

pub fn spawn_named_blocking_subscription<T, F>(
    name: impl Into<String>,
    worker: F,
) -> BlockingSubscription<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    let run = move || drop(tx.send(worker()));
    let name = name.into();
    if tokio::runtime::Handle::try_current().is_ok() {
        drop(jackin_telemetry::spawn::joined_blocking(run));
    } else {
        drop(jackin_telemetry::spawn::thread_joined_named(name, run));
    }
    BlockingSubscription(BlockingSource::Receiver(rx))
}

pub fn spawn_named_async_subscription<T, F>(
    name: impl Into<String>,
    future: F,
) -> BlockingSubscription<T>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    let run = async move { drop(tx.send(future.await)) };
    let name = name.into();
    if tokio::runtime::Handle::try_current().is_ok() {
        drop(jackin_telemetry::spawn::spawn_joined(run));
    } else {
        drop(jackin_telemetry::spawn::thread_joined_named(
            name,
            move || {
                if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    runtime.block_on(run);
                }
            },
        ));
    }
    BlockingSubscription(BlockingSource::Receiver(rx))
}

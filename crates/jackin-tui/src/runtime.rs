// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Consumer-owned Tokio subscription helpers pending relocation to the console crate.

use std::future::Future;
use termrock::runtime::{Subscription, SubscriptionPoll};

#[derive(Debug)]
pub struct BlockingSubscription<T>(tokio::sync::oneshot::Receiver<T>);

impl<T> Subscription for BlockingSubscription<T> {
    type Output = T;
    fn poll_next(&mut self) -> SubscriptionPoll<T> {
        match self.0.try_recv() {
            Ok(value) => SubscriptionPoll::Ready(value),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => SubscriptionPoll::Pending,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => SubscriptionPoll::Closed,
        }
    }
}

pub fn ready_blocking_subscription<T: Send + 'static>(value: T) -> BlockingSubscription<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    drop(tx.send(value));
    BlockingSubscription(rx)
}

pub fn spawn_blocking_subscription<T, F>(worker: F) -> BlockingSubscription<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    spawn_named_blocking_subscription("jackin-tui-blocking-subscription", worker)
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
    let run = move || {
        drop(tx.send(worker()));
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn_blocking(run);
    } else {
        drop(std::thread::Builder::new().name(name.into()).spawn(run));
    }
    BlockingSubscription(rx)
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
    let run = async move {
        drop(tx.send(future.await));
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(run);
    } else {
        drop(
            std::thread::Builder::new()
                .name(name.into())
                .spawn(move || {
                    if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        runtime.block_on(run);
                    }
                }),
        );
    }
    BlockingSubscription(rx)
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared jackin❯ application-adapter wiring for the console TUI.
//!
//! The shared TEA `Component<Ev, Msg>` and `View<Model>` contracts live in
//! `jackin_tui::runtime`. This module is the console's implementation of
//! those traits over its model (`ConsoleState`) and the existing render
//! function (`crate::tui::view::render`). The trait impls are thin
//! delegations that satisfy the shared contract at the type level. The
//! existing event loop in `crates/jackin/src/console/adapter/run.rs` owns
//! scheduling and dispatches rendering through this adapter.

#[derive(Debug)]
pub struct ConsoleViewContext<'a> {
    pub config: &'a jackin_config::AppConfig,
    pub cwd: &'a std::path::Path,
}

#[derive(Debug)]
pub struct ConsoleView<'a> {
    pub context: ConsoleViewContext<'a>,
}

impl jackin_tui::runtime::View<crate::tui::console::ConsoleState> for ConsoleView<'_> {
    fn render(
        &self,
        model: &crate::tui::console::ConsoleState,
        frame: &mut ratatui::Frame<'_>,
        area: ratatui::layout::Rect,
    ) {
        let crate::tui::console::ConsoleStage::Manager(ms) = &model.stage;
        crate::tui::view::render(frame, area, ms, self.context.config, self.context.cwd);
    }
}

use jackin_tui::runtime::{Subscription, SubscriptionPoll};
use std::future::Future;

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
    BlockingSubscription(rx)
}

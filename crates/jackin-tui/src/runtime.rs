// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared runtime contracts for Ratatui-style update loops.

use std::future::Future;

/// Whether applying a message changed visible state and should schedule a draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum Dirty {
    Clean,
    Redraw,
}

impl Dirty {
    #[must_use]
    pub const fn is_dirty(self) -> bool {
        matches!(self, Self::Redraw)
    }

    pub const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Redraw, _) | (_, Self::Redraw) => Self::Redraw,
            (Self::Clean, Self::Clean) => Self::Clean,
        }
    }
}

/// Marker effect type for update loops that do not produce side effects yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoEffect {}

/// Non-blocking result of checking an external event source.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub enum SubscriptionPoll<T> {
    Pending,
    Ready(T),
    Closed,
}

impl<T> SubscriptionPoll<T> {
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }
}

/// Pull-style subscription polled by a TUI runtime.
///
/// Implementations must never block. Long-running work belongs on a task or
/// worker thread; `poll_next` only drains a completed result into the update
/// loop.
pub trait Subscription {
    type Output;

    fn poll_next(&mut self) -> SubscriptionPoll<Self::Output>;
}

pub type BlockingSubscription<T> = tokio::sync::oneshot::Receiver<T>;

/// Wrap an already-computed value as a ready `BlockingSubscription`.
///
/// Avoids requiring callers to import `tokio` directly when they need to
/// short-circuit the load path (e.g. cache hit in op-picker load).
pub fn ready_blocking_subscription<T: Send + 'static>(value: T) -> BlockingSubscription<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    drop(tx.send(value));
    rx
}

/// Spawn blocking work and expose its single result as a subscription.
///
/// This keeps the TUI-side contract consistent: callers start slow work as an
/// effect, then poll the returned receiver without blocking the update loop.
pub fn spawn_blocking_subscription<T, F>(worker: F) -> BlockingSubscription<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    spawn_named_blocking_subscription("jackin-tui-blocking-subscription", worker)
}

/// Spawn blocking work on Tokio when available, otherwise fall back to a named
/// OS thread.
///
/// Some component tests and teardown helpers run outside a Tokio runtime. The
/// fallback keeps those paths on the same subscription contract instead of
/// reintroducing caller-owned channel/thread plumbing.
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
    rx
}

/// Spawn async work and expose its single result as a subscription.
///
/// TUI component tests can run outside a Tokio runtime, so this mirrors the
/// blocking helper's fallback by creating a named OS thread with a small runtime.
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
                    match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(runtime) => runtime.block_on(run),
                        Err(error) => {
                            drop(error);
                        }
                    }
                }),
        );
    }
    rx
}

impl<T> Subscription for tokio::sync::oneshot::Receiver<T> {
    type Output = T;

    fn poll_next(&mut self) -> SubscriptionPoll<Self::Output> {
        match self.try_recv() {
            Ok(value) => SubscriptionPoll::Ready(value),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => SubscriptionPoll::Pending,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => SubscriptionPoll::Closed,
        }
    }
}

impl<T> Subscription for tokio::sync::mpsc::UnboundedReceiver<T> {
    type Output = T;

    fn poll_next(&mut self) -> SubscriptionPoll<Self::Output> {
        match self.try_recv() {
            Ok(value) => SubscriptionPoll::Ready(value),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => SubscriptionPoll::Pending,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => SubscriptionPoll::Closed,
        }
    }
}

impl<T> Subscription for std::sync::mpsc::Receiver<T> {
    type Output = T;

    fn poll_next(&mut self) -> SubscriptionPoll<Self::Output> {
        match self.try_recv() {
            Ok(value) => SubscriptionPoll::Ready(value),
            Err(std::sync::mpsc::TryRecvError::Empty) => SubscriptionPoll::Pending,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => SubscriptionPoll::Closed,
        }
    }
}

/// Result of applying one message to a TUI model.
///
/// `dirty` tells the runtime whether to redraw. `effects` carries typed
/// side-effect requests for the app runtime to execute outside the update
/// function.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct UpdateResult<E = NoEffect> {
    dirty: Dirty,
    effects: Vec<E>,
}

impl<E> UpdateResult<E> {
    pub const fn clean() -> Self {
        Self {
            dirty: Dirty::Clean,
            effects: Vec::new(),
        }
    }

    pub const fn redraw() -> Self {
        Self {
            dirty: Dirty::Redraw,
            effects: Vec::new(),
        }
    }

    pub fn with_effect(effect: E) -> Self {
        Self {
            dirty: Dirty::Redraw,
            effects: vec![effect],
        }
    }

    pub const fn dirty(&self) -> Dirty {
        self.dirty
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty.is_dirty()
    }

    #[must_use]
    pub fn effects(&self) -> &[E] {
        &self.effects
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.dirty = self.dirty.merge(other.dirty);
        self.effects.extend(other.effects);
        self
    }
}

/// TEA component contract: translates raw terminal input events into typed
/// messages for the app's central `update` function.
///
/// `Ev` is the surface-specific event type (e.g. [`crossterm::event::Event`]
/// for keyboard/mouse-driven surfaces; raw bytes or a decoded action type for
/// the in-container multiplexer). `Msg` is the domain message the central
/// `update` consumes. Components maintain their own sub-state (e.g. cursor
/// position, focus) but must not mutate app model state; they only produce
/// messages.
///
/// # Contract
///
/// - `handle_event` is non-blocking and must not perform I/O.
/// - Returning `None` means the event was not consumed; the runtime may
///   offer the event to the next component in the chain.
/// - Returning `Some(msg)` means the event was consumed; the runtime calls
///   the central `update` with `msg`.
pub trait Component<Ev, Msg> {
    fn handle_event(&mut self, event: &Ev) -> Option<Msg>;
}

/// TEA view contract: renders an app model into one rectangular region of a
/// ratatui [`ratatui::Frame`].
///
/// Implementations are observational: they read `model` but must not mutate
/// it. All visible output (widget painting, cursor positioning, scroll
/// indicators) flows through the `frame` and `area` arguments. `View` never
/// drives subscriptions or spawns work.
pub trait View<Model> {
    fn render(&self, model: &Model, frame: &mut ratatui::Frame<'_>, area: ratatui::layout::Rect);
}

/// Drive one composed Ratatui frame: paint `view` over `model` into `area`,
/// then hand the same in-progress frame to `overlay` so a caller can layer
/// frame-scoped extras (modal backdrops, chrome, debug bars) that are not
/// part of the view's own model. Both steps run inside one
/// [`ratatui::Terminal::draw`] call so Ratatui diffs a single coherent
/// buffer per tick — splitting them across two `draw` calls would double
/// the diff/flush cost and let the terminal observe an intermediate frame.
///
/// Spike (plan 053): prototype for the shared TUI runtime half-layer
/// dispatch question. Any out-of-frame post-pass (e.g. raw OSC 8 overlay
/// bytes written to stdout after the frame buffer settles) stays the
/// caller's responsibility — it is not part of this helper's contract.
pub fn drive_frame<'a, B, M, V, F>(
    terminal: &'a mut ratatui::Terminal<B>,
    view: &V,
    model: &M,
    area: ratatui::layout::Rect,
    overlay: F,
) -> Result<ratatui::CompletedFrame<'a>, B::Error>
where
    B: ratatui::backend::Backend,
    V: View<M>,
    F: FnOnce(&mut ratatui::Frame<'_>),
{
    terminal.draw(|frame| {
        view.render(model, frame, area);
        overlay(frame);
    })
}

struct ClosureView<F>(std::cell::RefCell<Option<F>>);

impl<F> View<()> for ClosureView<F>
where
    F: FnOnce(&mut ratatui::Frame<'_>),
{
    fn render(&self, _model: &(), frame: &mut ratatui::Frame<'_>, _area: ratatui::layout::Rect) {
        if let Some(render) = self.0.borrow_mut().take() {
            render(frame);
        }
    }
}

/// Drive one frame for an existing frame-scoped widget renderer.
///
/// This adapter lets modal and prompt loops converge on [`drive_frame`]
/// without inventing a persistent model/view type for a short-lived widget.
pub fn drive_render<B, F>(
    terminal: &mut ratatui::Terminal<B>,
    render: F,
) -> Result<ratatui::CompletedFrame<'_>, B::Error>
where
    B: ratatui::backend::Backend,
    F: FnOnce(&mut ratatui::Frame<'_>),
{
    let area = terminal.size()?;
    let view = ClosureView(std::cell::RefCell::new(Some(render)));
    drive_frame(terminal, &view, &(), area.into(), |_| {})
}

#[cfg(test)]
mod tests;

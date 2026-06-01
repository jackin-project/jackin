//! Shared runtime contracts for Ratatui-style update loops.

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
        let _ = tx.send(worker());
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn_blocking(run);
    } else {
        let _ = std::thread::Builder::new().name(name.into()).spawn(run);
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

    #[must_use]
    pub fn into_effects(self) -> Vec<E> {
        self.effects
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.dirty = self.dirty.merge(other.dirty);
        self.effects.extend(other.effects);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Subscription, SubscriptionPoll, spawn_blocking_subscription,
        spawn_named_blocking_subscription,
    };

    #[test]
    fn oneshot_subscription_reports_ready_value() {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        tx.send(7).expect("receiver should be live");

        assert_eq!(rx.poll_next(), SubscriptionPoll::Ready(7));
    }

    #[test]
    fn oneshot_subscription_reports_pending_then_closed() {
        let (tx, mut rx) = tokio::sync::oneshot::channel::<u8>();

        assert_eq!(rx.poll_next(), SubscriptionPoll::Pending);

        drop(tx);

        assert_eq!(rx.poll_next(), SubscriptionPoll::Closed);
    }

    #[test]
    fn mpsc_subscription_reports_ready_values() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tx.send(7).expect("receiver should be live");
        tx.send(8).expect("receiver should be live");

        assert_eq!(rx.poll_next(), SubscriptionPoll::Ready(7));
        assert_eq!(rx.poll_next(), SubscriptionPoll::Ready(8));
        assert_eq!(rx.poll_next(), SubscriptionPoll::Pending);
    }

    #[test]
    fn mpsc_subscription_reports_closed() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<u8>();

        assert_eq!(rx.poll_next(), SubscriptionPoll::Pending);

        drop(tx);

        assert_eq!(rx.poll_next(), SubscriptionPoll::Closed);
    }

    #[test]
    fn std_mpsc_subscription_reports_ready_value() {
        let (tx, mut rx) = std::sync::mpsc::channel();
        tx.send(7).expect("receiver should be live");

        assert_eq!(rx.poll_next(), SubscriptionPoll::Ready(7));
        assert_eq!(rx.poll_next(), SubscriptionPoll::Pending);
    }

    #[test]
    fn std_mpsc_subscription_reports_closed() {
        let (tx, mut rx) = std::sync::mpsc::channel::<u8>();

        assert_eq!(rx.poll_next(), SubscriptionPoll::Pending);

        drop(tx);

        assert_eq!(rx.poll_next(), SubscriptionPoll::Closed);
    }

    #[test]
    fn spawn_blocking_subscription_reports_worker_result() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");

        runtime.block_on(async {
            let rx = spawn_blocking_subscription(|| 7);

            assert_eq!(rx.await.expect("worker should send result"), 7);
        });
    }

    #[test]
    fn named_blocking_subscription_reports_worker_result_without_runtime() {
        let mut rx = spawn_named_blocking_subscription("jackin-tui-test-worker", || 7);

        for _ in 0..100 {
            match rx.poll_next() {
                SubscriptionPoll::Ready(value) => {
                    assert_eq!(value, 7);
                    return;
                }
                SubscriptionPoll::Pending => {
                    std::thread::sleep(std::time::Duration::from_millis(1))
                }
                SubscriptionPoll::Closed => panic!("worker closed before sending result"),
            }
        }

        panic!("worker did not finish");
    }
}

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
    use super::{Subscription, SubscriptionPoll};

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
}

use crossterm::event::KeyEvent;
use termrock::runtime::{ReadySubscription, ReadySubscriptionPoll, ready_subscription};
use termrock::widgets::{TextInputOutcome, TextInputState as TermRockTextInputState};

/// The product's single shared modal-outcome contract: the console's
/// retired facade twin was folded into this enum (plan 006). Do not
/// reorder or rename variants — every consumer matches on this set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalOutcome<T> {
    Continue,
    Commit(T),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInputState<'a> {
    label: &'a str,
    inner: TermRockTextInputState,
}

impl<'a> TextInputState<'a> {
    pub fn new(label: &'a str, value: impl Into<String>) -> Self {
        Self {
            label,
            inner: TermRockTextInputState::new(value),
        }
    }
    pub const fn label(&self) -> &str {
        self.label
    }
    pub fn value(&self) -> &str {
        self.inner.value()
    }
    pub fn trimmed_value(&self) -> String {
        self.inner.value().trim().to_owned()
    }
    pub const fn termrock_state(&self) -> &TermRockTextInputState {
        &self.inner
    }
    pub fn handle_key(&mut self, key: KeyEvent) -> ModalOutcome<String> {
        match self.inner.handle_key(key.into()) {
            TextInputOutcome::Submitted(_) => ModalOutcome::Commit(self.trimmed_value()),
            TextInputOutcome::Cancelled => ModalOutcome::Cancel,
            TextInputOutcome::Ignored | TextInputOutcome::Changed => ModalOutcome::Continue,
            _ => ModalOutcome::Continue,
        }
    }
}

/// Product-owned tri-state load poll: upstream `ReadySubscriptionPoll`
/// has no `Pending` by design (it is a fused one-shot), so the worker
/// arm's in-flight state is expressed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadPoll<T> {
    Ready(T),
    Pending,
    Closed,
}

/// Spawn/delivery mechanics of a one-shot worker load: the result of a
/// blocking `op` call is delivered over a oneshot from a join-tracked
/// thread (or blocking tokio task).
#[derive(Debug)]
pub struct WorkerSubscription<T>(tokio::sync::oneshot::Receiver<T>);

/// Single in-flight load slot: a cache hit rides the upstream fused
/// one-shot [`ReadySubscription`]; a miss rides a [`WorkerSubscription`].
/// Both arms translate into the one [`LoadPoll`] tri-state so `poll_load`
/// stays the single completion path (cache hits and misses alike).
#[derive(Debug)]
pub enum LoadSubscription<T> {
    Ready(ReadySubscription<T>),
    Worker(WorkerSubscription<T>),
}

impl<T> LoadSubscription<T> {
    pub fn poll_next(&mut self) -> LoadPoll<T> {
        match self {
            Self::Ready(ready) => match ready.poll_next() {
                ReadySubscriptionPoll::Ready(value) => LoadPoll::Ready(value),
                _ => LoadPoll::Closed,
            },
            Self::Worker(worker) => match worker.0.try_recv() {
                Ok(value) => LoadPoll::Ready(value),
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => LoadPoll::Pending,
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => LoadPoll::Closed,
            },
        }
    }
}

pub fn ready_load_subscription<T>(value: T) -> LoadSubscription<T> {
    LoadSubscription::Ready(ready_subscription(value))
}
pub fn spawn_named_worker_subscription<T, F>(
    name: impl Into<String>,
    worker: F,
) -> LoadSubscription<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    let run = move || {
        drop(tx.send(worker()));
    };
    let name = name.into();
    if tokio::runtime::Handle::try_current().is_ok() {
        drop(jackin_telemetry::spawn::joined_blocking(run));
    } else {
        drop(jackin_telemetry::spawn::thread_joined_named(name, run));
    }
    LoadSubscription::Worker(WorkerSubscription(rx))
}

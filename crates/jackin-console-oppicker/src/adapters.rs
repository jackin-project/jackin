use crossterm::event::KeyEvent;
use termrock::runtime::{Subscription, SubscriptionPoll};
use termrock::widgets::{TextInputOutcome, TextInputState as TermRockTextInputState};

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

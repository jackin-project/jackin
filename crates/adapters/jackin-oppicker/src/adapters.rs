use crossterm::event::KeyEvent;
use termrock::runtime::{ReadySubscription, ReadySubscriptionPoll, ready_subscription};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadPoll<T> {
    Ready(T),
    Pending,
    Closed,
}

#[derive(Debug)]
pub struct WorkerSubscription<T>(tokio::sync::oneshot::Receiver<T>);

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

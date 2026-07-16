use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use termrock::runtime::{Subscription, SubscriptionPoll};
use termrock::widgets::{EditAction, TextInputState as TermRockTextInputState};

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
        match key.code {
            KeyCode::Esc => return ModalOutcome::Cancel,
            KeyCode::Enter if !self.trimmed_value().is_empty() => {
                return ModalOutcome::Commit(self.trimmed_value());
            }
            KeyCode::Backspace => self.inner.apply(EditAction::Backspace),
            KeyCode::Delete => self.inner.apply(EditAction::Delete),
            KeyCode::Left => self.inner.apply(EditAction::MoveLeft),
            KeyCode::Right => self.inner.apply(EditAction::MoveRight),
            KeyCode::Home => self.inner.apply(EditAction::Home),
            KeyCode::End => self.inner.apply(EditAction::End),
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.inner.apply(EditAction::Insert(c));
            }
            _ => {}
        }
        ModalOutcome::Continue
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
    let name = name.into();
    if tokio::runtime::Handle::try_current().is_ok() {
        drop(jackin_telemetry::spawn::joined_blocking(run));
    } else {
        drop(jackin_telemetry::spawn::thread_stream_named(name, run));
    }
    BlockingSubscription(rx)
}

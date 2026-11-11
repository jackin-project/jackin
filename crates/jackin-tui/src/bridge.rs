// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jackin_console::app::Remote;
use jackin_console::world::{Msg, Snapshot};
use junie_tui::runtime::ExternalSender;
use tokio::runtime::Handle;
use tokio::sync::watch;

#[derive(Clone, Debug)]
pub(crate) struct InFlight {
    count: Arc<watch::Sender<usize>>,
}

impl InFlight {
    pub(crate) fn new() -> InFlight {
        let (count, _) = watch::channel(0);
        InFlight {
            count: Arc::new(count),
        }
    }

    pub(crate) fn count(&self) -> usize {
        *self.count.borrow()
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<usize> {
        self.count.subscribe()
    }

    fn start(&self) -> Ticket {
        self.count.send_modify(|n| *n += 1);
        Ticket {
            count: Arc::clone(&self.count),
        }
    }
}

impl Default for InFlight {
    fn default() -> InFlight {
        InFlight::new()
    }
}

struct Ticket {
    count: Arc<watch::Sender<usize>>,
}

impl Drop for Ticket {
    fn drop(&mut self) {
        self.count.send_modify(|n| *n = n.saturating_sub(1));
    }
}

#[derive(Debug)]
pub(crate) struct Settled {
    reply: Msg,
    reload: Option<Box<Snapshot>>,
}

impl Settled {
    pub(crate) fn reverted(reply: Msg, saved: Option<Snapshot>) -> Settled {
        Settled {
            reply,
            reload: saved.map(Box::new),
        }
    }
}

impl From<Msg> for Settled {
    fn from(reply: Msg) -> Settled {
        Settled {
            reply,
            reload: None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Bridge {
    sender: ExternalSender,
    inflight: InFlight,
    detached: Arc<AtomicUsize>,
    handle: Handle,
}

struct Detached(Arc<AtomicUsize>);

impl Detached {
    fn start(count: &Arc<AtomicUsize>) -> Detached {
        count.fetch_add(1, Ordering::SeqCst);
        Detached(Arc::clone(count))
    }
}

impl Drop for Detached {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Bridge {
    pub(crate) fn new(sender: ExternalSender, inflight: InFlight, handle: Handle) -> Bridge {
        Bridge {
            sender,
            inflight,
            detached: Arc::new(AtomicUsize::new(0)),
            handle,
        }
    }

    pub(crate) fn has_detached_work(&self) -> bool {
        self.detached.load(Ordering::SeqCst) > 0
    }

    pub(crate) fn detach(
        &self,
        work: impl Future<Output = ()> + Send + 'static,
        failed: impl FnOnce() + Send + 'static,
    ) {
        let held = Detached::start(&self.detached);
        let task = self.handle.spawn(async move {
            let _held = held;
            work.await;
        });
        self.handle.spawn(async move {
            if task.await.is_err() {
                failed();
            }
        });
    }

    pub(crate) fn send(&self, msg: Msg) {
        self.sender.send(Remote::Msg(msg));
    }

    pub(crate) fn handle(&self) -> &Handle {
        &self.handle
    }

    pub(crate) fn spawn<T: Into<Settled> + Send + 'static>(
        &self,
        failure: Msg,
        work: impl Future<Output = T> + Send + 'static,
    ) {
        let ticket = self.inflight.start();
        let sender = self.sender.clone();
        let task = self.handle.spawn(work);
        self.handle.spawn(async move {
            let settled: Settled = match task.await {
                Ok(done) => done.into(),
                Err(_) => failure.into(),
            };
            sender.send(Remote::Msg(settled.reply));
            if let Some(saved) = settled.reload {
                sender.send(Remote::Reloaded(saved));
            }
            drop(ticket);
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use junie_tui::runtime::{ExternalReceiver, external_channel};

    use super::*;
    use crate::crashed;

    const WAIT: Duration = Duration::from_secs(5);

    fn crash() -> Msg {
        panic!("the work crashed")
    }

    async fn reply(receiver: &ExternalReceiver) -> Option<Remote> {
        tokio::time::timeout(WAIT, async {
            loop {
                if let Some(found) = receiver
                    .drain()
                    .iter()
                    .find_map(|external| external.downcast_ref::<Remote>().cloned())
                {
                    return found;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .ok()
    }

    #[tokio::test]
    async fn a_panic_in_spawned_work_sends_a_reply_and_frees_the_count() {
        let (sender, receiver) = external_channel();
        let inflight = InFlight::new();
        let bridge = Bridge::new(sender, inflight.clone(), Handle::current());
        bridge.spawn(crashed::refresh(3), async { crash() });
        let got = reply(&receiver).await;
        assert!(
            matches!(
                &got,
                Some(Remote::Msg(Msg::Refreshed {
                    ticket: 3,
                    result: Err(_)
                }))
            ),
            "{got:?}"
        );
        let mut count = inflight.subscribe();
        let freed = tokio::time::timeout(WAIT, count.wait_for(|n| *n == 0)).await;
        assert!(freed.is_ok(), "the in-flight count stayed above zero");
    }

    #[tokio::test]
    async fn a_panic_in_detached_work_runs_the_failure_and_a_clean_finish_does_not() {
        let (sender, _receiver) = external_channel();
        let bridge = Bridge::new(sender, InFlight::new(), Handle::current());
        let (told, heard) = tokio::sync::oneshot::channel();
        bridge.detach(
            async {
                crash();
            },
            move || assert!(told.send(()).is_ok()),
        );
        assert!(
            tokio::time::timeout(WAIT, heard)
                .await
                .is_ok_and(|sent| sent.is_ok())
        );
        let (told, mut heard) = tokio::sync::oneshot::channel();
        bridge.detach(async {}, move || assert!(told.send(()).is_ok()));
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }
        assert!(heard.try_recv().is_err());
    }
}

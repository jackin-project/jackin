// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::sync::Arc;

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
    handle: Handle,
}

impl Bridge {
    pub(crate) fn new(sender: ExternalSender, inflight: InFlight, handle: Handle) -> Bridge {
        Bridge {
            sender,
            inflight,
            handle,
        }
    }

    pub(crate) fn detach(&self, work: impl Future<Output = ()> + Send + 'static) {
        self.handle.spawn(work);
    }

    pub(crate) fn send(&self, msg: Msg) {
        self.sender.send(Remote::Msg(msg));
    }

    pub(crate) fn handle(&self) -> &Handle {
        &self.handle
    }

    pub(crate) fn spawn<T: Into<Settled>>(&self, work: impl Future<Output = T> + Send + 'static) {
        let ticket = self.inflight.start();
        let sender = self.sender.clone();
        self.handle.spawn(async move {
            let settled = work.await.into();
            sender.send(Remote::Msg(settled.reply));
            if let Some(saved) = settled.reload {
                sender.send(Remote::Reloaded(saved));
            }
            drop(ticket);
        });
    }
}

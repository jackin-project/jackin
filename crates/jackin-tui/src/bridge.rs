// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::sync::Arc;

use jackin_console::app::Remote;
use jackin_console::world::Msg;
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

    pub(crate) fn spawn(&self, work: impl Future<Output = Msg> + Send + 'static) {
        let ticket = self.inflight.start();
        let sender = self.sender.clone();
        self.handle.spawn(async move {
            let msg = work.await;
            sender.send(Remote::Msg(msg));
            drop(ticket);
        });
    }
}

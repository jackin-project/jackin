// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::task::{Context, Poll, Waker};

use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub struct Signals {
    interrupt: Signal,
    terminate: Signal,
    hangup: Signal,
}

impl Signals {
    pub fn install() -> io::Result<Signals> {
        Ok(Signals {
            interrupt: signal(SignalKind::interrupt())?,
            terminate: signal(SignalKind::terminate())?,
            hangup: signal(SignalKind::hangup())?,
        })
    }

    pub(crate) async fn next(&mut self) -> Option<()> {
        tokio::select! {
            received = self.interrupt.recv() => received,
            received = self.terminate.recv() => received,
            received = self.hangup.recv() => received,
        }
    }

    pub(crate) fn discard_pending(&mut self) {
        let mut context = Context::from_waker(Waker::noop());
        for signal in [&mut self.interrupt, &mut self.terminate, &mut self.hangup] {
            while let Poll::Ready(Some(())) = signal.poll_recv(&mut context) {}
        }
    }

    pub async fn interrupted(mut self, cancel: &CancellationToken) {
        if self.next().await.is_some() {
            cancel.cancel();
            if self.next().await.is_some() {
                return;
            }
        }
        std::future::pending::<()>().await;
    }
}

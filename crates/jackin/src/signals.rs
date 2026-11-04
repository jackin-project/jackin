// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io;

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

    async fn next(&mut self) -> Option<()> {
        tokio::select! {
            received = self.interrupt.recv() => received,
            received = self.terminate.recv() => received,
            received = self.hangup.recv() => received,
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

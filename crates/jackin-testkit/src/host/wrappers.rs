// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use jackin_core::{
    LineSink, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, SecretOutput,
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

fn carries(spec: &ProcessSpec, word: &str) -> bool {
    spec.args.iter().any(|arg| arg == word)
}

#[derive(Debug)]
pub struct CancelsOn {
    inner: Arc<dyn ProcessRunner>,
    word: &'static str,
    token: CancellationToken,
}

impl CancelsOn {
    pub fn new<R: ProcessRunner + 'static>(
        inner: &Arc<R>,
        word: &'static str,
        token: &CancellationToken,
    ) -> CancelsOn {
        CancelsOn {
            inner: Arc::<R>::clone(inner),
            word,
            token: token.clone(),
        }
    }
}

#[async_trait]
impl ProcessRunner for CancelsOn {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        if carries(spec, self.word) {
            self.token.cancel();
        }
        self.inner.run(spec, lines, cancel).await
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        self.inner.run_secret(spec, cancel).await
    }
}

#[derive(Debug)]
pub struct PausesOn {
    inner: Arc<dyn ProcessRunner>,
    word: &'static str,
    taken: AtomicBool,
    reached: watch::Sender<bool>,
    released: watch::Sender<bool>,
}

impl PausesOn {
    pub fn new<R: ProcessRunner + 'static>(inner: &Arc<R>, word: &'static str) -> PausesOn {
        PausesOn {
            inner: Arc::<R>::clone(inner),
            word,
            taken: AtomicBool::new(false),
            reached: watch::channel(false).0,
            released: watch::channel(false).0,
        }
    }

    pub async fn reached(&self) {
        drop(self.reached.subscribe().wait_for(|reached| *reached).await);
    }

    pub fn release(&self) {
        self.released.send_replace(true);
    }
}

#[async_trait]
impl ProcessRunner for PausesOn {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        if carries(spec, self.word) && !self.taken.swap(true, Ordering::SeqCst) {
            let mut released = self.released.subscribe();
            self.reached.send_replace(true);
            tokio::select! {
                biased;
                () = cancel.cancelled() => {}
                woken = released.wait_for(|released| *released) => drop(woken),
            }
        }
        self.inner.run(spec, lines, cancel).await
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        self.inner.run_secret(spec, cancel).await
    }
}

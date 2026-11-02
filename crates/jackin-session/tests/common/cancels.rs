// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use jackin_core::{
    LineSink, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, SecretOutput,
};
use jackin_testkit::TestRunner;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub(crate) struct CancelsOn {
    inner: Arc<TestRunner>,
    word: &'static str,
    token: CancellationToken,
}

impl CancelsOn {
    pub(crate) fn new(
        inner: &Arc<TestRunner>,
        word: &'static str,
        token: &CancellationToken,
    ) -> CancelsOn {
        CancelsOn {
            inner: Arc::clone(inner),
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
        if spec.args.iter().any(|arg| arg == self.word) {
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

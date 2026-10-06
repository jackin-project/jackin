// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Cloneable serialized wrapper for the mutable `CommandRunner` seam.
//!
//! Launch still has runner-bound work that must preserve one command stream
//! for debug logs and tests, but future dependency-graph branches need owned
//! runner handles. This adapter gives each branch a cloneable handle while a
//! Tokio mutex keeps the underlying runner serialized.

use jackin_core::{CommandRunner, RunOptions};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug)]
pub(crate) struct SharedCommandRunner<R> {
    inner: Arc<Mutex<R>>,
}

impl<R> SharedCommandRunner<R> {
    #[expect(
        dead_code,
        reason = "dependency-graph launch branches will construct shared handles as call sites migrate"
    )]
    pub(crate) fn new(runner: R) -> Self {
        Self {
            inner: Arc::new(Mutex::new(runner)),
        }
    }
}

impl<R> Clone for SharedCommandRunner<R> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<R> CommandRunner for SharedCommandRunner<R>
where
    R: CommandRunner,
{
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()> {
        self.inner.lock().await.run(program, args, cwd, opts).await
    }

    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.inner.lock().await.capture(program, args, cwd).await
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.inner
            .lock()
            .await
            .capture_secret(program, args, cwd)
            .await
    }
}

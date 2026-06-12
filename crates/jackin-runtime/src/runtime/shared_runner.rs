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
#[allow(
    dead_code,
    reason = "dependency-graph launch branches will construct shared handles as call sites migrate"
)]
pub(crate) struct SharedCommandRunner<R> {
    inner: Arc<Mutex<R>>,
}

impl<R> SharedCommandRunner<R> {
    #[allow(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct QueueRunner {
        recorded: Vec<String>,
        outputs: VecDeque<String>,
    }

    impl CommandRunner for QueueRunner {
        async fn run(
            &mut self,
            program: &str,
            args: &[&str],
            _cwd: Option<&Path>,
            _opts: &RunOptions,
        ) -> anyhow::Result<()> {
            self.recorded
                .push(format!("run:{program} {}", args.join(" ")));
            Ok(())
        }

        async fn capture(
            &mut self,
            program: &str,
            args: &[&str],
            _cwd: Option<&Path>,
        ) -> anyhow::Result<String> {
            self.recorded
                .push(format!("capture:{program} {}", args.join(" ")));
            Ok(self.outputs.pop_front().unwrap_or_default())
        }

        async fn capture_secret(
            &mut self,
            program: &str,
            args: &[&str],
            _cwd: Option<&Path>,
        ) -> anyhow::Result<String> {
            self.recorded
                .push(format!("secret:{program} {}", args.join(" ")));
            Ok(self.outputs.pop_front().unwrap_or_default())
        }
    }

    #[tokio::test]
    async fn cloned_handles_share_one_serialized_runner() {
        let runner = QueueRunner {
            outputs: VecDeque::from(["one".to_owned(), "two".to_owned()]),
            ..Default::default()
        };
        let shared = SharedCommandRunner::new(runner);
        let mut first = shared.clone();
        let mut second = shared.clone();

        let (first_result, second_result) = tokio::join!(
            first.capture("git", &["rev-parse", "HEAD"], None),
            second.capture_secret("gh", &["auth", "token"], None)
        );

        assert_eq!(first_result.unwrap(), "one");
        assert_eq!(second_result.unwrap(), "two");
        let guard = shared.inner.lock().await;
        assert_eq!(
            guard.recorded,
            vec!["capture:git rev-parse HEAD", "secret:gh auth token"]
        );
    }
}

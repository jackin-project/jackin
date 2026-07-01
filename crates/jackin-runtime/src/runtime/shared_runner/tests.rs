#[cfg(test)]
use super::*;
use std::collections::VecDeque;

#[derive(Default)]
struct QueueRunner {
    recorded: Vec<String>,
    outputs: VecDeque<String>,

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

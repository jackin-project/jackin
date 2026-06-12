//! Capture host git user.name/email for in-container git defaults.
//!
//! All reads are best-effort: missing git config or id failures produce empty
//! strings rather than hard errors.

use jackin_core::CommandRunner;

pub(super) struct GitIdentity {
    pub(super) user_name: String,
    pub(super) user_email: String,
}

pub(super) async fn try_capture(
    runner: &mut impl CommandRunner,
    program: &str,
    args: &[&str],
) -> Option<String> {
    runner
        .capture(program, args, None)
        .await
        .ok()
        .filter(|s| !s.is_empty())
}

pub(super) async fn load_git_identity(runner: &mut impl CommandRunner) -> GitIdentity {
    jackin_diagnostics::active_timing_started("identity", "git_user_name", None);
    let user_name = try_capture(runner, "git", &["config", "user.name"])
        .await
        .unwrap_or_default();
    jackin_diagnostics::active_timing_done(
        "identity",
        "git_user_name",
        Some(if user_name.is_empty() {
            "missing"
        } else {
            "present"
        }),
    );

    jackin_diagnostics::active_timing_started("identity", "git_user_email", None);
    let user_email = try_capture(runner, "git", &["config", "user.email"])
        .await
        .unwrap_or_default();
    jackin_diagnostics::active_timing_done(
        "identity",
        "git_user_email",
        Some(if user_email.is_empty() {
            "missing"
        } else {
            "present"
        }),
    );

    GitIdentity {
        user_name,
        user_email,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jackin_core::RunOptions;
    use std::collections::VecDeque;

    struct QueueRunner {
        outputs: VecDeque<String>,
    }

    impl CommandRunner for QueueRunner {
        async fn run(
            &mut self,
            _program: &str,
            _args: &[&str],
            _cwd: Option<&std::path::Path>,
            _opts: &RunOptions,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn capture(
            &mut self,
            _program: &str,
            _args: &[&str],
            _cwd: Option<&std::path::Path>,
        ) -> anyhow::Result<String> {
            Ok(self.outputs.pop_front().unwrap_or_default())
        }

        async fn capture_secret(
            &mut self,
            program: &str,
            args: &[&str],
            cwd: Option<&std::path::Path>,
        ) -> anyhow::Result<String> {
            self.capture(program, args, cwd).await
        }
    }

    #[tokio::test]
    async fn load_git_identity_records_nested_timings() {
        let temp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();
        let mut runner = QueueRunner {
            outputs: VecDeque::from(["Agent Name".to_owned(), "agent@example.com".to_owned()]),
        };

        let identity = load_git_identity(&mut runner).await;

        assert_eq!(identity.user_name, "Agent Name");
        assert_eq!(identity.user_email, "agent@example.com");
        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(jsonl.contains("\"stage\":\"identity\""), "{jsonl}");
        assert!(jsonl.contains("git_user_name"), "{jsonl}");
        assert!(jsonl.contains("git_user_email"), "{jsonl}");
        assert!(jsonl.contains("present"), "{jsonl}");
    }
}

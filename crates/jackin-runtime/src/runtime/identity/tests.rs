#[cfg(test)]
use super::*;
use jackin_core::RunOptions;
use std::collections::VecDeque;

struct QueueRunner {
    outputs: VecDeque<String>,

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

    #[cfg(unix)]
    #[test]
    fn host_run_as_user_targets_host_uid_and_gid() {
        let user = host_run_as_user().expect("unix host has a run-as user");
        let (uid, gid) = user.split_once(':').expect("run-as user has uid:gid");
        let uid: u32 = uid.parse().expect("uid parses");
        let gid: u32 = gid.parse().expect("gid parses");
        assert_eq!(uid, host_uid().expect("unix host has a uid"));
        assert_eq!(gid, host_gid().expect("unix host has a gid"));
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

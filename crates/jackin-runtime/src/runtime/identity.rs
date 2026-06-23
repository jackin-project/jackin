//! Capture host git user.name/email for in-container git defaults, and the
//! host operator's UID for the runtime `docker run --user` mapping.
//!
//! All reads are best-effort: missing git config or id failures produce empty
//! strings rather than hard errors.

use jackin_core::CommandRunner;

/// `--user` value that runs the in-container process as the host operator's
/// UID with primary group 0 (`<uid>:0`).
///
/// The host operator is this jackin process's own effective UID — it creates
/// every bind-mount source under `~/.jackin`, so matching it makes host-owned
/// mounts transparently readable/writable inside the container with no chown
/// and no UID baked into the (shareable) image. Returns `None` on non-unix
/// hosts, where no mapping applies.
///
/// Group 0 (not the host GID) is deliberate: the image's `/home/agent` tree is
/// normalized to group 0 with group==owner permissions at build time (the
/// `OpenShift` arbitrary-UID pattern), so a process in group 0 can use the
/// image-baked home regardless of which UID it runs as. A matching
/// `agent` passwd entry for the host UID is supplied at runtime via
/// `libnss-extrausers` so `getpwuid`/`$HOME` resolve correctly.
///
/// Security tradeoff (the agent is untrusted code): primary group 0 makes the
/// in-container process a member of the `root` group, so it can read/write any
/// path in the image that is group-`root` and group-readable/writable. This is
/// acceptable only because the in-container privilege boundary is the container
/// itself, not the `agent` user — the agent is already free to run arbitrary
/// code as its own UID, and the construct image ships no group-0-writable path
/// that grants escalation *out* of the container (no setuid-root binary is
/// left group-0-writable, and the docker socket is never mounted into a role
/// container). The host side is protected separately: every bind-mount source
/// is owned by this UID, so group 0 grants nothing beyond what owner already
/// does. If the construct image ever gains a group-0-writable sensitive path,
/// revisit this with a supplementary group instead of primary GID 0.
#[cfg(unix)]
pub(crate) fn host_run_as_user() -> Option<String> {
    Some(format!("{}:0", nix::unistd::geteuid().as_raw()))
}

#[cfg(not(unix))]
pub(crate) fn host_run_as_user() -> Option<String> {
    None
}

/// The host operator's effective UID, used to build the runtime
/// `libnss-extrausers` passwd line. `None` on non-unix hosts.
#[cfg(unix)]
pub(crate) fn host_uid() -> Option<u32> {
    Some(nix::unistd::geteuid().as_raw())
}

#[cfg(not(unix))]
pub(crate) fn host_uid() -> Option<u32> {
    None
}

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

    #[cfg(unix)]
    #[test]
    fn host_run_as_user_targets_host_uid_group_zero() {
        let user = host_run_as_user().expect("unix host has a run-as user");
        assert!(user.ends_with(":0"), "expected group 0, got {user}");
        let uid: u32 = user
            .strip_suffix(":0")
            .and_then(|u| u.parse().ok())
            .expect("uid prefix parses");
        assert_eq!(uid, host_uid().expect("unix host has a uid"));
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

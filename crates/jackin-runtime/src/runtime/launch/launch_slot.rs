// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Container name slot management: claim, lock, and credential verification.

use fs4::FileExt;

use super::super::attach::{ContainerState, docker_unavailable_msg};
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_core::WorkspaceName;
use jackin_docker::docker_client::DockerApi;

/// Cap retries so a filesystem without working flock (NFS without
/// lockd, exotic mount) surfaces as an actionable error instead of an
/// unbounded spin. 64 attempts at 40 bits of ID entropy is enough that
/// a genuine collision-space exhaustion is astronomically unlikely;
/// hitting the cap signals an environmental fault, not bad luck.
const CLAIM_MAX_ATTEMPTS: u32 = 64;

/// Claim a unique DNS-safe container name by acquiring an exclusive lock file.
/// Random IDs avoid deterministic role slots; the lock still protects the
/// vanishingly small random-collision window and concurrent launch races.
pub(crate) async fn claim_container_name(
    paths: &JackinPaths,
    workspace_name: Option<&WorkspaceName>,
    selector: &RoleSelector,
    docker: &impl DockerApi,
) -> anyhow::Result<(String, std::fs::File)> {
    let mut last_lock_err: Option<std::io::Error> = None;
    let mut occupied_attempts = 0u32;

    for _ in 0..CLAIM_MAX_ATTEMPTS {
        let name = crate::instance::new_container_name(workspace_name, selector);

        let inspection = docker.inspect_container_by_name(&name).await;
        let slot_free = match inspection.state {
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } => match inspection.handle {
                Some(handle) => match docker.remove_container_by_id(&handle).await {
                    Ok(()) => true,
                    Err(error) => {
                        return Err(error.context(format!(
                            "removing stale container `{name}` before reclaiming its name"
                        )));
                    }
                },
                None => anyhow::bail!(
                    "cannot reclaim stale container `{name}` because Docker returned no immutable ID"
                ),
            },
            ContainerState::Running
            | ContainerState::Paused
            | ContainerState::Restarting
            | ContainerState::Stopped { .. }
            | ContainerState::Created
            | ContainerState::Removing
            | ContainerState::Dead => false,
            ContainerState::NotFound => true,
            ContainerState::InspectUnavailable(reason) => {
                let _error = jackin_telemetry::record_error(
                    jackin_telemetry::schema::enums::ErrorType::LaunchFailed,
                );
                jackin_diagnostics::emit_operator_notice(
                    "container name availability inspection failed",
                );
                anyhow::bail!(
                    "{}",
                    docker_unavailable_msg(&format!("claim container name `{name}`"), &reason,)
                );
            }
        };

        if slot_free {
            match acquire_name_lock(paths, &name).await {
                Ok(lock_file) => return Ok((name, lock_file)),
                Err(lock) => {
                    let _warning = jackin_telemetry::record_retry_scheduled();
                    last_lock_err = Some(lock);
                }
            }
        } else {
            occupied_attempts += 1;
        }
    }

    let lock_summary = match last_lock_err {
        Some(lock) => format!("name lock acquisition failed ({lock})"),
        None if occupied_attempts == CLAIM_MAX_ATTEMPTS => {
            "all candidates already exist in Docker".to_owned()
        }
        None => "no lock attempted".to_owned(),
    };
    anyhow::bail!(
        "exhausted {CLAIM_MAX_ATTEMPTS} attempts to claim a unique container name ({lock_summary})"
    );
}

pub(crate) async fn claim_known_container_name(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<(String, std::fs::File)> {
    match docker.inspect_container_by_name(container_name).await.state {
        ContainerState::NotFound => {}
        ContainerState::Running
        | ContainerState::Paused
        | ContainerState::Restarting
        | ContainerState::Stopped { .. }
        | ContainerState::Created
        | ContainerState::Removing
        | ContainerState::Dead => {
            anyhow::bail!(
                "cannot restore `{container_name}` because its Docker container already exists; use `jackin hardline {container_name}`"
            );
        }
        ContainerState::InspectUnavailable(reason) => {
            anyhow::bail!(
                "{}",
                docker_unavailable_msg(&format!("restore `{container_name}`"), &reason,)
            );
        }
    }

    match acquire_name_lock(paths, container_name).await {
        Ok(lock_file) => Ok((container_name.to_owned(), lock_file)),
        Err(lock) => anyhow::bail!(
            "cannot restore `{container_name}` because name lock acquisition failed ({lock})"
        ),
    }
}

async fn acquire_name_lock(paths: &JackinPaths, name: &str) -> std::io::Result<std::fs::File> {
    let paths = paths.clone();
    let name = name.to_owned();
    jackin_telemetry::spawn::joined_blocking(move || try_acquire_name_lock(&paths, &name))
        .await
        .map_err(std::io::Error::other)?
}

/// Acquire a persistent name inode outside every prunable runtime root.
/// Closing the handle releases ownership; maintenance never removes its inode.
fn try_acquire_name_lock(paths: &JackinPaths, name: &str) -> std::io::Result<std::fs::File> {
    let lock_file = crate::runtime::coordination::open_lock(paths, &format!("name-{name}"))?;
    FileExt::try_lock(&lock_file).map_err(std::io::Error::from)?;
    Ok(lock_file)
}

/// Token-mode pre-flight for the `[github]` axis: `GH_TOKEN` must
/// resolve to a non-empty value before launch proceeds. The other
/// modes (`Sync` / `Ignore`) have nothing to verify here.
///
/// Extracted from `load_role_with` so the bail-message shape and
/// trigger condition can be unit-pinned without orchestrating the
/// full launch flow.
pub(crate) fn verify_github_token_present(
    github_mode: jackin_config::GithubAuthMode,
    resolved_token: Option<&str>,
    workspace: &WorkspaceName,
    role: &str,
) -> anyhow::Result<()> {
    if !matches!(github_mode, jackin_config::GithubAuthMode::Token) {
        return Ok(());
    }
    if resolved_token.is_some_and(|s| !s.is_empty()) {
        return Ok(());
    }
    anyhow::bail!(
        "auth_forward = \"token\" for [github] in workspace '{workspace}' role '{role}' \
         requires GH_TOKEN to resolve to a non-empty value, but it is unset.\n\n\
         Fix one of:\n  \
         - Add GH_TOKEN under [github.env] (or [workspaces.{workspace}.github.env], or \
         [workspaces.{workspace}.roles.{role}.github.env]).\n  \
         - Or change the mode: set auth_forward = \"sync\" or \"ignore\"."
    );
}

/// Resolve the `[…github.env]` declarations through the same
/// `op://` + host-env dispatch as regular operator env. Honors the
/// `op_runner` / `host_env` test seams on `LoadOptions` so tests stay
/// hermetic.
pub(crate) fn resolve_github_env_map(
    declarations: &std::collections::BTreeMap<String, jackin_core::EnvValue>,
    opts: &super::LoadOptions,
) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    let mut resolved: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    if declarations.is_empty() {
        return Ok(resolved);
    }
    let default_runner = jackin_env::OpCli::new_launch_env();
    let runner: &dyn jackin_env::OpRunner = opts.op_runner.as_deref().unwrap_or(&default_runner);
    let host_env_fn = |name: &str| -> Result<String, std::env::VarError> {
        opts.host_env.as_ref().map_or_else(
            || std::env::var(name),
            |map| map.get(name).cloned().ok_or(std::env::VarError::NotPresent),
        )
    };
    let mut errors: Vec<String> = Vec::new();
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(declarations.len());
        for (key, value) in declarations {
            let host_env_fn = &host_env_fn;
            handles.push(jackin_telemetry::spawn::thread_scoped_joined(
                scope,
                move || {
                    let timing_name = format!("github_env:{key}");
                    let value_kind = github_env_value_kind(value);
                    jackin_diagnostics::active_timing_started(
                        jackin_diagnostics::DiagnosticStage::Credentials,
                        &timing_name,
                        Some(value_kind),
                    );
                    let result =
                        jackin_env::resolve_env_value("[github.env]", key, value, runner, |name| {
                            host_env_fn(name)
                        });
                    match result {
                        Ok(value) => {
                            jackin_diagnostics::active_timing_done(
                                jackin_diagnostics::DiagnosticStage::Credentials,
                                &timing_name,
                                Some(value_kind),
                            );
                            (key.clone(), Ok(value))
                        }
                        Err(error) => {
                            jackin_diagnostics::active_timing_done(
                                jackin_diagnostics::DiagnosticStage::Credentials,
                                &timing_name,
                                Some("error"),
                            );
                            (key.clone(), Err(error))
                        }
                    }
                },
            ));
        }
        for handle in handles {
            match handle
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            {
                (key, Ok(value)) => {
                    resolved.insert(key, value);
                }
                (_, Err(error)) => errors.push(format!("  - {error}")),
            }
        }
    });
    if !errors.is_empty() {
        anyhow::bail!(
            "github env resolution failed for {} var(s):\n{}",
            errors.len(),
            errors.join("\n")
        );
    }
    Ok(resolved)
}

pub(crate) fn github_env_declarations_for_mode(
    declarations: &std::collections::BTreeMap<String, jackin_core::EnvValue>,
    mode: jackin_config::GithubAuthMode,
) -> std::collections::BTreeMap<String, jackin_core::EnvValue> {
    if matches!(mode, jackin_config::GithubAuthMode::Ignore) {
        return std::collections::BTreeMap::new();
    }

    [
        jackin_core::GH_TOKEN_ENV_NAME,
        jackin_core::GH_HOST_ENV_NAME,
        jackin_core::GH_ENTERPRISE_TOKEN_ENV_NAME,
    ]
    .into_iter()
    .filter_map(|key| {
        declarations
            .get(key)
            .cloned()
            .map(|value| (key.to_owned(), value))
    })
    .collect()
}

fn github_env_value_kind(value: &jackin_core::EnvValue) -> &'static str {
    match value {
        jackin_core::EnvValue::OpRef(_) => "op",
        jackin_core::EnvValue::Plain(value)
            if value
                .strip_prefix("${")
                .is_some_and(|rest| rest.ends_with('}'))
                || value.strip_prefix('$').is_some_and(|rest| !rest.is_empty()) =>
        {
            "host"
        }
        jackin_core::EnvValue::Plain(_) => "literal",
        jackin_core::EnvValue::Extended(e)
            if e.value
                .strip_prefix("${")
                .is_some_and(|rest| rest.ends_with('}'))
                || e.value
                    .strip_prefix('$')
                    .is_some_and(|rest| !rest.is_empty()) =>
        {
            "host"
        }
        jackin_core::EnvValue::Extended(_) => "literal",
    }
}

#[cfg(all(test, unix))]
mod lock_tests {
    #![expect(
        clippy::disallowed_methods,
        reason = "isolated filesystem and child-process fixtures run only on test threads"
    )]

    use super::try_acquire_name_lock;
    use std::io::{BufRead as _, Write as _};
    use std::os::unix::fs::MetadataExt as _;
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc::{Receiver, channel};
    use std::time::{Duration, Instant};

    const CHILD_ROOT: &str = "JACKIN_TEST_NAME_LOCK_ROOT";
    const CHILD_TEST: &str = "runtime::launch::launch_slot::lock_tests::name_lock_child";
    const REPORT: &str = "NAME_LOCK_REPORT ";
    const NAME: &str = "same-container-name";

    struct Contender {
        child: Child,
        reports: Receiver<String>,
    }

    impl Drop for Contender {
        fn drop(&mut self) {
            drop(self.child.kill());
            drop(self.child.wait());
        }
    }

    #[derive(Debug)]
    struct Observation {
        pid: u32,
        acquired: bool,
        inode: (u64, u64),
    }

    fn read_child_reports(
        stdout: std::process::ChildStdout,
        sender: std::sync::mpsc::Sender<String>,
    ) {
        for line in std::io::BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            // libtest may prepend the helper test's name to its first line.
            if let Some((_, report)) = line.split_once(REPORT)
                && sender.send(report.to_owned()).is_err()
            {
                break;
            }
        }
    }

    impl Contender {
        #[expect(
            clippy::unwrap_used,
            reason = "child fixture setup must fail the parent test on process or pipe errors"
        )]
        fn spawn(root: &std::path::Path) -> Self {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
                .env(CHILD_ROOT, root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            let stdout = child.stdout.take().unwrap();
            let (sender, reports) = channel();
            std::thread::spawn(move || read_child_reports(stdout, sender));
            Self { child, reports }
        }

        #[expect(
            clippy::unwrap_used,
            clippy::expect_used,
            clippy::panic,
            reason = "child protocol errors and missed deadlines must fail the parent test"
        )]
        fn attempt(&mut self) -> Observation {
            let stdin = self.child.stdin.as_mut().unwrap();
            writeln!(stdin, "attempt").unwrap();
            stdin.flush().unwrap();
            let report = self
                .reports
                .recv_timeout(Duration::from_secs(10))
                .expect("child must report an actual lock attempt before the deadline");
            let fields: Vec<_> = report.split_whitespace().collect();
            assert_eq!(fields.len(), 4, "invalid child report: {report}");
            let observation = Observation {
                pid: fields[0].parse().unwrap(),
                acquired: match fields[1] {
                    "acquired" => true,
                    "blocked" => false,
                    other => panic!("invalid lock status: {other}"),
                },
                inode: (fields[2].parse().unwrap(), fields[3].parse().unwrap()),
            };
            assert_eq!(
                observation.pid,
                self.child.id(),
                "report must identify its OS process"
            );
            observation
        }

        #[expect(
            clippy::unwrap_used,
            reason = "child wait errors must fail the parent test"
        )]
        fn exit(&mut self) {
            drop(self.child.stdin.take());
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    assert!(status.success(), "name-lock helper failed: {status}");
                    return;
                }
                assert!(Instant::now() < deadline, "name-lock helper failed to exit");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    /// Helper only; acceptance requires the parent three-process scenario.
    #[test]
    #[ignore = "child fixture invoked explicitly by the parent scenario"]
    fn name_lock_child() {
        let root = std::env::var_os(CHILD_ROOT).expect("child fixture requires isolated root");
        let paths = jackin_core::JackinPaths::for_tests(std::path::Path::new(&root));
        let lock_path = crate::runtime::coordination::root(&paths)
            .unwrap()
            .join(format!("name-{NAME}.lock"));
        let mut held = None;
        for command in std::io::stdin().lock().lines() {
            assert_eq!(command.unwrap(), "attempt");
            assert!(
                held.is_none(),
                "an owner must not attempt a second acquisition"
            );
            let acquired = match try_acquire_name_lock(&paths, NAME) {
                Ok(lock) => {
                    held = Some(lock);
                    true
                }
                Err(error) => {
                    assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
                    false
                }
            };
            let metadata = held
                .as_ref()
                .map(|lock| lock.metadata().unwrap())
                .or_else(|| std::fs::metadata(&lock_path).ok());
            let (device, inode) = metadata.map_or((0, 0), |meta| (meta.dev(), meta.ino()));
            println!(
                "{REPORT}{} {} {device} {inode}",
                std::process::id(),
                if acquired { "acquired" } else { "blocked" },
            );
            std::io::stdout().flush().unwrap();
        }
        // Dropping ownership must release flock without deleting the pathname.
        drop(held);
    }

    #[test]
    fn three_process_contenders_preserve_lock_inode_until_and_after_owner_exit() {
        let temp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(temp.path());
        let lock_path = crate::runtime::coordination::root(&paths)
            .unwrap()
            .join(format!("name-{NAME}.lock"));
        let mut owner = Contender::spawn(temp.path());
        let first = owner.attempt();
        assert!(first.acquired, "first process must own the slot");
        let mut failed = Contender::spawn(temp.path());
        let second = failed.attempt();
        assert!(
            !second.acquired,
            "second process must encounter owner contention"
        );
        let mut third = Contender::spawn(temp.path());
        let blocked = third.attempt();
        failed.exit();
        assert!(
            !blocked.acquired,
            "third process acquired a replacement inode while the first owner still held flock: {first:?}, {second:?}, {blocked:?}"
        );
        assert_ne!(first.pid, second.pid);
        assert_ne!(first.pid, blocked.pid);
        assert_ne!(second.pid, blocked.pid);
        assert_eq!(
            second.inode, first.inode,
            "failed contender must preserve owner inode"
        );
        assert_eq!(
            blocked.inode, first.inode,
            "third contender must observe owner inode"
        );
        let metadata = std::fs::metadata(&lock_path).unwrap();
        assert_eq!((metadata.dev(), metadata.ino()), first.inode);
        owner.exit();
        let reclaimed = third.attempt();
        assert!(reclaimed.acquired, "owner exit must release flock");
        assert_eq!(reclaimed.pid, blocked.pid);
        assert_eq!(
            reclaimed.inode, first.inode,
            "reclaim must lock the original inode"
        );
        third.exit();
        let metadata = std::fs::metadata(lock_path).expect("owner drop must preserve lock file");
        assert_eq!((metadata.dev(), metadata.ino()), first.inode);
    }

    #[tokio::test]
    async fn prune_paths_preserve_three_process_name_lock_ownership() {
        use jackin_test_support::{FakeDockerClient, FakeRunner};
        for operation in ["container", "instances", "all-instances", "home"] {
            let temp = tempfile::tempdir().unwrap();
            let paths = jackin_core::JackinPaths::for_tests(temp.path());
            std::fs::create_dir_all(&paths.data_dir).unwrap();
            let lock_path = crate::runtime::coordination::root(&paths)
                .unwrap()
                .join(format!("name-{NAME}.lock"));
            let mut owner = Contender::spawn(temp.path());
            let first = owner.attempt();
            assert!(first.acquired);
            let mut failed = Contender::spawn(temp.path());
            let second = failed.attempt();
            assert!(!second.acquired);
            let docker = FakeDockerClient::default();
            let mut runner = FakeRunner::default();
            match operation {
                "container" => crate::runtime::cleanup::purge_container_state(
                    &paths,
                    NAME,
                    &docker,
                    &mut runner,
                )
                .await
                .unwrap(),
                "instances" => {
                    crate::runtime::cleanup::prune_instances(&paths, &docker, &mut runner)
                        .await
                        .unwrap();
                }
                "all-instances" => {
                    crate::runtime::cleanup::prune_all_instances(&paths, &docker, &mut runner)
                        .await
                        .unwrap();
                }
                "home" => crate::runtime::cleanup::prune_jackin_home(&paths).unwrap(),
                _ => unreachable!(),
            }
            let mut third = Contender::spawn(temp.path());
            let blocked = third.attempt();
            failed.exit();
            assert!(
                !blocked.acquired,
                "{operation} split the inode while owner still held flock"
            );
            assert_ne!(first.pid, second.pid);
            assert_ne!(first.pid, blocked.pid);
            assert_ne!(second.pid, blocked.pid);
            assert_eq!(second.inode, first.inode);
            assert_eq!(
                blocked.inode, first.inode,
                "{operation} changed coordination identity"
            );
            owner.exit();
            let reclaimed = third.attempt();
            assert!(reclaimed.acquired);
            assert_eq!(reclaimed.inode, first.inode);
            third.exit();
            let metadata = std::fs::metadata(lock_path).unwrap();
            assert_eq!((metadata.dev(), metadata.ino()), first.inode);
        }
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod process;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jackin_workflow_spec::model::TestBundleFile;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{EvidenceRecord, RunStore, WorkflowError, WorkflowService};

pub const MAX_TEST_OUTPUT_BYTES: usize = 64 * 1024;

pub const MAX_REVIEW_BYTES: u64 = 64 * 1024;

pub const MAX_FEEDBACK_BYTES: usize = 4 * 1024;

pub const MAX_TEST_EXCERPT_BYTES: usize = 1_500;

pub const MAX_REVIEW_CORRECTIONS: u32 = 1;

pub const TEST_SANDBOX_IMAGE: &str =
    "python@sha256:1b668429b3511ab407d8e00648891631b0b1a4d7e15e3ca70f38ab5b91ad4ab4";
const TEST_COMMAND_WRAPPER: &str = "import os,signal,subprocess,sys,time\ndeadline=float(sys.argv[1])\nargv=sys.argv[2:]\nremaining=deadline-time.time()\nif remaining <= 0: sys.exit(124)\ntry: child=subprocess.Popen(argv,start_new_session=True)\nexcept OSError as error:\n print(error,file=sys.stderr); sys.exit(125)\ntry: code=child.wait(timeout=remaining)\nexcept subprocess.TimeoutExpired:\n try: os.killpg(child.pid,signal.SIGKILL)\n except ProcessLookupError: pass\n child.wait(); sys.exit(124)\nsys.exit(code)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestPolicy {
    argv: Vec<String>,
    timeout_secs: u64,
    policy_hash: String,
}

impl TestPolicy {
    pub fn freeze(argv: Vec<String>, timeout_secs: u64) -> Result<Self, WorkflowError> {
        if argv.is_empty() || argv.iter().any(|part| part.trim().is_empty()) {
            return Err(WorkflowError::invalid(
                "frozen test policy needs a non-empty exact argv".to_owned(),
            ));
        }
        if !(1..=3_600).contains(&timeout_secs) {
            return Err(WorkflowError::invalid(
                "frozen test policy timeout must be within 1..=3600 seconds".to_owned(),
            ));
        }
        let mut hasher = Sha256::new();
        for part in &argv {
            hasher.update(part.as_bytes());
            hasher.update([0]);
        }
        hasher.update(TEST_SANDBOX_IMAGE.as_bytes());
        hasher.update(b"\0sandbox-wrapper-v1;watchdog-grace=5s;network=none;readonly-root;cap-drop=all;memory=536870912;cpus=1;pids=64;trusted-ro;candidate-ro;tmpfs=/tmp:64m,/build:256m;env=PYTHONDONTWRITEBYTECODE=1\0");
        hasher.update(timeout_secs.to_be_bytes());
        Ok(Self {
            argv,
            timeout_secs,
            policy_hash: hex::encode(hasher.finalize()),
        })
    }

    #[must_use]
    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    #[must_use]
    pub fn program(&self) -> &str {
        &self.argv[0]
    }

    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.argv[1..]
    }

    #[must_use]
    pub const fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }

    #[must_use]
    pub fn policy_hash(&self) -> &str {
        &self.policy_hash
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoAgentCredentials {
    _private: (),
}

impl NoAgentCredentials {
    pub const NONE: Self = Self { _private: () };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestWorkerSpec {
    worker_id: String,
    candidate_id: String,
    commit: String,
    tree: String,
    policy_hash: String,
    timeout_secs: u64,
    credentials: NoAgentCredentials,
}

impl TestWorkerSpec {
    pub fn new(
        run_id: &str,
        round: u32,
        candidate_id: &str,
        commit: &str,
        tree: &str,
        policy: &TestPolicy,
    ) -> Result<Self, WorkflowError> {
        for (label, value) in [
            ("run", run_id),
            ("candidate", candidate_id),
            ("commit", commit),
            ("tree", tree),
        ] {
            if value.trim().is_empty()
                || value
                    .chars()
                    .any(|ch| ch.is_whitespace() || ch == '/' || ch == '\0')
            {
                return Err(WorkflowError::invalid(format!(
                    "test worker {label} must be a single non-empty path-safe token"
                )));
            }
        }
        Ok(Self {
            worker_id: format!("test-{run_id}-r{round}-{candidate_id}"),
            candidate_id: candidate_id.to_owned(),
            commit: commit.to_owned(),
            tree: tree.to_owned(),
            policy_hash: policy.policy_hash().to_owned(),
            timeout_secs: policy.timeout().as_secs(),
            credentials: NoAgentCredentials::NONE,
        })
    }

    #[must_use]
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    #[must_use]
    pub fn commit(&self) -> &str {
        &self.commit
    }

    #[must_use]
    pub fn tree(&self) -> &str {
        &self.tree
    }

    #[must_use]
    pub fn policy_hash(&self) -> &str {
        &self.policy_hash
    }

    #[must_use]
    pub const fn credentials(&self) -> NoAgentCredentials {
        self.credentials
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTestWorker {
    worker_id: String,
    work_dir: PathBuf,
    commit: String,
    tree: String,
}

impl PreparedTestWorker {
    #[must_use]
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    #[must_use]
    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    #[must_use]
    pub fn commit(&self) -> &str {
        &self.commit
    }

    #[must_use]
    pub fn tree(&self) -> &str {
        &self.tree
    }
}

pub(crate) fn run_git(dir: &Path, args: &[&str]) -> Result<String, WorkflowError> {
    let child = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| WorkflowError::storage(format!("test worker git failed to spawn: {err}")))?;
    let output = child
        .wait_with_output()
        .map_err(|err| WorkflowError::storage(format!("test worker git wait failed: {err}")))?;
    if !output.status.success() {
        return Err(WorkflowError::storage(format!(
            "test worker git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn prepare_test_worker(
    repo: &Path,
    spec: &TestWorkerSpec,
    scratch_root: &Path,
) -> Result<PreparedTestWorker, WorkflowError> {
    if !repo.is_dir() {
        return Err(WorkflowError::not_found(format!(
            "test worker source repository {} does not exist",
            repo.display()
        )));
    }
    std::fs::create_dir_all(scratch_root).map_err(|err| {
        WorkflowError::storage(format!("create test worker scratch root failed: {err}"))
    })?;
    let work_dir = scratch_root.join(spec.worker_id());
    if work_dir.exists() {
        return Err(WorkflowError::invalid(format!(
            "test worker directory {} already exists; workers are single-use",
            work_dir.display()
        )));
    }
    run_git(
        scratch_root,
        &[
            "clone",
            "--no-hardlinks",
            "--no-checkout",
            "--",
            &repo.to_string_lossy(),
            spec.worker_id(),
        ],
    )?;
    run_git(&work_dir, &["checkout", "--detach", spec.commit()])?;
    let head = run_git(&work_dir, &["rev-parse", "HEAD"])?;
    if head != spec.commit() {
        return Err(WorkflowError::storage(format!(
            "test worker HEAD {head} does not match checkpoint {}",
            spec.commit()
        )));
    }
    let tree = run_git(&work_dir, &["rev-parse", "HEAD^{tree}"])?;
    if tree != spec.tree() {
        return Err(WorkflowError::storage(format!(
            "test worker tree {tree} does not match checkpoint {}",
            spec.tree()
        )));
    }
    let dirty = run_git(&work_dir, &["status", "--porcelain"])?;
    if !dirty.is_empty() {
        return Err(WorkflowError::storage(
            "test worker checkout is not clean after pinning the checkpoint".to_owned(),
        ));
    }
    Ok(PreparedTestWorker {
        worker_id: spec.worker_id().to_owned(),
        work_dir,
        commit: head,
        tree,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

pub(super) fn run_host_command_output_until(
    command: &mut std::process::Command,
    deadline: Instant,
) -> Result<Option<std::process::Output>, WorkflowError> {
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| {
            WorkflowError::blocked(format!("Docker state query failed to spawn: {err}"))
        })?;
    let stdout = child.stdout.take().map(|stream| {
        jackin_telemetry::spawn::thread_joined(move || process::read_bounded(stream))
    });
    let stderr = child.stderr.take().map(|stream| {
        jackin_telemetry::spawn::thread_joined(move || process::read_bounded(stream))
    });
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|err| {
            WorkflowError::blocked(format!("Docker state query wait failed: {err}"))
        })? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().map_err(|err| {
                WorkflowError::blocked(format!("Docker state query kill failed: {err}"))
            })?;
            child.wait().map_err(|err| {
                WorkflowError::blocked(format!("Docker state query reap failed: {err}"))
            })?;
            return Ok(None);
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "bounded Docker state query deadline poll on the host verification path"
        )]
        std::thread::sleep(Duration::from_millis(10));
    };
    let read = |handle: Option<std::thread::JoinHandle<BoundedOutput>>| {
        handle.map_or_else(
            || Ok(Vec::new()),
            |handle| {
                handle
                    .join()
                    .map(|output| output.bytes)
                    .map_err(|_| WorkflowError::blocked("Docker state reader panicked".to_owned()))
            },
        )
    };
    Ok(Some(std::process::Output {
        status,
        stdout: read(stdout)?,
        stderr: read(stderr)?,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestExecution {
    exit_code: i32,
    timed_out: bool,
    stdout: BoundedOutput,
    stderr: BoundedOutput,
}

pub fn run_test_command(
    policy: &TestPolicy,
    worker: &PreparedTestWorker,
    bundle_files: &[TestBundleFile],
) -> Result<TestExecution, WorkflowError> {
    if policy.args().len() != 2 {
        return Err(WorkflowError::invalid(
            "sandboxed test policy requires exact argv: <program> <trusted-script> <work-dir>"
                .to_owned(),
        ));
    }
    let script = Path::new(&policy.args()[0]).canonicalize().map_err(|err| {
        WorkflowError::invalid(format!("trusted test script cannot be resolved: {err}"))
    })?;
    if !script.is_file() {
        return Err(WorkflowError::invalid(
            "trusted test script is not a regular file".to_owned(),
        ));
    }
    let worker_dir = worker.work_dir().canonicalize().map_err(|err| {
        WorkflowError::storage(format!("test worker directory cannot be resolved: {err}"))
    })?;
    if Path::new(&policy.args()[1]) != worker.work_dir() {
        return Err(WorkflowError::invalid(
            "sandboxed test argv does not name its prepared worker".to_owned(),
        ));
    }
    let (bundle_dir, script_relative) = stage_trusted_bundle(&script, bundle_files, worker)?;
    let _bundle_cleanup = RemoveOnDrop(bundle_dir.clone());
    let container_name = format!("jackin-test-{}", uuid::Uuid::new_v4());
    let deadline = Instant::now() + policy.timeout();
    let deadline_unix = std::time::SystemTime::now()
        .checked_add(policy.timeout() + Duration::from_secs(5))
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0.0, |value| value.as_secs_f64());
    ensure_sandbox_image(worker.worker_id(), deadline)?;
    if Instant::now() >= deadline {
        return Err(WorkflowError::blocked(format!(
            "test worker {} exhausted its deadline preparing the pinned sandbox image",
            worker.worker_id()
        )));
    }
    let mut command = docker_sandbox_command(
        policy,
        worker,
        &worker_dir,
        &bundle_dir,
        &script_relative,
        &container_name,
        deadline_unix,
    );
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| {
            WorkflowError::storage(format!(
                "test worker {} failed to spawn Docker sandbox: {err}",
                worker.worker_id()
            ))
        })?;
    let mut container_cleanup = RemoveContainerOnDrop::new(container_name.clone());
    let stdout_handle = jackin_telemetry::spawn::thread_joined({
        let stdout = child.stdout.take();
        move || {
            stdout.map_or(
                BoundedOutput {
                    bytes: Vec::new(),
                    truncated: false,
                },
                process::read_bounded,
            )
        }
    });
    let stderr_handle = jackin_telemetry::spawn::thread_joined({
        let stderr = child.stderr.take();
        move || {
            stderr.map_or(
                BoundedOutput {
                    bytes: Vec::new(),
                    truncated: false,
                },
                process::read_bounded,
            )
        }
    });
    let mut timed_out = false;
    let docker_cli_code = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|err| WorkflowError::storage(format!("test worker wait failed: {err}")))?
        {
            break status.code().unwrap_or(-1);
        }
        if Instant::now() >= deadline {
            timed_out = true;
            child
                .kill()
                .map_err(|err| WorkflowError::storage(format!("test worker kill failed: {err}")))?;
            let cleanup = process::run_host_command_until(
                std::process::Command::new("docker").args(["rm", "--force", &container_name]),
                Instant::now() + Duration::from_secs(2),
            );
            if !matches!(cleanup, Ok(Some(status)) if status.success()) {
                return Err(WorkflowError::blocked(format!(
                    "test worker {} timed out and Docker sandbox cleanup failed",
                    worker.worker_id()
                )));
            }
            container_cleanup.disarm();
            let status = child
                .wait()
                .map_err(|err| WorkflowError::storage(format!("test worker reap failed: {err}")))?;
            break status.code().unwrap_or(-1);
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "bounded test deadline poll on the host verification path, off render/runtime threads"
        )]
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = process::join_reader(stdout_handle, "stdout")?;
    let stderr = process::join_reader(stderr_handle, "stderr")?;
    if timed_out {
        return Err(WorkflowError::blocked(format!(
            "test worker {} hit the {}s timeout; \
             one clean retry is allowed for this recorded infrastructure failure",
            worker.worker_id(),
            policy.timeout().as_secs()
        )));
    }
    finish_test_execution(
        worker,
        &container_name,
        docker_cli_code,
        stdout,
        stderr,
        &mut container_cleanup,
    )
}

fn ensure_sandbox_image(worker_id: &str, deadline: Instant) -> Result<(), WorkflowError> {
    let image_present = process::run_host_command_until(
        std::process::Command::new("docker").args(["image", "inspect", TEST_SANDBOX_IMAGE]),
        deadline,
    )?
    .is_some_and(|status| status.success());
    if image_present {
        return Ok(());
    }
    let pulled = process::run_host_command_until(
        std::process::Command::new("docker").args(["pull", TEST_SANDBOX_IMAGE]),
        deadline,
    )?
    .is_some_and(|status| status.success());
    if pulled {
        Ok(())
    } else {
        Err(WorkflowError::blocked(format!(
            "test worker {worker_id} cannot obtain its pinned Docker sandbox image"
        )))
    }
}

fn finish_test_execution(
    worker: &PreparedTestWorker,
    container_name: &str,
    docker_cli_code: i32,
    stdout: BoundedOutput,
    stderr: BoundedOutput,
    container_cleanup: &mut RemoveContainerOnDrop,
) -> Result<TestExecution, WorkflowError> {
    let state = run_host_command_output_until(
        std::process::Command::new("docker").args([
            "inspect",
            "--format",
            "{{json .State}}",
            container_name,
        ]),
        Instant::now() + Duration::from_secs(2),
    )?
    .ok_or_else(|| WorkflowError::blocked("Docker sandbox state query timed out".to_owned()))?;
    let cleanup = process::run_host_command_until(
        std::process::Command::new("docker").args(["rm", "--force", container_name]),
        Instant::now() + Duration::from_secs(2),
    );
    if !matches!(cleanup, Ok(Some(status)) if status.success()) {
        return Err(WorkflowError::blocked(format!(
            "test worker {} Docker sandbox cleanup failed",
            worker.worker_id()
        )));
    }
    container_cleanup.disarm();
    if !state.status.success() {
        return Err(WorkflowError::blocked(format!(
            "test worker {} Docker sandbox did not produce a container state",
            worker.worker_id()
        )));
    }
    let state: serde_json::Value = serde_json::from_slice(&state.stdout).map_err(|err| {
        WorkflowError::blocked(format!(
            "test worker Docker sandbox returned invalid state: {err}"
        ))
    })?;
    if state
        .get("Error")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|error| !error.is_empty())
    {
        return Err(WorkflowError::blocked(format!(
            "test worker {} Docker sandbox failed before test completion",
            worker.worker_id()
        )));
    }
    if state.get("Status").and_then(serde_json::Value::as_str) != Some("exited")
        || state.get("Running").and_then(serde_json::Value::as_bool) != Some(false)
        || state
            .get("StartedAt")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|started| started.starts_with("0001-01-01"))
    {
        return Err(WorkflowError::blocked(format!(
            "test worker {} Docker sandbox did not complete cleanly",
            worker.worker_id()
        )));
    }
    let code = state
        .get("ExitCode")
        .and_then(serde_json::Value::as_i64)
        .and_then(|exit| i32::try_from(exit).ok())
        .ok_or_else(|| {
            WorkflowError::blocked("test worker Docker sandbox omitted its exit code".to_owned())
        })?;
    if code != docker_cli_code {
        return Err(WorkflowError::blocked(format!(
            "test worker {} Docker exit status disagrees with container state",
            worker.worker_id()
        )));
    }
    Ok(TestExecution {
        exit_code: code,
        timed_out: false,
        stdout,
        stderr,
    })
}

fn stage_trusted_bundle(
    script: &Path,
    bundle_files: &[TestBundleFile],
    worker: &PreparedTestWorker,
) -> Result<(PathBuf, PathBuf), WorkflowError> {
    let script_relative = bundle_files
        .iter()
        .find_map(|file| {
            let relative = Path::new(&file.logical_path);
            if relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
            {
                return None;
            }
            let mut root = script.to_path_buf();
            for _ in relative.components() {
                root.pop();
            }
            (root.join(relative).canonicalize().ok().as_deref() == Some(script))
                .then_some(relative.to_path_buf())
        })
        .ok_or_else(|| {
            WorkflowError::invalid(
                "trusted test script is not addressable from its bundle manifest".to_owned(),
            )
        })?;
    let mut bundle_root = script.to_path_buf();
    for _ in script_relative.components() {
        bundle_root.pop();
    }
    let bundle_dir = worker
        .work_dir()
        .parent()
        .ok_or_else(|| WorkflowError::storage("test worker has no scratch parent".to_owned()))?
        .join(format!(".trusted-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&bundle_dir).map_err(|err| {
        WorkflowError::storage(format!("create isolated trusted bundle failed: {err}"))
    })?;
    for file in bundle_files {
        let relative = Path::new(&file.logical_path);
        if relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            let _unused = std::fs::remove_dir_all(&bundle_dir);
            return Err(WorkflowError::invalid(
                "trusted bundle path escapes its root".to_owned(),
            ));
        }
        let bytes = std::fs::read(bundle_root.join(relative)).map_err(|err| {
            WorkflowError::storage(format!(
                "read trusted bundle file {} failed: {err}",
                file.logical_path
            ))
        })?;
        if hex::encode(Sha256::digest(&bytes)) != file.digest {
            let _unused = std::fs::remove_dir_all(&bundle_dir);
            return Err(WorkflowError::invalid(format!(
                "trusted bundle file {} digest changed before sandbox staging",
                file.logical_path
            )));
        }
        let destination = bundle_dir.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                WorkflowError::storage(format!("create trusted bundle subdirectory failed: {err}"))
            })?;
        }
        std::fs::write(destination, bytes).map_err(|err| {
            WorkflowError::storage(format!(
                "stage trusted bundle file {} failed: {err}",
                file.logical_path
            ))
        })?;
    }
    Ok((bundle_dir, script_relative))
}

fn docker_sandbox_command(
    policy: &TestPolicy,
    worker: &PreparedTestWorker,
    worker_dir: &Path,
    bundle_dir: &Path,
    script_relative: &Path,
    container_name: &str,
    deadline_unix: f64,
) -> std::process::Command {
    let mut command = std::process::Command::new("docker");
    command
        .args([
            "run",
            "--pull=never",
            "--network=none",
            "--read-only",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--memory=536870912",
            "--cpus=1",
            "--pids-limit=64",
            "--tmpfs=/tmp:rw,noexec,nosuid,size=67108864",
            "--tmpfs=/build:rw,nosuid,size=268435456",
            "--user=0:0",
            "--workdir=/work/repo",
        ])
        .arg(format!("--name={container_name}"))
        .arg(format!(
            "--label=jackin.workflow.worker={}",
            worker.worker_id()
        ))
        .arg("--mount")
        .arg(format!(
            "type=bind,src={},dst=/work/repo,readonly",
            worker_dir.display()
        ))
        .arg("--mount")
        .arg(format!(
            "type=bind,src={},dst=/jackin/checks,readonly",
            bundle_dir.display()
        ))
        .arg("--env=PYTHONDONTWRITEBYTECODE=1")
        .arg("--entrypoint=python3")
        .arg(TEST_SANDBOX_IMAGE)
        .arg("-c")
        .arg(TEST_COMMAND_WRAPPER)
        .arg(deadline_unix.to_string())
        .arg(policy.program());
    for argument in policy.args() {
        if Path::new(argument) == Path::new(&policy.args()[0]) {
            command.arg(format!(
                "{}{}",
                jackin_core::container_paths::CHECKS_PREFIX,
                script_relative.to_string_lossy()
            ));
        } else if Path::new(argument) == worker.work_dir() {
            command.arg("/work/repo");
        } else {
            command.arg(argument);
        }
    }
    command
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _unused = std::fs::remove_dir_all(&self.0);
    }
}

struct RemoveContainerOnDrop {
    name: String,
    armed: bool,
}

impl RemoveContainerOnDrop {
    fn new(name: String) -> Self {
        Self { name, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for RemoveContainerOnDrop {
    fn drop(&mut self) {
        if self.armed {
            let _cleanup = process::run_host_command_until(
                std::process::Command::new("docker").args(["rm", "--force", &self.name]),
                Instant::now() + Duration::from_secs(2),
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestResult {
    candidate_id: String,
    commit: String,
    tree: String,
    policy_hash: String,
    worker_id: String,
    exit_code: i32,
    passed: bool,
    excerpt: String,
    output_digest: String,
}

impl TestResult {
    #[must_use]
    pub fn from_execution(
        spec: &TestWorkerSpec,
        policy: &TestPolicy,
        execution: &TestExecution,
    ) -> Self {
        debug_assert_eq!(spec.policy_hash(), policy.policy_hash());
        let mut hasher = Sha256::new();
        hasher.update(execution.stdout().bytes());
        hasher.update([0]);
        hasher.update(execution.stderr().bytes());
        let combined = format!(
            "{}\n{}",
            execution.stderr().text(),
            execution.stdout().text()
        );
        let excerpt: String = combined.chars().take(MAX_TEST_EXCERPT_BYTES).collect();
        Self {
            candidate_id: spec.candidate_id().to_owned(),
            commit: spec.commit().to_owned(),
            tree: spec.tree().to_owned(),
            policy_hash: policy.policy_hash().to_owned(),
            worker_id: spec.worker_id().to_owned(),
            exit_code: execution.exit_code(),
            passed: execution.exit_code() == 0 && !execution.timed_out(),
            excerpt,
            output_digest: hex::encode(hasher.finalize()),
        }
    }

    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    #[must_use]
    pub const fn passed(&self) -> bool {
        self.passed
    }

    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        self.exit_code
    }

    #[must_use]
    pub fn excerpt(&self) -> &str {
        &self.excerpt
    }

    #[must_use]
    pub fn output_digest(&self) -> &str {
        &self.output_digest
    }
}

pub fn persist_test_output(
    store: &RunStore,
    candidate_id: &str,
    execution: &TestExecution,
) -> Result<String, WorkflowError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        format!(
            "candidate {candidate_id} {}\n--- stdout ---\n",
            execution.exit_label()
        )
        .as_bytes(),
    );
    bytes.extend_from_slice(execution.stdout().bytes());
    bytes.extend_from_slice(b"\n--- stderr ---\n");
    bytes.extend_from_slice(execution.stderr().bytes());
    let artifact = store.write_artifact(&format!("test-output-{candidate_id}"), &bytes)?;
    Ok(artifact.digest)
}

pub fn record_test_evidence(
    store: &RunStore,
    run_id: &str,
    result: &TestResult,
    artifact_digest: &str,
) -> Result<(), WorkflowError> {
    store.record_evidence(&EvidenceRecord {
        run_id,
        candidate: result.candidate_id(),
        kind: "test",
        source: "trusted-test-worker",
        quality: "host-observed",
        result: if result.passed() { "pass" } else { "fail" },
        policy_hash: Some(&result.policy_hash),
        artifact_digest: Some(artifact_digest),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Blocker,
    Major,
    Minor,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewFinding {
    pub id: String,
    pub severity: Severity,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_start: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_end: Option<u32>,
    pub explanation: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pass,
    Fail,
}

impl ReviewStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewResult {
    pub schema_version: u8,
    pub candidate_id: String,
    pub commit: String,
    pub tree: String,
    pub status: ReviewStatus,
    #[serde(default)]
    pub findings: Vec<ReviewFinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewExpectation<'a> {
    pub candidate_id: &'a str,
    pub commit: &'a str,
    pub tree: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewWorkerSpec {
    worker_id: String,
    candidate_id: String,
    commit: String,
    tree: String,
    result_path: PathBuf,
    corrections_used: u32,
}

impl ReviewWorkerSpec {
    pub fn new(
        run_id: &str,
        round: u32,
        candidate_id: &str,
        commit: &str,
        tree: &str,
        result_dir: &Path,
    ) -> Result<Self, WorkflowError> {
        for (label, value) in [
            ("run", run_id),
            ("candidate", candidate_id),
            ("commit", commit),
            ("tree", tree),
        ] {
            if value.trim().is_empty()
                || value
                    .chars()
                    .any(|ch| ch.is_whitespace() || ch == '/' || ch == '\0')
            {
                return Err(WorkflowError::invalid(format!(
                    "review worker {label} must be a single non-empty path-safe token"
                )));
            }
        }
        Ok(Self {
            worker_id: format!("review-{run_id}-r{round}-{candidate_id}"),
            candidate_id: candidate_id.to_owned(),
            commit: commit.to_owned(),
            tree: tree.to_owned(),
            result_path: result_dir.join("review.json"),
            corrections_used: 0,
        })
    }

    #[must_use]
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    #[must_use]
    pub fn result_path(&self) -> &Path {
        &self.result_path
    }

    #[must_use]
    pub const fn corrections_used(&self) -> u32 {
        self.corrections_used
    }

    pub fn take_correction(&mut self) -> Result<(), WorkflowError> {
        if !correction_allowed(self.corrections_used) {
            return Err(WorkflowError::blocked(
                "review correction budget is spent; operator action required".to_owned(),
            ));
        }
        self.corrections_used += 1;
        Ok(())
    }

    #[must_use]
    pub fn expectation(&self) -> ReviewExpectation<'_> {
        ReviewExpectation {
            candidate_id: &self.candidate_id,
            commit: &self.commit,
            tree: &self.tree,
        }
    }
}

#[must_use]
pub const fn correction_allowed(used: u32) -> bool {
    used < MAX_REVIEW_CORRECTIONS
}

pub fn read_review_bytes(path: &Path) -> Result<Vec<u8>, WorkflowError> {
    let len = std::fs::metadata(path)
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                WorkflowError::not_found(format!(
                    "review result {} is missing; a missing review blocks acceptance",
                    path.display()
                ))
            } else {
                WorkflowError::storage(format!(
                    "read review result {} failed: {err}",
                    path.display()
                ))
            }
        })?
        .len();
    if len > MAX_REVIEW_BYTES {
        return Err(WorkflowError::invalid(format!(
            "review result is {len} bytes; the limit is {MAX_REVIEW_BYTES}"
        )));
    }
    let bytes = std::fs::read(path).map_err(|err| {
        WorkflowError::storage(format!(
            "read review result {} failed: {err}",
            path.display()
        ))
    })?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_REVIEW_BYTES {
        return Err(WorkflowError::invalid(format!(
            "review result is {} bytes; the limit is {MAX_REVIEW_BYTES}",
            bytes.len()
        )));
    }
    Ok(bytes)
}

fn valid_relative_path(path: &str) -> bool {
    if path.is_empty() || path.len() > 512 || path.starts_with('/') {
        return false;
    }
    !path.split('/').any(|part| part.is_empty() || part == "..")
}

pub fn validate_review(
    bytes: &[u8],
    expected: ReviewExpectation<'_>,
) -> Result<ReviewResult, WorkflowError> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_REVIEW_BYTES {
        return Err(WorkflowError::invalid(format!(
            "review result is {} bytes; the limit is {MAX_REVIEW_BYTES}",
            bytes.len()
        )));
    }
    let result: ReviewResult = serde_json::from_slice(bytes).map_err(|err| {
        WorkflowError::invalid(format!("review result is not valid review JSON: {err}"))
    })?;
    if result.schema_version != 1 {
        return Err(WorkflowError::invalid(format!(
            "review schema_version is {}; only version 1 is accepted",
            result.schema_version
        )));
    }
    for (label, actual, wanted) in [
        (
            "candidate_id",
            result.candidate_id.as_str(),
            expected.candidate_id,
        ),
        ("commit", result.commit.as_str(), expected.commit),
        ("tree", result.tree.as_str(), expected.tree),
    ] {
        if actual.trim().is_empty() || actual != wanted {
            return Err(WorkflowError::invalid(format!(
                "review {label} does not name the candidate under review"
            )));
        }
    }
    if result.findings.len() > 256 {
        return Err(WorkflowError::invalid(
            "review carries more than 256 findings".to_owned(),
        ));
    }
    for finding in &result.findings {
        if finding.id.trim().is_empty() || finding.id.len() > 128 {
            return Err(WorkflowError::invalid(
                "review finding needs a non-empty id of at most 128 bytes".to_owned(),
            ));
        }
        if !valid_relative_path(&finding.path) {
            return Err(WorkflowError::invalid(format!(
                "review finding path {:?} is outside the candidate source",
                finding.path
            )));
        }
        match (finding.line_start, finding.line_end) {
            (Some(0), _) | (_, Some(0)) => {
                return Err(WorkflowError::invalid(
                    "review line numbers are 1-based".to_owned(),
                ));
            }
            (Some(start), Some(end)) if end < start => {
                return Err(WorkflowError::invalid(
                    "review line range ends before it starts".to_owned(),
                ));
            }
            (None, Some(_)) => {
                return Err(WorkflowError::invalid(
                    "review line_end needs a line_start".to_owned(),
                ));
            }
            _ => {}
        }
        for (label, text) in [
            ("explanation", finding.explanation.as_str()),
            ("suggestion", finding.suggestion.as_str()),
        ] {
            if text.trim().is_empty() || text.len() > 4_096 {
                return Err(WorkflowError::invalid(format!(
                    "review finding {label} must be non-empty and at most 4096 bytes"
                )));
            }
        }
    }
    let blocking = result
        .findings
        .iter()
        .filter(|finding| !matches!(finding.severity, Severity::Note))
        .count();
    match result.status {
        ReviewStatus::Pass if blocking > 0 => {
            return Err(WorkflowError::invalid(
                "a passing review must not carry blocker, major, or minor findings".to_owned(),
            ));
        }
        ReviewStatus::Fail if result.findings.is_empty() => {
            return Err(WorkflowError::invalid(
                "a failing review needs at least one finding".to_owned(),
            ));
        }
        _ => {}
    }
    Ok(result)
}

#[must_use]
pub fn review_acceptable(result: &ReviewResult) -> bool {
    result.status == ReviewStatus::Pass
        && result
            .findings
            .iter()
            .all(|finding| finding.severity == Severity::Note)
}

pub fn record_review_evidence(
    store: &RunStore,
    run_id: &str,
    candidate_id: &str,
    policy_hash: &str,
    result: &ReviewResult,
    artifact_digest: &str,
) -> Result<(), WorkflowError> {
    store.record_evidence(&EvidenceRecord {
        run_id,
        candidate: candidate_id,
        kind: "review",
        source: "independent-review-worker",
        quality: "host-validated",
        result: result.status.as_str(),
        policy_hash: Some(policy_hash),
        artifact_digest: Some(artifact_digest),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailingCheck {
    pub name: String,
    pub excerpt: String,
    pub location: Option<String>,
}

#[must_use]
pub fn summarize_test_failure(
    policy: &TestPolicy,
    execution: &TestExecution,
    excerpt: &str,
) -> FailingCheck {
    let argv = policy.argv().join(" ");
    let argv_short: String = argv.chars().take(160).collect();
    let name = format!("{argv_short} ({})", execution.exit_label());
    let lines: Vec<&str> = excerpt.lines().collect();
    let mut picked: Vec<&str> = lines
        .iter()
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("assert")
                || lower.contains("fail")
                || lower.contains("error")
                || lower.contains("traceback")
        })
        .take(10)
        .copied()
        .collect();
    if picked.is_empty() {
        let tail: Vec<&str> = lines.iter().rev().take(10).rev().copied().collect();
        picked = tail;
    }
    let excerpt: String = picked
        .iter()
        .map(|line| line.chars().take(200).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    FailingCheck {
        name,
        location: traceback_location(&lines),
        excerpt,
    }
}

fn traceback_location(lines: &[&str]) -> Option<String> {
    for line in lines {
        let Some(file_start) = line.find("File \"") else {
            continue;
        };
        let rest = &line[file_start + 6..];
        let Some(file_end) = rest.find('"') else {
            continue;
        };
        let file = &rest[..file_end];
        let after = &rest[file_end..];
        let Some(line_start) = after.find(", line ") else {
            continue;
        };
        let digits: String = after[line_start + 7..]
            .chars()
            .take_while(|ch| ch.is_numeric())
            .collect();
        if digits.is_empty() {
            continue;
        }
        let base = file.rsplit('/').next().unwrap_or(file);
        return Some(format!("{base}:{digits}"));
    }
    None
}

fn redact_secrets(text: &str) -> String {
    const TOKEN_PREFIXES: &[&str] = &[
        "sk-", "AKIA", "xoxa-", "xoxb-", "xoxp-", "ghp_", "gho_", "gsk_", "Bearer ",
    ];
    const SECRET_LABELS: &[&str] = &[
        "api_key",
        "apikey",
        "secret",
        "token",
        "password",
        "session_id",
        "agent_id",
        "account_id",
    ];
    let mut out = Vec::new();
    for line in text.lines() {
        let mut redacted = line.to_owned();
        for prefix in TOKEN_PREFIXES {
            let mut search_from = 0;
            while let Some(hit) = redacted[search_from..].find(prefix) {
                let start = search_from + hit;
                let end = redacted[start..]
                    .find(|ch: char| ch.is_whitespace() || ch == '"' || ch == '\'')
                    .map_or(redacted.len(), |offset| start + offset);
                if end - start > prefix.len() + 3 {
                    redacted.replace_range(start..end, "[redacted]");
                    search_from = start + "[redacted]".len();
                } else {
                    search_from = start + prefix.len();
                }
            }
        }
        let lower = redacted.to_lowercase();
        for label in SECRET_LABELS {
            let mut search_from = 0;
            while let Some(hit) = lower[search_from..].find(label) {
                let label_start = search_from + hit;
                let after = &redacted[label_start + label.len()..];
                let value_start = after
                    .find(|ch: char| ch != ' ' && ch != '\t' && ch != ':' && ch != '=')
                    .unwrap_or(after.len());
                if value_start >= after.len() {
                    break;
                }
                let absolute = label_start + label.len() + value_start;
                let value_end = redacted[absolute..]
                    .find(|ch: char| ch.is_whitespace() || ch == '"' || ch == '\'' || ch == ';')
                    .map_or(redacted.len(), |offset| absolute + offset);
                if value_end > absolute {
                    redacted.replace_range(absolute..value_end, "[redacted]");
                    search_from = absolute + "[redacted]".len();
                    continue;
                }
                search_from = absolute;
            }
        }
        out.push(redacted);
    }
    out.join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackInput<'a> {
    pub candidate_id: &'a str,
    pub commit: &'a str,
    pub round: u32,
    pub remaining_repairs: u32,
    pub checks: Vec<FailingCheck>,
    pub findings: Vec<&'a ReviewFinding>,
    pub evidence_refs: Vec<String>,
}

fn finding_location(finding: &ReviewFinding) -> String {
    match (finding.line_start, finding.line_end) {
        (Some(start), Some(end)) if end != start => {
            format!("{}:{start}-{end}", finding.path)
        }
        (Some(start), _) => format!("{}:{start}", finding.path),
        (None, _) => finding.path.clone(),
    }
}

pub fn build_feedback(input: &FeedbackInput<'_>) -> Result<String, WorkflowError> {
    if input.candidate_id.trim().is_empty() || input.commit.trim().is_empty() {
        return Err(WorkflowError::invalid(
            "feedback needs the current candidate identity".to_owned(),
        ));
    }
    if input.checks.is_empty() && input.findings.is_empty() {
        return Err(WorkflowError::invalid(
            "feedback needs at least one failing check or finding".to_owned(),
        ));
    }
    let short_commit: String = input.commit.chars().take(12).collect();
    let mut doc = format!(
        "repair feedback for {} ({short_commit}) round {}: {} repair turn(s) left\n",
        input.candidate_id, input.round, input.remaining_repairs
    );
    if !input.checks.is_empty() {
        doc.push_str("failing checks:\n");
        for check in &input.checks {
            doc.push_str(&format!("- {}", check.name));
            if let Some(location) = &check.location {
                doc.push_str(&format!(" at {location}"));
            }
            doc.push('\n');
            for line in check.excerpt.lines().take(10) {
                doc.push_str("  ");
                doc.push_str(&line.chars().take(200).collect::<String>());
                doc.push('\n');
            }
        }
    }
    if !input.findings.is_empty() {
        doc.push_str("review findings:\n");
        for finding in &input.findings {
            let explanation: String = finding.explanation.chars().take(300).collect();
            let suggestion: String = finding.suggestion.chars().take(300).collect();
            doc.push_str(&format!(
                "- [{}] {} {} — {explanation} → {suggestion}\n",
                severity_label(finding.severity),
                finding.id,
                finding_location(finding),
            ));
        }
    }
    if !input.evidence_refs.is_empty() {
        doc.push_str("evidence:\n");
        for reference in &input.evidence_refs {
            doc.push_str(&format!(
                "- {}\n",
                reference.chars().take(160).collect::<String>()
            ));
        }
    }
    let scrubbed = redact_secrets(&doc);
    if scrubbed.len() <= MAX_FEEDBACK_BYTES {
        return Ok(scrubbed);
    }
    let mut capped: String = scrubbed.chars().take(MAX_FEEDBACK_BYTES).collect();
    capped.push_str("\n[truncated]");
    Ok(capped)
}

const fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Blocker => "blocker",
        Severity::Major => "major",
        Severity::Minor => "minor",
        Severity::Note => "note",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateRef<'a> {
    pub candidate_id: &'a str,
    pub commit: &'a str,
    pub tree: &'a str,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationOutcome<'a> {
    pub test_passed: bool,
    pub review_acceptable: bool,
    pub test_policy_hash: &'a str,
    pub expected_policy_hash: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostReadiness {
    pub source_current: bool,
    pub backup_satisfied: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputOwnership {
    pub host_owned: bool,
    pub epoch_current: bool,
    pub uncertainty_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateEvidence<'a> {
    pub candidate: CandidateRef<'a>,
    pub current: CandidateRef<'a>,
    pub verification: VerificationOutcome<'a>,
    pub readiness: HostReadiness,
    pub ownership: InputOwnership,
    pub round: u32,
    pub max_repair_turns: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Accept,
    Repair,
    Fail(String),
    NeedsOperator(String),
}

#[must_use]
pub fn decide_candidate(evidence: &CandidateEvidence<'_>) -> Decision {
    let candidate = &evidence.candidate;
    if candidate.candidate_id.trim().is_empty()
        || candidate.commit.trim().is_empty()
        || candidate.tree.trim().is_empty()
    {
        return Decision::NeedsOperator("incomplete candidate evidence".to_owned());
    }
    if *candidate != evidence.current {
        return Decision::NeedsOperator(
            "stale evidence: the candidate moved; re-verify the current candidate".to_owned(),
        );
    }
    let verification = &evidence.verification;
    if verification.test_policy_hash != verification.expected_policy_hash {
        return Decision::NeedsOperator(
            "stale evidence: the test policy changed; a new plan is required".to_owned(),
        );
    }
    let ownership = &evidence.ownership;
    if ownership.uncertainty_pending {
        return Decision::NeedsOperator(
            "unresolved delivery uncertainty blocks the decision".to_owned(),
        );
    }
    if !ownership.epoch_current {
        return Decision::NeedsOperator(
            "owner epoch moved; automatic input from this epoch is stale".to_owned(),
        );
    }
    if !ownership.host_owned {
        return Decision::NeedsOperator(
            "automatic input is not host-owned; resolve input ownership first".to_owned(),
        );
    }
    let readiness = &evidence.readiness;
    if !readiness.source_current {
        return Decision::NeedsOperator(
            "source drifted from the candidate; recapture before deciding".to_owned(),
        );
    }
    if !readiness.backup_satisfied {
        return Decision::NeedsOperator("required backup has not completed".to_owned());
    }
    if verification.test_passed && verification.review_acceptable {
        return Decision::Accept;
    }
    if evidence.round < evidence.max_repair_turns {
        return Decision::Repair;
    }
    Decision::Fail("repair budget is exhausted with failing verification".to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionIdentity {
    pub instance_id: String,
    pub session_id: u64,
    pub capsule_epoch: Option<u64>,
    pub process_identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryGate {
    pub recorded: SessionIdentity,
    pub observed: SessionIdentity,
    pub owner_epoch_current: bool,
    pub operator_owned: bool,
    pub turn_ended: bool,
}

pub fn admit_repair_delivery(gate: &DeliveryGate) -> Result<(), WorkflowError> {
    if gate.recorded != gate.observed {
        return Err(WorkflowError::blocked(
            "implementer session identity changed; \
             refusing repair delivery to an uncertain session"
                .to_owned(),
        ));
    }
    if !gate.owner_epoch_current {
        return Err(WorkflowError::blocked(
            "owner epoch moved; repair prompt from this epoch is stale".to_owned(),
        ));
    }
    if gate.operator_owned {
        return Err(WorkflowError::blocked(
            "operator owns session input; automatic repair input is refused".to_owned(),
        ));
    }
    if !gate.turn_ended {
        return Err(WorkflowError::blocked(
            "implementer turn has not ended; repair delivery waits".to_owned(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairDelivery {
    pub command_id: String,
    pub prompt_digest: String,
}

pub fn stage_repair_feedback(
    service: &WorkflowService,
    run_id: &str,
    session_ref: &str,
    feedback: &str,
    turn: u32,
    lease_generation: u64,
    gate: &DeliveryGate,
) -> Result<RepairDelivery, WorkflowError> {
    admit_repair_delivery(gate)?;
    if feedback.trim().is_empty() || feedback.len() > MAX_FEEDBACK_BYTES + 16 {
        return Err(WorkflowError::invalid(
            "repair feedback must be the bounded document from build_feedback".to_owned(),
        ));
    }
    let (command_id, prompt_digest) =
        service.begin_dispatch(run_id, session_ref, feedback, turn, lease_generation)?;
    service
        .store()
        .append_event(run_id, "host", "repair_feedback_staged", Some(&command_id))?;
    Ok(RepairDelivery {
        command_id,
        prompt_digest,
    })
}

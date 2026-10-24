// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use std::collections::BTreeMap;
use std::io::{BufRead as _, Write as _};
use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _, PermissionsExt as _};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{
    Arc,
    mpsc::{Receiver, channel},
};
use std::time::{Duration, Instant};

const CHILD_ROOT_ENV: &str = "JACKIN_MARKER_PAIR_CHILD_ROOT";
const CHILD_COMMAND_ENV: &str = "JACKIN_MARKER_PAIR_CHILD_COMMAND";
const CHILD_READY_PREFIX: &str = "MARKER_PAIR_READY ";
const CHILD_TEST_ID: &str = "account_identity::tests::case_03::marker_pair_child_fixture";
const CHILD_RELEASE_COMMAND: &str = "release";
const TEST_ROLE: &str = "marker-pair-test-role";

#[derive(Clone, Debug, Eq, PartialEq)]
struct SnapshotEntry {
    kind: &'static str,
    mode: u32,
    device: u64,
    inode: u64,
    contents: Option<Vec<u8>>,
    link_target: Option<PathBuf>,
}

fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, SnapshotEntry> {
    fn visit(root: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, SnapshotEntry>) {
        let metadata = std::fs::symlink_metadata(path).unwrap();
        let file_type = metadata.file_type();
        let kind = if file_type.is_symlink() {
            "symlink"
        } else if file_type.is_dir() {
            "directory"
        } else if file_type.is_file() {
            "file"
        } else if file_type.is_socket() {
            "socket"
        } else if file_type.is_fifo() {
            "fifo"
        } else {
            "other"
        };
        let contents = if file_type.is_file() {
            Some(std::fs::read(path).unwrap())
        } else {
            None
        };
        let link_target = if file_type.is_symlink() {
            Some(std::fs::read_link(path).unwrap())
        } else {
            None
        };
        let relative = path.strip_prefix(root).unwrap().to_path_buf();
        entries.insert(
            relative,
            SnapshotEntry {
                kind,
                mode: metadata.permissions().mode(),
                device: metadata.dev(),
                inode: metadata.ino(),
                contents,
                link_target,
            },
        );
        if file_type.is_dir() {
            let mut children: Vec<_> = std::fs::read_dir(path)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect();
            children.sort();
            for child in children {
                visit(root, &child, entries);
            }
        }
    }

    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries);
    entries
}

fn set_directory_mode(path: &Path, mode: u32) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(path, permissions).unwrap();
}

pub(super) fn prepare_marker_root(
    root: &Path,
    config: &AppConfig,
    admitted: &[AdmittedInstance],
) -> jackin_core::JackinPaths {
    std::fs::create_dir_all(root).unwrap();
    set_directory_mode(root, 0o700);
    let paths = jackin_core::JackinPaths::for_tests(root);
    write_generation_fixture(&paths, config);
    let mut manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: "marker-pair-fixture",
        workspace_name: None,
        workspace_label: "marker-pair-fixture",
        workdir: "/workspace",
        host_workdir_fingerprint: "marker-pair-fixture",
        role_key: TEST_ROLE,
        role_display_name: "Marker pair fixture",
        agent_runtime: Agent::Claude,
        role_source_git: "",
        role_source_ref: None,
        image_tag: "marker-pair-fixture",
        docker: DockerResources::from_container_name("marker-pair-fixture"),
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![Agent::Claude],
    });
    manifest.set_admitted_instances(admitted.iter().cloned());
    manifest.write(root).unwrap();
    paths
}

fn persist_config(paths: &jackin_core::JackinPaths, config: &AppConfig) {
    std::fs::write(&paths.config_file, toml::to_string(config).unwrap()).unwrap();
}

pub(super) fn record_pair(
    root: &Path,
    paths: &jackin_core::JackinPaths,
    revision: &AccountConfigRevision,
    selected: &AppConfig,
    admission: &AppConfig,
    admitted: &[AdmittedInstance],
) -> anyhow::Result<()> {
    record_account_configuration(AccountConfigurationRecord {
        root,
        paths,
        revision,
        config: selected,
        admission_config: admission,
        workspace: None,
        role: TEST_ROLE,
        admitted,
    })
}

fn marker_bytes(root: &Path) -> (Vec<u8>, Vec<u8>) {
    (
        std::fs::read(root.join(ACCOUNT_FINGERPRINT_FILE)).unwrap(),
        std::fs::read(root.join(ACCOUNT_ADMISSION_FILE)).unwrap(),
    )
}

fn with_environment(config: &AppConfig, name: &str, value: &str) -> AppConfig {
    let mut changed = config.clone();
    changed.env.insert(name.to_owned(), EnvValue::from(value));
    changed
}

pub(super) struct MarkerPairChild {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout_completion: Option<Receiver<std::io::Result<u64>>>,
    root: PathBuf,
    preserve_root: Arc<AtomicBool>,
    termination_attempted: bool,
}

pub(super) struct MarkerPairTestRoot {
    temp: Option<tempfile::TempDir>,
    preserve: Arc<AtomicBool>,
}

#[derive(Clone, Copy)]
pub(super) enum MarkerPairTransactionChildCommand {
    PauseAfterPrepared,
    PauseAfterSelectedInstall,
    PauseAfterAdmissionInstall,
    PauseBeforeCommitRename,
    PauseAfterCommitRename,
    PauseAfterCommitSync,
    PauseAfterPreparedExpectingPreflightRefusal,
    RecoverInFreshChild,
}

impl MarkerPairTransactionChildCommand {
    fn as_str(self) -> &'static str {
        match self {
            Self::PauseAfterPrepared => "prepared",
            Self::PauseAfterSelectedInstall => "selected",
            Self::PauseAfterAdmissionInstall => "admission",
            Self::PauseBeforeCommitRename => "before_commit",
            Self::PauseAfterCommitRename => "after_commit",
            Self::PauseAfterCommitSync => "after_commit_sync",
            Self::PauseAfterPreparedExpectingPreflightRefusal => "preflight_refusal",
            Self::RecoverInFreshChild => "recover",
        }
    }

    fn point(self) -> Option<marker_transaction::TransactionPausePoint> {
        use super::super::marker_transaction::TransactionPausePoint as Point;
        Some(match self {
            Self::PauseAfterPrepared | Self::PauseAfterPreparedExpectingPreflightRefusal => {
                Point::PreparedDurable
            }
            Self::PauseAfterSelectedInstall => Point::SelectedInstalled,
            Self::PauseAfterAdmissionInstall => Point::AdmissionInstalled,
            Self::PauseBeforeCommitRename => Point::BeforeCommitRename,
            Self::PauseAfterCommitRename => Point::AfterCommitRename,
            Self::PauseAfterCommitSync => Point::AfterCommitSync,
            Self::RecoverInFreshChild => return None,
        })
    }

    fn from_str(command: &str) -> Option<Self> {
        Some(match command {
            "prepared" => Self::PauseAfterPrepared,
            "selected" => Self::PauseAfterSelectedInstall,
            "admission" => Self::PauseAfterAdmissionInstall,
            "before_commit" => Self::PauseBeforeCommitRename,
            "after_commit" => Self::PauseAfterCommitRename,
            "after_commit_sync" => Self::PauseAfterCommitSync,
            "preflight_refusal" => Self::PauseAfterPreparedExpectingPreflightRefusal,
            "recover" => Self::RecoverInFreshChild,
            _ => return None,
        })
    }
}

impl MarkerPairTestRoot {
    pub(super) fn new() -> std::io::Result<Self> {
        Ok(Self {
            temp: Some(tempfile::tempdir()?),
            preserve: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(super) fn path(&self) -> &Path {
        self.temp
            .as_ref()
            .expect("marker-pair test root is present before drop")
            .path()
    }

    pub(super) fn preservation_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.preserve)
    }
}

impl Drop for MarkerPairTestRoot {
    fn drop(&mut self) {
        if self.preserve.load(Ordering::Acquire)
            && let Some(temp) = self.temp.take()
        {
            let _preserved_path = temp.keep();
        }
    }
}

impl MarkerPairChild {
    pub(super) fn spawn(
        root: &Path,
        preserve_root: Arc<AtomicBool>,
    ) -> anyhow::Result<(Self, u32)> {
        Self::spawn_command(root, preserve_root, "hold")
    }

    pub(super) fn spawn_transaction(
        root: &Path,
        preserve_root: Arc<AtomicBool>,
        command: MarkerPairTransactionChildCommand,
    ) -> anyhow::Result<(Self, u32)> {
        Self::spawn_command(root, preserve_root, command.as_str())
    }

    fn spawn_command(
        root: &Path,
        preserve_root: Arc<AtomicBool>,
        command: &str,
    ) -> anyhow::Result<(Self, u32)> {
        let child = Command::new(std::env::current_exe().context("current test executable")?)
            .args(["--exact", CHILD_TEST_ID, "--ignored", "--nocapture"])
            .env(CHILD_ROOT_ENV, root)
            .env(CHILD_COMMAND_ENV, command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("spawn owned marker-pair lock child")?;
        let child_id = child.id();
        let mut owner = Self {
            child,
            stdin: None,
            stdout_completion: None,
            root: root.to_path_buf(),
            preserve_root,
            termination_attempted: false,
        };
        owner.stdin = owner.child.stdin.take();
        if owner.stdin.is_none() {
            return Err(owner.abort_with(anyhow::anyhow!("marker-pair child stdin was not piped")));
        }
        let Some(stdout) = owner.child.stdout.take() else {
            return Err(owner.abort_with(anyhow::anyhow!("marker-pair child stdout was not piped")));
        };
        let (ready_sender, ready) = channel();
        let (completion_sender, completion) = channel();
        let reader = std::thread::Builder::new()
            .name("marker-pair-child-stdout".to_owned())
            .spawn(move || {
                let mut reader = std::io::BufReader::new(stdout);
                let readiness = read_child_ready(&mut reader);
                drop(ready_sender.send(readiness));
                let drained = std::io::copy(&mut reader, &mut std::io::sink());
                drop(completion_sender.send(drained));
            });
        match reader {
            Ok(_) => owner.stdout_completion = Some(completion),
            Err(error) => {
                let error = anyhow::Error::from(error).context("spawn marker-pair stdout reader");
                return Err(owner.abort_with(error));
            }
        }
        let line = match ready.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => {
                let error = anyhow::Error::from(error).context("read marker-pair child readiness");
                return Err(owner.abort_with(error));
            }
            Err(error) => {
                let error = anyhow::Error::from(error)
                    .context("marker-pair child readiness deadline expired");
                return Err(owner.abort_with(error));
            }
        };
        let Some(reported) = line.strip_prefix(CHILD_READY_PREFIX) else {
            return Err(owner.abort_with(anyhow::anyhow!(
                "marker-pair child did not report lock readiness"
            )));
        };
        let reported = match reported.trim().parse::<u32>() {
            Ok(reported) => reported,
            Err(error) => {
                let error = anyhow::Error::from(error)
                    .context("marker-pair child readiness did not contain a PID");
                return Err(owner.abort_with(error));
            }
        };
        if reported != child_id {
            return Err(owner.abort_with(anyhow::anyhow!(
                "marker-pair readiness PID did not match the owned child"
            )));
        }
        Ok((owner, reported))
    }

    pub(super) fn release(mut self) -> anyhow::Result<()> {
        let Some(mut stdin) = self.stdin.take() else {
            return Err(self.abort_with(anyhow::anyhow!("marker-pair child stdin was not piped")));
        };
        if let Err(error) = writeln!(stdin, "{CHILD_RELEASE_COMMAND}") {
            drop(stdin);
            return Err(self.abort_with(error.into()));
        }
        if let Err(error) = stdin.flush() {
            drop(stdin);
            return Err(self.abort_with(error.into()));
        }
        drop(stdin);
        let status = match wait_child_bounded(&mut self.child, Duration::from_secs(10)) {
            Ok(status) => status,
            Err(error) => {
                let error = anyhow::Error::from(error)
                    .context("marker-pair child release did not finish before its deadline");
                return Err(self.abort_with(error));
            }
        };
        self.termination_attempted = true;
        let child_error = if status.success() {
            None
        } else {
            Some(anyhow::anyhow!("marker-pair child failed: {status}"))
        };
        let drain_result = self.confirm_stdout_drained();
        match (child_error, drain_result) {
            (None, Ok(())) => Ok(()),
            (None, Err(drain_error)) => Err(drain_error),
            (Some(child_error), Ok(())) => Err(child_error),
            (Some(child_error), Err(drain_error)) => Err(child_error.context(format!(
                "marker-pair stdout drain also failed: {drain_error:#}"
            ))),
        }
    }

    fn wait_for_stdout(&mut self, timeout: Duration) -> anyhow::Result<()> {
        let result = match self.stdout_completion.as_ref() {
            Some(completion) => completion.recv_timeout(timeout),
            None => return Ok(()),
        };
        match result {
            Ok(Ok(_)) => {
                self.stdout_completion.take();
                Ok(())
            }
            Ok(Err(error)) => {
                self.stdout_completion.take();
                Err(anyhow::Error::from(error).context("drain marker-pair child stdout"))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(anyhow::anyhow!(
                "marker-pair child stdout drain did not finish before its deadline"
            )),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                self.stdout_completion.take();
                Err(anyhow::anyhow!(
                    "marker-pair stdout reader exited without reporting completion"
                ))
            }
        }
    }

    fn confirm_stdout_drained(&mut self) -> anyhow::Result<()> {
        match self.wait_for_stdout(Duration::from_secs(2)) {
            Ok(()) => Ok(()),
            Err(error) => Err(self.preserve_unresolved(error)),
        }
    }

    fn abort_with(&mut self, cause: anyhow::Error) -> anyhow::Error {
        match self.terminate_and_reap() {
            Ok(()) => cause,
            Err(cleanup_error) => cause.context(format!(
                "marker-pair child failed and termination/reap was unresolved: {cleanup_error:#}"
            )),
        }
    }

    pub(super) fn terminate_and_reap(&mut self) -> anyhow::Result<()> {
        if self.termination_attempted {
            return Ok(());
        }
        self.termination_attempted = true;
        drop(self.stdin.take());

        let initial_check = match self.child.try_wait() {
            Ok(Some(_)) => return self.confirm_stdout_drained(),
            Ok(None) => None,
            Err(error) => Some(error),
        };
        let kill_error = self.child.kill().err();
        match wait_child_bounded(&mut self.child, Duration::from_secs(2)) {
            Ok(_) => self.confirm_stdout_drained(),
            Err(reap_error) => {
                let mut cleanup_error = anyhow::Error::from(reap_error)
                    .context("bounded marker-pair child reap did not confirm exit");
                if let Some(error) = kill_error {
                    cleanup_error = cleanup_error.context(format!(
                        "terminating owned marker-pair child {} failed: {error}",
                        self.child.id()
                    ));
                }
                if let Some(error) = initial_check {
                    cleanup_error = cleanup_error.context(format!(
                        "initial status check for owned marker-pair child {} failed: {error}",
                        self.child.id()
                    ));
                }
                Err(self.preserve_unresolved(cleanup_error))
            }
        }
    }

    fn preserve_unresolved(&self, cleanup_error: anyhow::Error) -> anyhow::Error {
        let cleanup_error = cleanup_error.context(format!(
            "owned marker-pair child pid={} root={}",
            self.child.id(),
            self.root.display()
        ));
        let details = format!(
            "owned child pid={} root={} cleanup={cleanup_error:#}",
            self.child.id(),
            self.root.display()
        );
        match preserve_root_before_evidence(&self.preserve_root, || {
            write_child_cleanup_evidence(&self.root, self.child.id(), &details)
        }) {
            Ok(()) => cleanup_error,
            Err(evidence_error) => cleanup_error.context(format!(
                "fixture root {} was retained for owned child {}, but unresolved-child evidence could not be written: {evidence_error}",
                self.root.display(),
                self.child.id()
            )),
        }
    }
}

fn preserve_root_before_evidence(
    preserve_root: &AtomicBool,
    write_evidence: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    preserve_root.store(true, Ordering::SeqCst);
    write_evidence()
}

fn write_child_cleanup_evidence(root: &Path, child_id: u32, details: &str) -> std::io::Result<()> {
    let evidence_directory = root.parent().unwrap_or(root);
    let evidence_path =
        evidence_directory.join(format!("marker-pair-unresolved-child-{child_id}.txt"));
    let descriptor = rustix::fs::openat(
        rustix::fs::CWD,
        &evidence_path,
        rustix::fs::OFlags::WRONLY
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::EXCL
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::from_raw_mode(0o600),
    )?;
    let mut evidence = std::fs::File::from(descriptor);
    writeln!(evidence, "{details}")?;
    evidence.sync_all()
}

fn read_child_ready(
    reader: &mut std::io::BufReader<std::process::ChildStdout>,
) -> std::io::Result<String> {
    for _ in 0..16 {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        if line.starts_with(CHILD_READY_PREFIX) {
            return Ok(line);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        "marker-pair child closed stdout without its readiness record",
    ))
}

impl Drop for MarkerPairChild {
    fn drop(&mut self) {
        if let Err(error) = self.terminate_and_reap() {
            drop(writeln!(std::io::stderr().lock(), "{error:#}"));
        }
    }
}

fn wait_child_bounded(child: &mut Child, timeout: Duration) -> std::io::Result<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "marker-pair child did not exit before its deadline",
            ));
        }
        std::thread::yield_now();
    }
}

#[test]
#[ignore = "invoked only by the bounded parent-process marker-pair fixture"]
fn marker_pair_child_fixture() {
    let root = std::env::var_os(CHILD_ROOT_ENV).expect("child root is required");
    let root = PathBuf::from(root);
    let command = std::env::var(CHILD_COMMAND_ENV).unwrap_or_else(|_| "hold".to_owned());
    if command == "hold" {
        let lease = MarkerPairLease::acquire(&root).unwrap();
        println!("{CHILD_READY_PREFIX}{}", std::process::id());
        std::io::stdout().flush().unwrap();
        wait_for_child_release();
        drop(lease);
        return;
    }

    let command = MarkerPairTransactionChildCommand::from_str(&command)
        .expect("child transaction command must be in the fixed command set");
    if matches!(
        command,
        MarkerPairTransactionChildCommand::RecoverInFreshChild
    ) {
        println!("{CHILD_READY_PREFIX}{}", std::process::id());
        std::io::stdout().flush().unwrap();
        wait_for_child_release();
        let lease = marker_transaction::shared_lease_with_recovery(&root)
            .unwrap()
            .expect("owned recovery child root is present");
        drop(lease);
        return;
    }

    let lease = MarkerPairLease::acquire(&root).unwrap();
    let point = command
        .point()
        .expect("publisher command has one fixed pause point");
    let _pause = marker_transaction::pause_at_for_test(point);
    let result = marker_transaction::publish_locked(
        &lease,
        b"1111111111111111111111111111111111111111111111111111111111111111",
        b"2222222222222222222222222222222222222222222222222222222222222222",
        || Ok(()),
    );
    if matches!(
        command,
        MarkerPairTransactionChildCommand::PauseAfterPreparedExpectingPreflightRefusal
    ) {
        let error =
            result.expect_err("prepared publication should reject the tampered preflight input");
        assert_eq!(
            marker_transaction::publication_failure_kind(&error),
            Some(marker_transaction::PublicationFailureKind::PreparedRollbackPending),
            "preflight refusal must preserve the Prepared transaction and original rollback error"
        );
    } else {
        result.unwrap();
    }
}

fn wait_for_child_release() {
    let stdin = std::io::stdin();
    let mut command = String::new();
    std::io::BufReader::new(stdin.lock())
        .read_line(&mut command)
        .unwrap();
    assert_eq!(command.trim_end(), CHILD_RELEASE_COMMAND);
}

#[test]
fn marker_pair_independent_opens_are_nonblocking_and_drop_releases() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("container");
    let (config, admitted) = admitted_fingerprint_fixture();
    let _paths = prepare_marker_root(&root, &config, &admitted);

    let held = MarkerPairLease::acquire(&root).unwrap();
    assert!(MarkerPairLease::acquire(&root).is_err());

    let other_root = temp.path().join("independent-container");
    prepare_marker_root(&other_root, &config, &admitted);
    let other = MarkerPairLease::acquire(&other_root).unwrap();
    drop(other);

    drop(held);
    let reclaimed = MarkerPairLease::acquire(&root).unwrap();
    drop(reclaimed);
}

#[test]
fn marker_pair_shared_readers_allow_matchers_and_hold_off_publishers() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("container");
    let (base, admitted) = admitted_fingerprint_fixture();
    let old_config = with_environment(&base, "MARKER_PAIR_SHARED_READERS", "old");
    let paths = prepare_marker_root(&root, &old_config, &admitted);
    let old_revision = AccountConfigRevision::acquire(&paths).unwrap();
    record_pair(
        &root,
        &paths,
        &old_revision,
        &old_config,
        &old_config,
        &admitted,
    )
    .unwrap();
    drop(old_revision);

    let mut new_config = with_environment(&base, "MARKER_PAIR_SHARED_READERS", "new");
    new_config
        .agent_configurations
        .get_mut("b-instance")
        .unwrap()
        .model = Some("marker-pair-shared-readers-new-model".to_owned());
    let old_fingerprint =
        account_configuration_fingerprint(&old_config, None, TEST_ROLE, &admitted).unwrap();
    let new_fingerprint =
        account_configuration_fingerprint(&new_config, None, TEST_ROLE, &admitted).unwrap();
    assert_ne!(old_fingerprint, new_fingerprint);
    persist_config(&paths, &new_config);
    let new_revision = AccountConfigRevision::acquire(&paths).unwrap();
    let before = marker_bytes(&root);

    let first_reader = MarkerPairLease::acquire_optional(&root).unwrap().unwrap();
    let second_reader = MarkerPairLease::acquire_optional(&root).unwrap().unwrap();
    assert!(account_configuration_matches(&root, &old_config, None, TEST_ROLE).unwrap());
    assert!(account_admission_matches(&root, &old_config, None, TEST_ROLE).unwrap());
    assert!(
        record_pair(
            &root,
            &paths,
            &new_revision,
            &new_config,
            &new_config,
            &admitted,
        )
        .is_err()
    );
    assert_eq!(marker_bytes(&root), before);

    drop(first_reader);
    assert!(account_configuration_matches(&root, &old_config, None, TEST_ROLE).unwrap());
    assert!(account_admission_matches(&root, &old_config, None, TEST_ROLE).unwrap());
    assert!(
        record_pair(
            &root,
            &paths,
            &new_revision,
            &new_config,
            &new_config,
            &admitted,
        )
        .is_err()
    );
    assert_eq!(marker_bytes(&root), before);

    drop(second_reader);
    record_pair(
        &root,
        &paths,
        &new_revision,
        &new_config,
        &new_config,
        &admitted,
    )
    .unwrap();
    assert!(!account_configuration_matches(&root, &old_config, None, TEST_ROLE).unwrap());
    assert!(!account_admission_matches(&root, &old_config, None, TEST_ROLE).unwrap());
    assert!(account_configuration_matches(&root, &new_config, None, TEST_ROLE).unwrap());
    assert!(account_admission_matches(&root, &new_config, None, TEST_ROLE).unwrap());
    assert_eq!(
        marker_bytes(&root),
        (
            new_fingerprint.as_bytes().to_vec(),
            new_fingerprint.as_bytes().to_vec()
        )
    );
    assert_ne!(marker_bytes(&root), before);
}

#[test]
fn marker_pair_child_contention_rejects_all_operations_until_release() {
    let temp = MarkerPairTestRoot::new().unwrap();
    let root = temp.path().join("container");
    let (config, admitted) = admitted_fingerprint_fixture();
    let paths = prepare_marker_root(&root, &config, &admitted);
    let revision = AccountConfigRevision::acquire(&paths).unwrap();
    record_pair(&root, &paths, &revision, &config, &config, &admitted).unwrap();
    let before = marker_bytes(&root);

    let (child, child_id) = MarkerPairChild::spawn(&root, temp.preservation_flag()).unwrap();
    assert_ne!(child_id, std::process::id());
    drop(account_configuration_matches(&root, &config, None, TEST_ROLE).unwrap_err());
    drop(account_admission_matches(&root, &config, None, TEST_ROLE).unwrap_err());
    assert!(record_pair(&root, &paths, &revision, &config, &config, &admitted).is_err());
    assert_eq!(marker_bytes(&root), before);

    let other_root = temp.path().join("independent-container");
    prepare_marker_root(&other_root, &config, &admitted);
    let independent = MarkerPairLease::acquire(&other_root).unwrap();
    drop(independent);

    child.release().unwrap();
    let reclaimed = MarkerPairLease::acquire(&root).unwrap();
    drop(reclaimed);
    assert!(account_configuration_matches(&root, &config, None, TEST_ROLE).unwrap());
    assert!(account_admission_matches(&root, &config, None, TEST_ROLE).unwrap());
    record_pair(&root, &paths, &revision, &config, &config, &admitted).unwrap();
}

#[test]
fn unresolved_child_preserves_root_before_evidence_sink_failure() {
    let test_root = MarkerPairTestRoot::new().unwrap();
    let retained_path = test_root.path().to_path_buf();
    let preserve_root = test_root.preservation_flag();
    let error = preserve_root_before_evidence(&preserve_root, || {
        assert!(preserve_root.load(Ordering::SeqCst));
        Err(std::io::Error::other("injected evidence sink failure"))
    })
    .unwrap_err();

    assert_eq!(error.to_string(), "injected evidence sink failure");
    assert!(preserve_root.load(Ordering::SeqCst));
    drop(test_root);
    assert!(
        retained_path.exists(),
        "unresolved fixture root was removed"
    );
    std::fs::remove_dir_all(retained_path).unwrap();
}

#[test]
fn partial_marker_pair_refuses_both_matchers_until_publication_completes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("container");
    let (base, admitted) = admitted_fingerprint_fixture();
    let with_b_instance_model = |model: &str| {
        let mut config = base.clone();
        config
            .agent_configurations
            .get_mut("b-instance")
            .unwrap()
            .model = Some(model.into());
        config
    };
    let old_selected = with_b_instance_model("marker-pair-old-selected");
    let old_admission = with_b_instance_model("marker-pair-old-admission");
    let new_selected = with_b_instance_model("marker-pair-new-selected");
    let new_admission = with_b_instance_model("marker-pair-new-admission");
    let paths = prepare_marker_root(&root, &old_admission, &admitted);
    let old_revision = AccountConfigRevision::acquire(&paths).unwrap();
    record_pair(
        &root,
        &paths,
        &old_revision,
        &old_selected,
        &old_admission,
        &admitted,
    )
    .unwrap();
    drop(old_revision);

    let expected_old_selected =
        account_configuration_fingerprint(&old_selected, None, TEST_ROLE, &admitted).unwrap();
    let expected_old_admission =
        account_configuration_fingerprint(&old_admission, None, TEST_ROLE, &admitted).unwrap();
    let expected_new_selected =
        account_configuration_fingerprint(&new_selected, None, TEST_ROLE, &admitted).unwrap();
    let expected_new_admission =
        account_configuration_fingerprint(&new_admission, None, TEST_ROLE, &admitted).unwrap();
    assert_ne!(expected_old_selected, expected_new_selected);
    assert_ne!(expected_old_admission, expected_new_admission);
    assert_ne!(expected_old_selected, expected_old_admission);
    assert_ne!(expected_new_selected, expected_new_admission);

    persist_config(&paths, &new_admission);
    let new_revision = AccountConfigRevision::acquire(&paths).unwrap();
    let observed_pause = Arc::new(AtomicBool::new(false));
    let observed_in_hook = Arc::clone(&observed_pause);
    let root_for_hook = root.clone();
    let new_selected_for_hook = new_selected.clone();
    let new_admission_for_hook = new_admission.clone();
    let expected_new_selected_for_hook = expected_new_selected.clone();
    let expected_old_admission_for_hook = expected_old_admission.clone();
    let hook = marker_pair::pause_after_selected_rename_once(move |observed_root| {
        assert_eq!(observed_root, root_for_hook);
        assert_eq!(
            std::fs::read(observed_root.join(ACCOUNT_FINGERPRINT_FILE)).unwrap(),
            expected_new_selected_for_hook.as_bytes()
        );
        assert_eq!(
            std::fs::read(observed_root.join(ACCOUNT_ADMISSION_FILE)).unwrap(),
            expected_old_admission_for_hook.as_bytes()
        );
        drop(
            account_configuration_matches(observed_root, &new_selected_for_hook, None, TEST_ROLE)
                .unwrap_err(),
        );
        drop(
            account_admission_matches(observed_root, &new_admission_for_hook, None, TEST_ROLE)
                .unwrap_err(),
        );
        observed_in_hook.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });
    record_pair(
        &root,
        &paths,
        &new_revision,
        &new_selected,
        &new_admission,
        &admitted,
    )
    .unwrap();
    drop(hook);
    assert!(observed_pause.load(std::sync::atomic::Ordering::SeqCst));
    assert!(account_configuration_matches(&root, &new_selected, None, TEST_ROLE).unwrap());
    assert!(account_admission_matches(&root, &new_admission, None, TEST_ROLE).unwrap());
    assert_eq!(
        marker_bytes(&root),
        (
            expected_new_selected.as_bytes().to_vec(),
            expected_new_admission.as_bytes().to_vec()
        )
    );
    assert_ne!(expected_new_selected, expected_new_admission);
}

#[test]
fn marker_pair_generation_rotation_restores_both_previous_markers() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("container");
    let (base, admitted) = admitted_fingerprint_fixture();
    let old_config = with_environment(&base, "MARKER_PAIR_GENERATION", "old");
    let paths = prepare_marker_root(&root, &old_config, &admitted);
    let old_revision = AccountConfigRevision::acquire(&paths).unwrap();
    record_pair(
        &root,
        &paths,
        &old_revision,
        &old_config,
        &old_config,
        &admitted,
    )
    .unwrap();
    drop(old_revision);
    let before = marker_bytes(&root);

    let selected = with_environment(&base, "MARKER_PAIR_SELECTED", "new-selected");
    let admission = with_environment(&base, "MARKER_PAIR_ADMISSION", "new-admission");
    persist_config(&paths, &admission);
    let revision = AccountConfigRevision::acquire(&paths).unwrap();
    let rotated = with_environment(&base, "MARKER_PAIR_GENERATION", "direct-rotation");
    let rotated_path = paths.config_file.clone();
    let hook = marker_pair::pause_after_selected_rename_once(move |_| {
        std::fs::write(rotated_path, toml::to_string(&rotated)?)?;
        Ok(())
    });
    let error =
        record_pair(&root, &paths, &revision, &selected, &admission, &admitted).unwrap_err();
    drop(hook);

    assert!(
        error
            .to_string()
            .contains("configuration changed during launch"),
        "unexpected generation-rotation error: {error:#}"
    );
    assert_eq!(marker_bytes(&root), before);
    assert_no_marker_pair_temporary_files(&root);
}

fn assert_no_marker_pair_temporary_files(root: &Path) {
    let temporary: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name.starts_with(".account-config.sha256.")
                || name.starts_with(".account-admission.sha256.")
        })
        .collect();
    assert!(
        temporary.is_empty(),
        "marker-pair temporary files remain: {temporary:?}"
    );
}

#[test]
fn marker_root_modes_and_absence_preserve_decision_contracts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("container");
    let (config, admitted) = admitted_fingerprint_fixture();
    let paths = prepare_marker_root(&root, &config, &admitted);

    for mode in [0o700, 0o755] {
        set_directory_mode(&root, mode);
        let lease = MarkerPairLease::acquire(&root).unwrap();
        drop(lease);
    }
    for mode in [0o770, 0o702] {
        set_directory_mode(&root, mode);
        assert!(MarkerPairLease::acquire(&root).is_err());
    }
    set_directory_mode(&root, 0o700);

    let missing = temp.path().join("missing-container");
    assert!(
        MarkerPairLease::acquire_optional(&missing)
            .unwrap()
            .is_none()
    );
    assert!(!account_configuration_matches(&missing, &config, None, TEST_ROLE).unwrap());
    assert!(!account_admission_matches(&missing, &config, None, TEST_ROLE).unwrap());
    let revision = AccountConfigRevision::acquire(&paths).unwrap();
    assert!(record_pair(&missing, &paths, &revision, &config, &config, &admitted).is_err());
    assert!(!missing.exists());
}

#[test]
fn marker_root_symlink_and_replacement_fail_without_touching_replacement() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("container");
    let (config, admitted) = admitted_fingerprint_fixture();
    let paths = prepare_marker_root(&root, &config, &admitted);
    let revision = AccountConfigRevision::acquire(&paths).unwrap();
    record_pair(&root, &paths, &revision, &config, &config, &admitted).unwrap();
    let baseline = snapshot_tree(&root);
    let symlink_root = temp.path().join("container-link");
    std::os::unix::fs::symlink(&root, &symlink_root).unwrap();
    drop(account_configuration_matches(&symlink_root, &config, None, TEST_ROLE).unwrap_err());
    drop(account_admission_matches(&symlink_root, &config, None, TEST_ROLE).unwrap_err());
    assert!(
        record_pair(
            &symlink_root,
            &paths,
            &revision,
            &config,
            &config,
            &admitted
        )
        .is_err()
    );
    assert_eq!(snapshot_tree(&root), baseline);
    drop(revision);

    let replacement = temp.path().join("replacement-container");
    let original_identity = std::fs::metadata(&root).unwrap();
    let new_config = with_environment(&config, "MARKER_PAIR_ROOT", "new-root");
    persist_config(&paths, &new_config);
    let new_revision = AccountConfigRevision::acquire(&paths).unwrap();
    let new_named_root_snapshot = Arc::new(std::sync::Mutex::new(None));
    let root_for_hook = root.clone();
    let replacement_for_hook = replacement.clone();
    let snapshot_for_hook = Arc::clone(&new_named_root_snapshot);
    let hook = marker_pair::pause_after_selected_rename_once(move |observed_root| {
        assert_eq!(observed_root, root_for_hook);
        std::fs::rename(&root_for_hook, &replacement_for_hook)?;
        std::fs::create_dir(&root_for_hook)?;
        set_directory_mode(&root_for_hook, 0o700);
        std::fs::write(
            root_for_hook.join("replacement-sentinel"),
            b"preserve replacement",
        )?;
        *snapshot_for_hook.lock().unwrap() = Some(snapshot_tree(&root_for_hook));
        Ok(())
    });
    let error = record_pair(
        &root,
        &paths,
        &new_revision,
        &new_config,
        &new_config,
        &admitted,
    )
    .unwrap_err();
    drop(hook);
    assert!(error.to_string().contains("identity changed"), "{error:#}");

    let retained_identity = std::fs::metadata(&replacement).unwrap();
    assert_eq!(original_identity.dev(), retained_identity.dev());
    assert_eq!(original_identity.ino(), retained_identity.ino());
    assert_eq!(
        std::fs::read(root.join("replacement-sentinel")).unwrap(),
        b"preserve replacement"
    );
    assert!(!root.join(ACCOUNT_FINGERPRINT_FILE).exists());
    assert!(!root.join(ACCOUNT_ADMISSION_FILE).exists());
    assert_eq!(
        std::fs::read(replacement.join(ACCOUNT_FINGERPRINT_FILE)).unwrap(),
        account_configuration_fingerprint(&new_config, None, TEST_ROLE, &admitted)
            .unwrap()
            .as_bytes()
    );
    assert_eq!(
        std::fs::read(replacement.join(ACCOUNT_ADMISSION_FILE)).unwrap(),
        account_configuration_fingerprint(&config, None, TEST_ROLE, &admitted)
            .unwrap()
            .as_bytes()
    );

    assert_retained_prepared_custody(&replacement, &original_identity, &new_config, &admitted);

    let observed_new_named_root = new_named_root_snapshot
        .lock()
        .unwrap()
        .take()
        .expect("the hook captured the newly named replacement root");
    assert_eq!(snapshot_tree(&root), observed_new_named_root);
}

#[derive(Clone, Copy)]
enum ReservedJournalKind {
    Regular,
    Directory,
    Symlink,
    Socket,
}

fn install_reserved_journal(
    root: &Path,
    external: &Path,
    kind: ReservedJournalKind,
) -> Option<UnixListener> {
    let journal = root.join(ACCOUNT_MARKERS_TRANSACTION_FILE);
    match kind {
        ReservedJournalKind::Regular => {
            std::fs::write(&journal, b"prepared evidence").unwrap();
            None
        }
        ReservedJournalKind::Directory => {
            std::fs::create_dir(&journal).unwrap();
            std::fs::write(journal.join("record"), b"prepared evidence").unwrap();
            None
        }
        ReservedJournalKind::Symlink => {
            std::fs::write(external, b"outside evidence").unwrap();
            std::os::unix::fs::symlink(external, &journal).unwrap();
            None
        }
        ReservedJournalKind::Socket => Some(UnixListener::bind(journal).unwrap()),
    }
}

#[test]
fn reserved_journal_kinds_fail_closed_without_changing_evidence() {
    for (index, kind) in [
        ReservedJournalKind::Regular,
        ReservedJournalKind::Directory,
        ReservedJournalKind::Symlink,
        ReservedJournalKind::Socket,
    ]
    .into_iter()
    .enumerate()
    {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join(format!("container-{index}"));
        let external = temp.path().join(format!("external-{index}"));
        let (config, admitted) = admitted_fingerprint_fixture();
        let paths = prepare_marker_root(&root, &config, &admitted);
        let revision = AccountConfigRevision::acquire(&paths).unwrap();
        record_pair(&root, &paths, &revision, &config, &config, &admitted).unwrap();
        let _socket = install_reserved_journal(&root, &external, kind);
        let before_root = snapshot_tree(&root);
        let before_external = if external.exists() {
            Some(snapshot_tree(&external))
        } else {
            None
        };

        drop(account_configuration_matches(&root, &config, None, TEST_ROLE).unwrap_err());
        assert_eq!(snapshot_tree(&root), before_root);
        drop(account_admission_matches(&root, &config, None, TEST_ROLE).unwrap_err());
        assert_eq!(snapshot_tree(&root), before_root);
        assert!(record_pair(&root, &paths, &revision, &config, &config, &admitted).is_err());
        assert_eq!(snapshot_tree(&root), before_root);
        if let Some(before_external) = before_external {
            assert_eq!(snapshot_tree(&external), before_external);
        }
    }
}

fn assert_retained_prepared_custody(
    replacement: &Path,
    original_identity: &std::fs::Metadata,
    new_config: &AppConfig,
    admitted: &[AdmittedInstance],
) {
    let journal_path = replacement.join(ACCOUNT_MARKERS_TRANSACTION_FILE);
    let journal_metadata = std::fs::symlink_metadata(&journal_path).unwrap();
    assert!(journal_metadata.file_type().is_file() && !journal_metadata.file_type().is_symlink());
    assert_eq!(journal_metadata.permissions().mode() & 0o7777, 0o600);
    assert_eq!(journal_metadata.uid(), rustix::process::geteuid().as_raw());
    assert_eq!(journal_metadata.nlink(), 1);
    assert_eq!(journal_metadata.dev(), original_identity.dev());
    let prepared: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
    assert_eq!(prepared["phase"].as_str(), Some("prepared"));
    assert_eq!(prepared["schema_version"].as_u64(), Some(2));
    assert_eq!(
        prepared["root"]["device"].as_u64(),
        Some(original_identity.dev())
    );
    assert_eq!(
        prepared["root"]["inode"].as_u64(),
        Some(original_identity.ino())
    );
    let transaction_id = prepared["transaction_id"].as_str().unwrap();
    assert_eq!(transaction_id.len(), 48);
    assert!(
        transaction_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );

    let selected_stage_name = format!(".account-markers-{transaction_id}-selected");
    let admission_stage_name = format!(".account-markers-{transaction_id}-admission");
    assert_eq!(
        prepared["selected"]["stage_name"].as_str(),
        Some(selected_stage_name.as_str())
    );
    assert_eq!(
        prepared["admission"]["stage_name"].as_str(),
        Some(admission_stage_name.as_str())
    );

    let assert_recorded_file = |path: &Path, recorded: &serde_json::Value| {
        let metadata = std::fs::symlink_metadata(path).unwrap();
        assert!(metadata.file_type().is_file() && !metadata.file_type().is_symlink());
        assert_eq!(metadata.permissions().mode() & 0o7777, 0o600);
        assert_eq!(metadata.dev(), original_identity.dev());
        assert_eq!(metadata.uid(), rustix::process::geteuid().as_raw());
        assert_eq!(metadata.nlink(), 1);
        assert_eq!(recorded["device"].as_u64(), Some(metadata.dev()));
        assert_eq!(recorded["inode"].as_u64(), Some(metadata.ino()));
        assert_eq!(recorded["owner"].as_u64(), Some(u64::from(metadata.uid())));
        assert_eq!(recorded["links"].as_u64(), Some(metadata.nlink()));
        assert_eq!(recorded["mode"].as_u64(), Some(0o600));
        assert_eq!(recorded["size"].as_u64(), Some(metadata.len()));
    };

    let selected_path = replacement.join(ACCOUNT_FINGERPRINT_FILE);
    assert_recorded_file(&selected_path, &prepared["selected"]["stage"]);
    assert!(matches!(
        std::fs::symlink_metadata(replacement.join(&selected_stage_name)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    ));

    let admission_stage_path = replacement.join(&admission_stage_name);
    assert_recorded_file(&admission_stage_path, &prepared["admission"]["stage"]);
    let expected_admission_bytes =
        account_configuration_fingerprint(new_config, None, TEST_ROLE, admitted)
            .unwrap()
            .as_bytes()
            .to_vec();
    let recorded_admission_bytes: Vec<u8> =
        serde_json::from_value(prepared["admission"]["new_bytes"].clone()).unwrap();
    assert_eq!(recorded_admission_bytes, expected_admission_bytes);
    assert_eq!(
        std::fs::read(&admission_stage_path).unwrap(),
        expected_admission_bytes
    );

    let candidate_name = ".account-markers-transaction.next";
    assert_eq!(prepared["candidate"]["name"].as_str(), Some(candidate_name));
    let candidate_path = replacement.join(candidate_name);
    assert_recorded_file(&candidate_path, &prepared["candidate"]["file"]);
    let candidate_bytes = std::fs::read(&candidate_path).unwrap();
    let committed: serde_json::Value = serde_json::from_slice(&candidate_bytes).unwrap();
    assert_eq!(committed["phase"].as_str(), Some("committed"));
    assert_eq!(committed["schema_version"].as_u64(), Some(2));
    assert_eq!(committed["transaction_id"], prepared["transaction_id"]);
    assert_eq!(committed["root"], prepared["root"]);
    assert_eq!(
        committed["selected"]["new_bytes"],
        prepared["selected"]["new_bytes"]
    );
    assert_eq!(
        committed["admission"]["new_bytes"],
        prepared["admission"]["new_bytes"]
    );
    assert_eq!(
        committed["selected"]["installed"],
        prepared["selected"]["stage"]
    );
    assert_eq!(
        committed["admission"]["installed"],
        prepared["admission"]["stage"]
    );
}

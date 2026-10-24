// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::*;

use std::io::Write as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const ROOT_NAME: &str = "jk-marker-root-fixture";
const CHILD_TEST_ID: &str = "tests::case_04::prepare_for_bindings_root_child";
const CHILD_JACKIN_HOME_ENV: &str = "JACKIN_ROLES_CHILD_JACKIN_HOME";
const CHILD_UMASK_ENV: &str = "JACKIN_ROLES_CHILD_UMASK";

struct FixtureRoot {
    directory: Option<tempfile::TempDir>,
    preserve: Arc<AtomicBool>,
}

impl FixtureRoot {
    fn new() -> std::io::Result<Self> {
        Ok(Self {
            directory: Some(tempdir()?),
            preserve: Arc::new(AtomicBool::new(false)),
        })
    }

    fn path(&self) -> &Path {
        self.directory
            .as_ref()
            .expect("fixture root exists before drop")
            .path()
    }

    fn preservation_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.preserve)
    }
}

impl Drop for FixtureRoot {
    fn drop(&mut self) {
        if self.preserve.load(Ordering::Acquire)
            && let Some(directory) = self.directory.take()
        {
            let preserved_path = directory.keep();
            drop(writeln!(
                std::io::stderr().lock(),
                "retained role-root fixture at {} after child cleanup failure",
                preserved_path.display()
            ));
        }
    }
}

struct OwnedChild {
    child: Option<Child>,
    child_id: u32,
    jackin_home: PathBuf,
    preserve_root: Arc<AtomicBool>,
    termination_attempted: bool,
}

impl OwnedChild {
    fn spawn(
        jackin_home: &Path,
        umask: &str,
        preserve_root: Arc<AtomicBool>,
    ) -> anyhow::Result<Self> {
        let child = Command::new(std::env::current_exe()?)
            .args(["--exact", CHILD_TEST_ID, "--ignored", "--nocapture"])
            .env(CHILD_JACKIN_HOME_ENV, jackin_home)
            .env(CHILD_UMASK_ENV, umask)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        let child_id = child.id();
        Ok(Self {
            child: Some(child),
            child_id,
            jackin_home: jackin_home.to_path_buf(),
            preserve_root,
            termination_attempted: false,
        })
    }

    fn wait(mut self, timeout: Duration) -> anyhow::Result<ExitStatus> {
        let result = wait_for_child(
            self.child
                .as_mut()
                .expect("owned child is present until wait completes"),
            timeout,
        );
        match result {
            Ok(status) => {
                self.termination_attempted = true;
                self.child.take();
                Ok(status)
            }
            Err(error) => {
                let cause = anyhow::Error::from(error).context(format!(
                    "waiting for owned role-root child pid={} home={}",
                    self.child_id,
                    self.jackin_home.display()
                ));
                Err(self.abort_with(cause))
            }
        }
    }

    fn abort_with(&mut self, cause: anyhow::Error) -> anyhow::Error {
        match self.terminate_and_reap() {
            Ok(()) => cause,
            Err(cleanup_error) => cause.context(format!(
                "owned role-root child termination/reap was unresolved: {cleanup_error:#}"
            )),
        }
    }

    fn terminate_and_reap(&mut self) -> anyhow::Result<()> {
        if self.termination_attempted {
            return Ok(());
        }
        self.termination_attempted = true;

        if self.child.is_none() {
            return Ok(());
        }
        let initial_error = match self
            .child
            .as_mut()
            .expect("owned child was checked above")
            .try_wait()
        {
            Ok(Some(_)) => {
                self.child.take();
                return Ok(());
            }
            Ok(None) => None,
            Err(error) => Some(error),
        };
        let kill_error = self.child.as_mut().and_then(|child| child.kill().err());
        let reap_result = wait_for_child(
            self.child.as_mut().expect("unreaped child remains owned"),
            Duration::from_secs(2),
        );
        match reap_result {
            Ok(_) => {
                self.child.take();
                Ok(())
            }
            Err(reap_error) => {
                let mut cleanup_error = anyhow::Error::from(reap_error).context(format!(
                    "bounded reap failed for owned role-root child pid={} home={}",
                    self.child_id,
                    self.jackin_home.display()
                ));
                if let Some(error) = kill_error {
                    cleanup_error = cleanup_error.context(format!(
                        "terminating owned role-root child {} failed: {error}",
                        self.child_id
                    ));
                }
                if let Some(error) = initial_error {
                    cleanup_error = cleanup_error.context(format!(
                        "initial status check for owned role-root child {} failed: {error}",
                        self.child_id
                    ));
                }
                Err(self.preserve_unresolved(cleanup_error))
            }
        }
    }

    fn preserve_unresolved(&self, cleanup_error: anyhow::Error) -> anyhow::Error {
        let cleanup_error = cleanup_error.context(format!(
            "owned role-root child pid={} home={}",
            self.child_id,
            self.jackin_home.display()
        ));
        let details = format!(
            "owned child pid={} home={} cleanup={cleanup_error:#}",
            self.child_id,
            self.jackin_home.display()
        );
        match preserve_root_before_evidence(&self.preserve_root, || {
            write_child_cleanup_evidence(&self.jackin_home, self.child_id, &details)
        }) {
            Ok(()) => cleanup_error,
            Err(evidence_error) => cleanup_error.context(format!(
                "fixture root was retained for owned child {}, but cleanup evidence failed: {evidence_error}",
                self.child_id
            )),
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Err(error) = self.terminate_and_reap() {
            drop(writeln!(std::io::stderr().lock(), "{error:#}"));
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

fn write_child_cleanup_evidence(home: &Path, child_id: u32, details: &str) -> std::io::Result<()> {
    let evidence_path = home.join(format!("jackin-roles-unresolved-child-{child_id}.txt"));
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

fn wait_for_child(child: &mut Child, timeout: Duration) -> std::io::Result<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "role-root child did not exit before its deadline",
            ));
        }
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

#[test]
fn unresolved_child_preserves_fixture_before_evidence_sink_failure() {
    let fixture = FixtureRoot::new().unwrap();
    let retained_path = fixture.path().to_path_buf();
    let preserve_root = fixture.preservation_flag();
    let error = preserve_root_before_evidence(&preserve_root, || {
        assert!(preserve_root.load(Ordering::SeqCst));
        Err(std::io::Error::other("injected evidence sink failure"))
    })
    .unwrap_err();

    assert_eq!(error.to_string(), "injected evidence sink failure");
    assert!(preserve_root.load(Ordering::SeqCst));
    drop(fixture);
    assert!(
        retained_path.exists(),
        "unresolved fixture root was removed"
    );
    std::fs::remove_dir_all(retained_path).unwrap();
}

fn paths_for_jackin_home(home: &Path) -> JackinPaths {
    JackinPaths::resolve_with_env(Path::new("/"), Some(home.as_os_str()), None)
}

fn directory_entries(path: &Path) -> Vec<String> {
    let mut entries = std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn prepare_ignore_state(
    paths: &JackinPaths,
    manifest: &RoleManifest,
    host_home: &Path,
) -> anyhow::Result<(RoleState, AuthProvisionOutcome)> {
    RoleState::prepare_for_bindings(
        paths,
        ROOT_NAME,
        manifest,
        &[],
        &GithubAuthContext {
            mode: GithubAuthMode::Ignore,
            token: None,
        },
        host_home,
        Agent::Claude,
    )
}

#[test]
fn prepare_for_bindings_creates_private_root_under_owned_child_umasks() {
    for (label, umask) in [("0002", "0002"), ("0000", "0000")] {
        let fixture = FixtureRoot::new().unwrap();
        let real_home = fixture.path().join(format!("real-home-{label}"));
        std::fs::create_dir(&real_home).unwrap();
        let alias_home = fixture.path().join(format!("home-alias-{label}"));
        symlink(&real_home, &alias_home).unwrap();

        let child = OwnedChild::spawn(&alias_home, umask, fixture.preservation_flag()).unwrap();
        let status = child.wait(Duration::from_secs(30)).unwrap();
        assert!(
            status.success(),
            "root creator child failed under umask {umask}: {status}"
        );

        let root = alias_home.join("data").join(ROOT_NAME);
        let metadata = std::fs::symlink_metadata(&root).unwrap();
        assert!(metadata.is_dir());
        assert!(!metadata.file_type().is_symlink());
        assert_eq!(metadata.uid(), rustix::process::geteuid().as_raw());
        assert_eq!(metadata.mode() & 0o777, 0o700, "umask {umask}");
        assert!(root.join("home").is_dir());
        assert!(root.join("state").is_dir());
    }
}

#[test]
fn prepare_for_bindings_preserves_safe_roots_through_symlinked_ancestors() {
    for mode in [0o700, 0o755] {
        let temp = tempdir().unwrap();
        let real_home = temp.path().join("real-home");
        let real_data = real_home.join("data");
        std::fs::create_dir_all(&real_data).unwrap();
        let alias_home = temp.path().join("home-alias");
        symlink(&real_home, &alias_home).unwrap();
        let root = real_data.join(ROOT_NAME);
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(mode)).unwrap();
        std::fs::write(root.join("sentinel"), b"keep existing root").unwrap();
        let before = std::fs::symlink_metadata(&root).unwrap();

        let manifest = simple_manifest(&temp);
        let (state, outcome) =
            prepare_ignore_state(&paths_for_jackin_home(&alias_home), &manifest, temp.path())
                .unwrap();

        assert_eq!(outcome, AuthProvisionOutcome::Skipped);
        assert_eq!(state.root, alias_home.join("data").join(ROOT_NAME));
        let after = std::fs::symlink_metadata(&root).unwrap();
        assert!(after.is_dir());
        assert!(!after.file_type().is_symlink());
        assert_eq!(after.dev(), before.dev());
        assert_eq!(after.ino(), before.ino());
        assert_eq!(after.mode() & 0o7777, mode);
        assert_eq!(
            std::fs::read(root.join("sentinel")).unwrap(),
            b"keep existing root"
        );
        assert_eq!(
            directory_entries(&root),
            vec!["home".to_owned(), "sentinel".to_owned(), "state".to_owned()]
        );
        assert!(root.join("home").is_dir());
        assert!(root.join("state").is_dir());
    }
}

#[test]
fn prepare_for_bindings_rejects_unsafe_roots_without_mutation() {
    for mode in [0o775, 0o777, 0o1777] {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let root = paths.data_dir.join(ROOT_NAME);
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(mode)).unwrap();
        std::fs::write(root.join("sentinel"), b"unsafe root evidence").unwrap();
        let before = std::fs::symlink_metadata(&root).unwrap();
        let manifest = simple_manifest(&temp);

        let Err(error) = prepare_ignore_state(&paths, &manifest, temp.path()) else {
            panic!("expected unsafe root preparation refusal");
        };
        drop(error);
        let after = std::fs::symlink_metadata(&root).unwrap();
        assert_eq!(after.dev(), before.dev());
        assert_eq!(after.ino(), before.ino());
        assert_eq!(after.mode() & 0o7777, mode);
        assert_eq!(
            std::fs::read(root.join("sentinel")).unwrap(),
            b"unsafe root evidence"
        );
        assert_eq!(directory_entries(&root), vec!["sentinel".to_owned()]);
        assert!(!root.join("home").exists());
        assert!(!root.join("state").exists());
        assert!(!root.join("credentials").exists());
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    let root = paths.data_dir.join(ROOT_NAME);
    let target = temp.path().join("root-target");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("sentinel"), b"symlink target evidence").unwrap();
    let target_before = std::fs::symlink_metadata(&target).unwrap();
    symlink(&target, &root).unwrap();
    let root_before = std::fs::symlink_metadata(&root).unwrap();
    let manifest = simple_manifest(&temp);

    let Err(error) = prepare_ignore_state(&paths, &manifest, temp.path()) else {
        panic!("expected unsafe root preparation refusal");
    };
    drop(error);
    let root_after = std::fs::symlink_metadata(&root).unwrap();
    let target_after = std::fs::symlink_metadata(&target).unwrap();
    assert!(root_after.file_type().is_symlink());
    assert_eq!(root_after.dev(), root_before.dev());
    assert_eq!(root_after.ino(), root_before.ino());
    assert_eq!(std::fs::read_link(&root).unwrap(), target);
    assert_eq!(target_after.dev(), target_before.dev());
    assert_eq!(target_after.ino(), target_before.ino());
    assert_eq!(
        std::fs::read(target.join("sentinel")).unwrap(),
        b"symlink target evidence"
    );
    assert_eq!(directory_entries(&target), vec!["sentinel".to_owned()]);
    assert!(!target.join("home").exists());
    assert!(!target.join("state").exists());

    let file_temp = tempdir().unwrap();
    let file_paths = JackinPaths::for_tests(file_temp.path());
    std::fs::create_dir_all(&file_paths.data_dir).unwrap();
    let file_root = file_paths.data_dir.join(ROOT_NAME);
    std::fs::write(&file_root, b"regular-file root evidence").unwrap();
    let file_before = std::fs::symlink_metadata(&file_root).unwrap();
    let file_manifest = simple_manifest(&file_temp);

    let Err(error) = prepare_ignore_state(&file_paths, &file_manifest, file_temp.path()) else {
        panic!("expected unsafe root preparation refusal");
    };
    drop(error);
    let file_after = std::fs::symlink_metadata(&file_root).unwrap();
    assert_eq!(file_after.dev(), file_before.dev());
    assert_eq!(file_after.ino(), file_before.ino());
    assert_eq!(
        std::fs::read(&file_root).unwrap(),
        b"regular-file root evidence"
    );
    assert!(!file_root.join("home").exists());
}

#[test]
#[ignore = "invoked only by the bounded parent-process root-mode fixture"]
fn prepare_for_bindings_root_child() {
    let jackin_home = std::env::var_os(CHILD_JACKIN_HOME_ENV).expect("child home path is required");
    let umask = std::env::var(CHILD_UMASK_ENV).expect("child umask is required");
    let mask = match umask.as_str() {
        "0002" => 0o002,
        "0000" => 0o000,
        other => panic!("unsupported child umask {other}"),
    };
    let temp = tempdir().unwrap();
    let manifest = simple_manifest(&temp);
    let paths = paths_for_jackin_home(Path::new(&jackin_home));
    nix::sys::stat::umask(nix::sys::stat::Mode::from_bits_truncate(mask));
    let (state, outcome) = prepare_ignore_state(&paths, &manifest, temp.path()).unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(state.root.join("home").is_dir());
    assert!(state.root.join("state").is_dir());
}

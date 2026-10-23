mod framing;
mod lease;
mod state;
mod storage;

use super::CatalogAuthorityError;
use rustix::fs::{Mode, OFlags, fchmod, mkdirat, openat};
use rustix::io::Errno;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT_ROOT_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_CONTROL_ID: AtomicU64 = AtomicU64::new(1);

const CHILD_MODE_ENV: &str = "JACKIN_CATALOG_TEST_CHILD_MODE";
const CHILD_ROOT_ENV: &str = "JACKIN_CATALOG_TEST_CHILD_ROOT";
const CHILD_CONTROL_ENV: &str = "JACKIN_CATALOG_TEST_CHILD_CONTROL";
const CHILD_WORKER_ENV: &str = "JACKIN_CATALOG_TEST_CHILD_WORKER";
const CHILD_EXPECTED_ERROR_ENV: &str = "JACKIN_CATALOG_TEST_CHILD_EXPECTED_ERROR";
const CHILD_TEST_ID: &str = "catalog_revision::tests::storage::process_child_dispatcher";
const PRESERVE_EVIDENCE_MARKER: &str = ".jackin-unreaped-child-evidence";
const SOCKET_PATH_BOUND: usize = 104;

const TEST_DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const TEST_CREATE_FILE_FLAGS: OFlags = OFlags::WRONLY
    .union(OFlags::CREATE)
    .union(OFlags::EXCL)
    .union(OFlags::NONBLOCK)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const TEST_WRITE_FILE_FLAGS: OFlags = OFlags::WRONLY
    .union(OFlags::NONBLOCK)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const TEST_READ_WRITE_FILE_FLAGS: OFlags = OFlags::RDWR
    .union(OFlags::NONBLOCK)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

fn open_test_path(path: &Path, flags: OFlags, mode: Mode) -> Result<File, Errno> {
    openat(rustix::fs::CWD, path, flags, mode).map(File::from)
}

pub(super) fn create_new_test_file(path: &Path) -> Result<File, Errno> {
    open_test_path(path, TEST_CREATE_FILE_FLAGS, Mode::from_raw_mode(0o600))
}

pub(super) fn open_write_test_file(path: &Path) -> Result<File, Errno> {
    open_test_path(path, TEST_WRITE_FILE_FLAGS, Mode::empty())
}

pub(super) fn open_read_write_test_file(path: &Path) -> Result<File, Errno> {
    open_test_path(path, TEST_READ_WRITE_FILE_FLAGS, Mode::empty())
}

pub(super) fn open_test_directory(path: &Path) -> Result<File, Errno> {
    open_test_path(path, TEST_DIRECTORY_FLAGS, Mode::empty())
}

fn report_stderr(message: &str) {
    let mut stderr = std::io::stderr();
    drop(stderr.write_all(message.as_bytes()));
    drop(stderr.write_all(b"\n"));
}

fn write_preservation_marker(directory: &Path, summary: &str) -> Result<(), String> {
    let path = directory.join(PRESERVE_EVIDENCE_MARKER);
    let mut marker = create_new_test_file(&path)
        .map_err(|error| format!("create {} failed: {error}", path.display()))?;
    marker
        .write_all(summary.as_bytes())
        .map_err(|error| format!("write {} failed: {error}", path.display()))?;
    marker
        .sync_all()
        .map_err(|error| format!("sync {} failed: {error}", path.display()))
}

fn preserve_owned_child_fixtures(
    root_preserve: &AtomicBool,
    control_preserve: &AtomicBool,
    root_path: &Path,
    control_path: &Path,
    summary: &str,
    mut marker_sink: impl FnMut(&Path, &str) -> Result<(), String>,
) {
    root_preserve.store(true, Ordering::Release);
    control_preserve.store(true, Ordering::Release);
    for directory in [root_path, control_path] {
        if let Err(error) = marker_sink(directory, summary) {
            report_stderr(&format!(
                "durable child-preservation marker failed: {error}"
            ));
        }
    }
}

pub(super) struct TestRoot {
    path: PathBuf,
    preserve: Arc<AtomicBool>,
}

impl TestRoot {
    pub(super) fn new() -> Self {
        let base = fs::canonicalize(std::env::temp_dir())
            .expect("test temp directory resolves without a missing ancestor");
        loop {
            let id = NEXT_ROOT_ID.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!(
                ".jackin-catalog-authority-test-{}-{id}",
                std::process::id()
            ));
            if let Err(error) = fs::create_dir(&path) {
                assert_eq!(
                    error.kind(),
                    std::io::ErrorKind::AlreadyExists,
                    "private test root creation failed unexpectedly"
                );
                continue;
            }
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .expect("private test root mode is set explicitly");
            return Self {
                path,
                preserve: Arc::new(AtomicBool::new(false)),
            };
        }
    }

    pub(super) fn new_for_socket() -> Self {
        let base_path = fs::canonicalize("/tmp")
            .expect("local /tmp resolves to its canonical private-test base");
        let base =
            open_test_directory(&base_path).expect("canonical /tmp opens as a no-follow directory");
        loop {
            let id = NEXT_ROOT_ID.fetch_add(1, Ordering::Relaxed);
            let name = format!(".jca-{}-{id}", std::process::id());
            match mkdirat(&base, name.as_str(), Mode::from_raw_mode(0o700)) {
                Ok(()) => {
                    let directory =
                        openat(&base, name.as_str(), TEST_DIRECTORY_FLAGS, Mode::empty())
                            .expect("short private socket root opens without following links");
                    fchmod(&directory, Mode::from_raw_mode(0o700))
                        .expect("short private socket root mode is exact");
                    let path = base_path.join(&name);
                    for socket_name in ["catalog-authority.lock", "catalog-authority.state"] {
                        assert!(
                            path.join(socket_name).as_os_str().as_bytes().len() < SOCKET_PATH_BOUND,
                            "private socket fixture path fits macOS sun_path[104]"
                        );
                    }
                    let mode = fs::metadata(&path)
                        .expect("short private socket root metadata is available")
                        .mode()
                        & 0o7777;
                    assert_eq!(mode, 0o700, "short private socket root is owner-only");
                    return Self {
                        path,
                        preserve: Arc::new(AtomicBool::new(false)),
                    };
                }
                Err(error) if error == Errno::EXIST => {}
                Err(error) => panic!("short private socket root creation failed: {error}"),
            }
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        if self.preserve.load(Ordering::Acquire) {
            report_stderr(&format!(
                "preserving test root {} because child cleanup requested fixture retention",
                self.path.display()
            ));
            return;
        }
        drop(fs::remove_dir_all(&self.path));
    }
}

pub(super) struct ControlDir {
    path: PathBuf,
    preserve: Arc<AtomicBool>,
}

impl ControlDir {
    pub(super) fn new() -> Self {
        let base = fs::canonicalize(std::env::temp_dir())
            .expect("test temp directory resolves without a missing ancestor");
        loop {
            let id = NEXT_CONTROL_ID.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!(
                ".jackin-catalog-authority-control-{}-{id}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => {
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                        .expect("private child control directory mode is set explicitly");
                    return Self {
                        path,
                        preserve: Arc::new(AtomicBool::new(false)),
                    };
                }
                Err(error) => assert_eq!(
                    error.kind(),
                    std::io::ErrorKind::AlreadyExists,
                    "private child control directory creation failed unexpectedly"
                ),
            }
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn marker_path(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    pub(super) fn signal(&self, name: &str) {
        let mut marker = create_new_test_file(&self.marker_path(name))
            .expect("parent control marker is created once");
        marker
            .write_all(b"go")
            .expect("parent control marker is written");
        marker.sync_all().expect("parent control marker is durable");
    }

    pub(super) fn write_value(&self, name: &str, value: &str) {
        let mut file = create_new_test_file(&self.marker_path(name))
            .expect("parent control value is created once");
        file.write_all(value.as_bytes())
            .expect("parent control value is written");
        file.sync_all().expect("parent control value is durable");
    }

    pub(super) fn read_value(&self, name: &str) -> String {
        fs::read_to_string(self.marker_path(name)).expect("sealed child control value is present")
    }
}

impl Drop for ControlDir {
    fn drop(&mut self) {
        if self.preserve.load(Ordering::Acquire) {
            report_stderr(&format!(
                "preserving child control directory {} because child cleanup requested fixture retention",
                self.path.display()
            ));
            return;
        }
        drop(fs::remove_dir_all(&self.path));
    }
}

pub(super) struct TestChild {
    child: Child,
    root_path: PathBuf,
    control_path: PathBuf,
    root_preserve: Arc<AtomicBool>,
    control_preserve: Arc<AtomicBool>,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    label: String,
}

impl TestChild {
    pub(super) fn try_wait(&mut self) -> Option<std::process::ExitStatus> {
        self.child
            .try_wait()
            .expect("owned test child status can be read")
    }

    pub(super) fn kill_and_reap(&mut self) -> Result<std::process::ExitStatus, String> {
        match self.child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => {
                return Err(self
                    .cleanup_failure("unknown", &format!("initial status poll failed: {error}")));
            }
        }
        let kill_note = match self.child.kill() {
            Ok(()) => None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Some(format!("kill found no running child: {error}"))
            }
            Err(error) => {
                return Err(self.cleanup_failure("unknown", &format!("kill failed: {error}")));
            }
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Ok(status),
                Ok(None) if Instant::now() >= deadline => {
                    let kill_detail = kill_note
                        .as_deref()
                        .map_or(String::new(), |note| format!("; {note}"));
                    return Err(self.cleanup_failure(
                        "running",
                        &format!("finite five-second kill/reap deadline expired{kill_detail}"),
                    ));
                }
                Ok(None) => std::thread::park_timeout(Duration::from_millis(10)),
                Err(error) => {
                    let kill_detail = kill_note
                        .as_deref()
                        .map_or(String::new(), |note| format!("; {note}"));
                    return Err(self.cleanup_failure(
                        "unknown",
                        &format!("bounded status poll failed: {error}{kill_detail}"),
                    ));
                }
            }
        }
    }

    fn cleanup_failure(&self, state: &str, reason: &str) -> String {
        let summary = format!(
            "owned child {} pid={} state={state}: {reason}; preserving root {} and control {}",
            self.label,
            self.child.id(),
            self.root_path.display(),
            self.control_path.display()
        );
        preserve_owned_child_fixtures(
            &self.root_preserve,
            &self.control_preserve,
            &self.root_path,
            &self.control_path,
            &summary,
            write_preservation_marker,
        );
        summary
    }

    fn diagnostic(&self) -> String {
        let stdout = fs::read_to_string(&self.stdout_path).unwrap_or_default();
        let stderr = fs::read_to_string(&self.stderr_path).unwrap_or_default();
        format!(
            "{} stdout:\n{stdout}\n{} stderr:\n{stderr}",
            self.label, self.label
        )
    }
}

impl Drop for TestChild {
    fn drop(&mut self) {
        if let Err(error) = self.kill_and_reap() {
            report_stderr(&format!(
                "best-effort child cleanup could not confirm reaping: {error}"
            ));
        }
    }
}

pub(super) fn spawn_test_child(
    root: &TestRoot,
    control: &ControlDir,
    mode: &str,
    worker: &str,
    expected_error: Option<&str>,
) -> TestChild {
    let root_path = root.path().to_path_buf();
    let control_path = control.path().to_path_buf();
    let root_preserve = Arc::clone(&root.preserve);
    let control_preserve = Arc::clone(&control.preserve);
    let stdout_path = control.path().join(format!("{worker}.stdout"));
    let stderr_path = control.path().join(format!("{worker}.stderr"));
    let stdout =
        create_new_test_file(&stdout_path).expect("child stdout log is private and unique");
    let stderr =
        create_new_test_file(&stderr_path).expect("child stderr log is private and unique");
    let mut command =
        Command::new(std::env::current_exe().expect("current Rust test executable is available"));
    command
        .arg("--exact")
        .arg(CHILD_TEST_ID)
        .arg("--nocapture")
        .env(CHILD_MODE_ENV, mode)
        .env(CHILD_ROOT_ENV, root.path())
        .env(CHILD_CONTROL_ENV, control.path())
        .env(CHILD_WORKER_ENV, worker);
    if let Some(expected) = expected_error {
        command.env(CHILD_EXPECTED_ERROR_ENV, expected);
    }
    let child = command
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("bounded child test process starts");
    TestChild {
        child,
        root_path,
        control_path,
        root_preserve,
        control_preserve,
        stdout_path,
        stderr_path,
        label: worker.to_owned(),
    }
}

pub(super) fn wait_for_child_marker(
    child: &mut TestChild,
    control: &ControlDir,
    marker: &str,
    scenario_deadline: Instant,
) {
    let phase_deadline = (Instant::now() + Duration::from_secs(10)).min(scenario_deadline);
    loop {
        if control.marker_path(marker).is_file() {
            return;
        }
        let status = child.try_wait();
        assert!(
            status.is_none(),
            "child {} exited before marker {marker} with {status:?}\n{}",
            child.label,
            child.diagnostic()
        );
        assert!(
            Instant::now() < phase_deadline,
            "child {} timed out before marker {marker}\n{}",
            child.label,
            child.diagnostic()
        );
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

pub(super) fn wait_for_child_success(child: &mut TestChild, scenario_deadline: Instant) {
    let phase_deadline = (Instant::now() + Duration::from_secs(10)).min(scenario_deadline);
    loop {
        if let Some(status) = child.try_wait() {
            assert!(
                status.success(),
                "child {} failed with {status}\n{}",
                child.label,
                child.diagnostic()
            );
            return;
        }
        assert!(
            Instant::now() < phase_deadline,
            "child {} timed out before exit\n{}",
            child.label,
            child.diagnostic()
        );
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

pub(super) fn wait_for_child_signal(control: &Path, marker: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let path = control.join(marker);
    while !path.is_file() {
        assert!(
            Instant::now() < deadline,
            "child timed out waiting for {marker}"
        );
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

pub(super) struct ChildEnvironment {
    pub(super) mode: String,
    pub(super) root: PathBuf,
    pub(super) control: PathBuf,
    pub(super) worker: String,
}

pub(super) fn child_environment() -> Option<ChildEnvironment> {
    let mode = std::env::var(CHILD_MODE_ENV).ok()?;
    let root = std::env::var_os(CHILD_ROOT_ENV).map(PathBuf::from)?;
    let control = std::env::var_os(CHILD_CONTROL_ENV).map(PathBuf::from)?;
    let worker = std::env::var(CHILD_WORKER_ENV).ok()?;
    Some(ChildEnvironment {
        mode,
        root,
        control,
        worker,
    })
}

pub(super) fn child_expected_error() -> Option<String> {
    std::env::var(CHILD_EXPECTED_ERROR_ENV).ok()
}

pub(super) fn write_child_marker(control: &Path, name: &str) {
    let mut marker =
        create_new_test_file(&control.join(name)).expect("child completion marker is created once");
    marker
        .write_all(b"done")
        .expect("child completion marker is written");
    marker
        .sync_all()
        .expect("child completion marker is durable");
}

pub(super) fn write_child_value(control: &Path, name: &str, value: &str) {
    let mut file =
        create_new_test_file(&control.join(name)).expect("child result value is created once");
    file.write_all(value.as_bytes())
        .expect("child result value is written");
    file.sync_all().expect("child result value is durable");
}

pub(super) fn read_child_value(control: &Path, name: &str) -> String {
    fs::read_to_string(control.join(name)).expect("parent result value is present")
}

pub(super) fn assert_error<T>(
    result: Result<T, CatalogAuthorityError>,
    expected: CatalogAuthorityError,
) {
    assert_eq!(result.err(), Some(expected));
}

#[test]
fn unreaped_fixture_flags_survive_failed_marker_sink() {
    let root = TestRoot::new();
    let control = ControlDir::new();
    let root_path = root.path().to_path_buf();
    let control_path = control.path().to_path_buf();
    let mut sink_calls = 0;

    preserve_owned_child_fixtures(
        &root.preserve,
        &control.preserve,
        &root.path,
        &control.path,
        "injected child cleanup failure; state=unknown",
        |directory, _summary| {
            assert!(root.preserve.load(Ordering::Acquire));
            assert!(control.preserve.load(Ordering::Acquire));
            assert!(directory == root.path() || directory == control.path());
            sink_calls += 1;
            Err("injected marker sink failure".to_owned())
        },
    );

    assert_eq!(sink_calls, 2, "both private fixture markers are attempted");
    assert!(!root_path.join(PRESERVE_EVIDENCE_MARKER).exists());
    assert!(!control_path.join(PRESERVE_EVIDENCE_MARKER).exists());
    drop(root);
    drop(control);
    assert!(
        root_path.is_dir(),
        "in-memory root flag retains its fixture"
    );
    assert!(
        control_path.is_dir(),
        "in-memory control flag retains its fixture"
    );
    fs::remove_dir_all(&root_path).expect("proof root is explicitly removed after assertions");
    fs::remove_dir_all(&control_path)
        .expect("proof control directory is explicitly removed after assertions");
}

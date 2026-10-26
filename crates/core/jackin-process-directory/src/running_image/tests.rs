// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(any(target_os = "linux", target_os = "macos"))]

use super::RunningImage;
use std::fs::{self, File};
use std::io::{self, ErrorKind};
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const WORKER_ROLE: &str = "JACKIN_RUNNING_IMAGE_TEST_ROLE";
const WORKER_ROOT: &str = "JACKIN_RUNNING_IMAGE_TEST_ROOT";
const WORKER_NAME: &str = "running_image::tests::handoff_worker";
const WORKER_TIMEOUT: Duration = Duration::from_secs(8);

fn assert_permission_denied(result: io::Result<()>) {
    let error = match result {
        Ok(()) => panic!("operation unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ErrorKind::PermissionDenied);
}

fn open_readonly(path: &Path) -> io::Result<File> {
    use rustix::fs::{Mode, OFlags, open};

    Ok(open(path, OFlags::RDONLY.union(OFlags::CLOEXEC), Mode::empty()).map(File::from)?)
}

fn open_directory(path: &Path) -> io::Result<File> {
    use rustix::fs::{Mode, OFlags, open};

    Ok(open(
        path,
        OFlags::RDONLY
            .union(OFlags::DIRECTORY)
            .union(OFlags::CLOEXEC),
        Mode::empty(),
    )
    .map(File::from)?)
}

#[cfg(target_os = "linux")]
#[test]
fn running_image_accepts_its_file_and_repeated_identity_checks() {
    let running = RunningImage::capture().expect("capture current running image");
    let descriptor =
        open_readonly(Path::new("/proc/self/exe")).expect("open the current image through procfs");
    running
        .verify_file_identity(&descriptor)
        .expect("accept descriptor for current image");
    running
        .verify_file_identity(&descriptor)
        .expect("repeat identity verification");
}

#[test]
fn running_image_rejects_unrelated_files_and_nonregular_descriptors() {
    let running = RunningImage::capture().expect("capture current running image");
    let temporary = tempfile::tempdir().expect("create unrelated-file fixture");
    let unrelated_path = temporary.path().join("unrelated");
    fs::write(&unrelated_path, b"different filesystem object").expect("write unrelated file");
    let unrelated = open_readonly(&unrelated_path).expect("open unrelated regular file");
    assert_permission_denied(running.verify_file_identity(&unrelated));

    let directory = open_directory(temporary.path()).expect("open directory descriptor");
    assert_permission_denied(running.verify_file_identity(&directory));
}

#[cfg(target_os = "linux")]
#[test]
fn linux_running_image_descriptor_is_readonly_private_and_close_on_exec() {
    use rustix::fs::{OFlags, fcntl_getfl};
    use rustix::io::{FdFlags, fcntl_getfd};
    use std::os::fd::AsRawFd;

    let running = RunningImage::capture().expect("capture current running image");
    assert!(running._executable.as_raw_fd() > 2);
    assert_eq!(
        fcntl_getfl(&running._executable).expect("read image descriptor status flags")
            & OFlags::ACCMODE,
        OFlags::RDONLY,
    );
    assert!(
        fcntl_getfd(&running._executable)
            .expect("read image descriptor flags")
            .contains(FdFlags::CLOEXEC)
    );
}

struct ChildGuard {
    child: Child,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            match self.child.kill() {
                Ok(()) | Err(_) => {}
            }
            match self.child.wait() {
                Ok(_) | Err(_) => {}
            }
        }
    }
}

fn spawn_worker(root: &Path, role: &str) -> ChildGuard {
    let child = Command::new("./image-alias")
        .args(["--exact", WORKER_NAME, "--ignored", "--nocapture"])
        .current_dir(root)
        .env(WORKER_ROLE, role)
        .env(WORKER_ROOT, root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn selected image through the relative alias");
    ChildGuard { child }
}

fn wait_for_marker(child: &mut Child, marker: &Path, description: &str) {
    let deadline = Instant::now() + WORKER_TIMEOUT;
    loop {
        if marker.exists() {
            return;
        }
        if let Some(status) = child.try_wait().expect("poll handoff worker") {
            panic!("worker exited before {description}: {status}");
        }
        assert!(
            Instant::now() < deadline,
            "worker did not reach {description} before its deadline"
        );
        test_poll_delay(deadline);
    }
}

fn wait_for_success(child: &mut Child, role: &str) {
    let status = wait_for_exit(child, WORKER_TIMEOUT)
        .unwrap_or_else(|error| panic!("{role} image worker did not finish: {error}"));
    assert!(status.success(), "{role} image worker failed: {status}");
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> io::Result<ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                ErrorKind::TimedOut,
                "image worker exceeded its deadline",
            ));
        }
        test_poll_delay(deadline);
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "bounded test workers poll their own readiness markers outside a runtime"
)]
fn test_poll_delay(deadline: Instant) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if !remaining.is_zero() {
        std::thread::sleep(Duration::from_millis(5).min(remaining));
    }
}

fn write_release(root: &Path, role: &str) {
    fs::write(root.join(format!("{role}.release")), b"release")
        .expect("release image worker after alias transition");
}

#[test]
fn mapped_image_identity_survives_relative_alias_switch() {
    let temporary = tempfile::tempdir().expect("create image handoff fixture");
    let root = temporary.path();
    let image_a_directory = root.join("A");
    let image_b_directory = root.join("B");
    fs::create_dir(&image_a_directory).expect("create image A directory");
    fs::create_dir(&image_b_directory).expect("create image B directory");

    let harness = std::env::current_exe().expect("locate parent test harness for fixture copies");
    let image_a = image_a_directory.join("owner");
    let image_b = image_b_directory.join("owner");
    fs::copy(&harness, &image_a).expect("copy test harness into image A");
    fs::copy(&harness, &image_b).expect("copy test harness into image B");
    for image in [&image_a, &image_b] {
        fs::set_permissions(image, fs::Permissions::from_mode(0o500))
            .expect("make fixture image immutable to ordinary writes");
    }

    let metadata_a = fs::metadata(&image_a).expect("inspect image A");
    let metadata_b = fs::metadata(&image_b).expect("inspect image B");
    assert_eq!(
        fs::read(&image_a).expect("read image A"),
        fs::read(&image_b).expect("read image B")
    );
    assert!(
        metadata_a.dev() != metadata_b.dev() || metadata_a.ino() != metadata_b.ino(),
        "fixture images must be distinct filesystem objects"
    );

    let alias = root.join("image-alias");
    symlink(PathBuf::from("A/owner"), &alias).expect("create relative image alias for A");

    let mut initial = spawn_worker(root, "initial");
    wait_for_marker(
        &mut initial.child,
        &root.join("initial.ready"),
        "pre-capture readiness",
    );

    let replacement = root.join("image-alias.next");
    symlink(PathBuf::from("B/owner"), &replacement).expect("create relative image alias for B");
    fs::rename(&replacement, &alias).expect("atomically switch relative image alias to B");
    write_release(root, "initial");
    wait_for_success(&mut initial.child, "initial");

    let mut fresh = spawn_worker(root, "fresh");
    wait_for_marker(
        &mut fresh.child,
        &root.join("fresh.ready"),
        "fresh-worker pre-capture readiness",
    );
    write_release(root, "fresh");
    wait_for_success(&mut fresh.child, "fresh");
}

#[test]
#[ignore = "invoked only by the bounded relative-alias handoff test"]
fn handoff_worker() {
    let role = std::env::var(WORKER_ROLE).expect("handoff worker role must be explicit");
    assert!(role == "initial" || role == "fresh", "unknown worker role");
    let root = PathBuf::from(std::env::var_os(WORKER_ROOT).expect("handoff worker root"));
    fs::write(root.join(format!("{role}.ready")), b"ready")
        .expect("publish worker readiness before capture");

    let release = root.join(format!("{role}.release"));
    let deadline = Instant::now() + WORKER_TIMEOUT;
    while !release.exists() {
        assert!(Instant::now() < deadline, "parent did not release worker");
        test_poll_delay(deadline);
    }

    let running = RunningImage::capture().expect("capture child running image");
    let image_a = open_readonly(&root.join("A/owner")).expect("open image A candidate");
    let image_b = open_readonly(&root.join("B/owner")).expect("open image B candidate");
    let alias = open_readonly(Path::new("image-alias")).expect("reopen selected relative alias");

    match role.as_str() {
        "initial" => {
            running
                .verify_file_identity(&image_a)
                .expect("retained mapped image remains A after alias switch");
            assert_permission_denied(running.verify_file_identity(&image_b));
            assert_permission_denied(running.verify_file_identity(&alias));
        }
        "fresh" => {
            running
                .verify_file_identity(&image_b)
                .expect("fresh process maps B through the switched alias");
            assert_permission_denied(running.verify_file_identity(&image_a));
            running
                .verify_file_identity(&alias)
                .expect("reopened switched alias identifies B");
        }
        _ => unreachable!("worker role was validated before capture"),
    }
}

// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    ffi::OsString,
    fs,
    os::unix::{ffi::OsStringExt, process::CommandExt},
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, atomic::AtomicBool},
    thread,
    time::{Duration, Instant},
};

use super::{
    CaptureFailureKind, CommandBudget, EnvironmentMode, SetupControls, capture_limited,
    capture_spooled, capture_strict,
};

#[test]
fn bounded_capture_preserves_complete_output_at_the_limit() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf '1234'; printf 'abcd' >&2"]);
    let output = capture_limited(&mut command, 4, 4).expect("capture at exact limits");
    assert_eq!(output.stdout, b"1234");
    assert_eq!(output.stderr, b"abcd");
}

#[test]
fn bounded_capture_drains_and_rejects_overflow_without_returning_partial_output() {
    for (script, stdout_limit, stderr_limit) in [
        ("head -c 131072 /dev/zero", 4, 4),
        ("head -c 131072 /dev/zero >&2", 4, 4),
    ] {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        let error = capture_limited(&mut command, stdout_limit, stderr_limit)
            .expect_err("overflow must fail closed");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("exceeded 4 byte limit"));
    }
}

#[test]
fn strict_capture_preserves_non_utf8_arguments_cwd_and_environment_operations() {
    let directory = tempfile::tempdir().expect("creating capture fixture");
    #[cfg(target_os = "macos")]
    let non_utf8_root = directory.path().join("cwd");
    #[cfg(not(target_os = "macos"))]
    let non_utf8_root = directory
        .path()
        .join(OsString::from_vec(b"cwd-\xff".to_vec()));
    fs::create_dir(&non_utf8_root).expect("creating non-UTF-8 cwd");
    let argument = OsString::from_vec(b"arg-\xfe".to_vec());
    let controls = SetupControls::for_test(Duration::from_secs(10));
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "printf '%s' \"$1\"; test \"$KEEP\" = yes; test -z \"${DROP+x}\"",
            "sh",
        ])
        .arg(&argument)
        .current_dir(&non_utf8_root)
        .env_clear()
        .env("KEEP", "yes")
        .env("DROP", "no")
        .env_remove("DROP");

    let output = capture_strict(
        &mut command,
        &controls,
        EnvironmentMode::Clear,
        1024,
        1024,
        CommandBudget::Probe,
    )
    .expect("strict capture completes");
    assert!(output.status.success());
    assert_eq!(output.stdout, argument.as_encoded_bytes());
}

#[test]
fn strict_capture_exposes_complete_target_nonzero_status() {
    let controls = SetupControls::for_test(Duration::from_secs(10));
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf 'observed'; exit 23"]);

    let output = capture_strict(
        &mut command,
        &controls,
        EnvironmentMode::Inherit,
        1024,
        1024,
        CommandBudget::Probe,
    )
    .expect("complete target nonzero is distinct from supervisor failure");
    assert_eq!(output.status.code(), Some(23));
    assert_eq!(output.stdout, b"observed");
}

#[test]
fn strict_capture_drains_descendant_that_inherits_output_pipes() {
    let directory = tempfile::tempdir().expect("creating capture fixture");
    let tick_path = directory.path().join("ticks");
    let controls = SetupControls::for_test(Duration::from_secs(5));
    let mut command = Command::new("/bin/sh");
    command.args([
        "-c",
        "(trap '' TERM HUP INT; printf x >> \"$1\"; while :; do /bin/sleep 0.02; printf x >> \"$1\"; done) & exit 0",
        "sh",
    ]);
    command.arg(&tick_path);

    let output = capture_strict(
        &mut command,
        &controls,
        EnvironmentMode::Inherit,
        1024,
        1024,
        CommandBudget::Probe,
    )
    .expect("supervisor drains a descendant that inherited output pipes");
    assert!(output.status.success());
    let tick_count = fs::metadata(&tick_path)
        .expect("descendant wrote its readiness tick")
        .len();
    assert!(tick_count > 0);
    let observation_deadline = Instant::now() + Duration::from_millis(50);
    while Instant::now() < observation_deadline {
        thread::park_timeout(observation_deadline.saturating_duration_since(Instant::now()));
    }
    assert_eq!(
        fs::metadata(tick_path)
            .expect("descendant tick remains available")
            .len(),
        tick_count,
        "the process group is drained before successful capture returns"
    );
}

const PIPE_HOLDER_CHILD_MODE: &str = "JACKIN_STRICT_PIPE_HOLDER_CHILD";
const PIPE_HOLDER_CONTROLLER_MODE: &str = "JACKIN_STRICT_PIPE_HOLDER_CONTROLLER";
const PIPE_HOLDER_HOLDER_MODE: &str = "JACKIN_STRICT_PIPE_HOLDER_HOLDER";
const PIPE_HOLDER_READY_PATH: &str = "JACKIN_STRICT_PIPE_HOLDER_READY_PATH";
const PIPE_HOLDER_HEARTBEAT_PATH: &str = "JACKIN_STRICT_PIPE_HOLDER_HEARTBEAT_PATH";
const PIPE_HOLDER_RELEASE_PATH: &str = "JACKIN_STRICT_PIPE_HOLDER_RELEASE_PATH";
const PIPE_HOLDER_STOPPED_PATH: &str = "JACKIN_STRICT_PIPE_HOLDER_STOPPED_PATH";
const PIPE_HOLDER_TEST_NAME: &str = "precommit_snapshot::process::tests::strict_capture_rejects_surviving_pipe_holder_after_zero_exit";

struct PipeHolderCleanup {
    release_path: std::path::PathBuf,
    stopped_path: std::path::PathBuf,
    completed: bool,
    wait_attempted: bool,
}

impl PipeHolderCleanup {
    fn release_and_wait(&mut self) -> bool {
        if self.completed {
            return true;
        }
        if self.wait_attempted {
            return false;
        }
        if fs::write(&self.release_path, b"release\n").is_err() {
            return false;
        }

        self.wait_attempted = true;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if fs::read(&self.stopped_path).is_ok_and(|contents| contents == b"reaped\n") {
                self.completed = true;
                return true;
            }
            thread::park_timeout(Duration::from_millis(10));
        }
        false
    }
}

impl Drop for PipeHolderCleanup {
    fn drop(&mut self) {
        // Keep this bounded on assertion failure. The controller has its own
        // monotonic lifetime and owns/reaps the actual pipe-holder Child.
        let _ = self.release_and_wait();
    }
}

fn wait_for_child_until(child: &mut std::process::Child, deadline: Instant) -> bool {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {}
            Err(error) => panic!("poll pipe-holder child: {error}"),
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::park_timeout(Duration::from_millis(10));
    }
}

fn close_helper_output_pipes() {
    nix::unistd::close(1).expect("close helper stdout pipe");
    nix::unistd::close(2).expect("close helper stderr pipe");
}

fn run_pipe_holder_test_helper() -> bool {
    if std::env::var_os(PIPE_HOLDER_HOLDER_MODE).is_some() {
        let release_path =
            std::env::var_os(PIPE_HOLDER_RELEASE_PATH).expect("pipe-holder release path");
        let heartbeat_path =
            std::env::var_os(PIPE_HOLDER_HEARTBEAT_PATH).expect("pipe-holder heartbeat path");
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut heartbeat = 0_u64;
        let heartbeat_temp_path = Path::new(&heartbeat_path).with_extension("tmp");
        while !Path::new(&release_path).exists() && Instant::now() < deadline {
            heartbeat += 1;
            fs::write(&heartbeat_temp_path, heartbeat.to_string())
                .and_then(|()| fs::rename(&heartbeat_temp_path, &heartbeat_path))
                .expect("atomically publish pipe-holder heartbeat");
            thread::park_timeout(Duration::from_millis(10));
        }
        return true;
    }

    if std::env::var_os(PIPE_HOLDER_CONTROLLER_MODE).is_some() {
        let release_path =
            std::env::var_os(PIPE_HOLDER_RELEASE_PATH).expect("pipe-holder release path");
        let ready_path = std::env::var_os(PIPE_HOLDER_READY_PATH).expect("pipe-holder ready path");
        let heartbeat_path =
            std::env::var_os(PIPE_HOLDER_HEARTBEAT_PATH).expect("pipe-holder heartbeat path");
        let stopped_path =
            std::env::var_os(PIPE_HOLDER_STOPPED_PATH).expect("pipe-holder stopped path");
        let mut holder = Command::new(std::env::current_exe().expect("test executable"));
        holder
            .args(["--exact", PIPE_HOLDER_TEST_NAME, "--nocapture"])
            .env(PIPE_HOLDER_HOLDER_MODE, "1")
            .env(PIPE_HOLDER_RELEASE_PATH, release_path)
            .env(PIPE_HOLDER_HEARTBEAT_PATH, heartbeat_path)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let mut holder = holder.spawn().expect("spawn controlled pipe holder");
        fs::write(ready_path, b"ready\n").expect("publish pipe-holder readiness");

        // The separate controller is bounded and owns/reaps the exact Child.
        let deadline = Instant::now() + Duration::from_secs(9);
        let mut reaped = wait_for_child_until(&mut holder, deadline);
        if !reaped {
            let _kill_result = holder.kill();
            reaped = wait_for_child_until(&mut holder, Instant::now() + Duration::from_secs(1));
        }
        if reaped {
            holder.wait().expect("reap controlled pipe holder");
        }
        close_helper_output_pipes();
        let cleanup_status: &[u8] = if reaped { b"reaped\n" } else { b"uncertain\n" };
        fs::write(stopped_path, cleanup_status).expect("publish pipe-holder cleanup status");
        return true;
    }

    if std::env::var_os(PIPE_HOLDER_CHILD_MODE).is_some() {
        let release_path =
            std::env::var_os(PIPE_HOLDER_RELEASE_PATH).expect("pipe-holder release path");
        let ready_path = std::env::var_os(PIPE_HOLDER_READY_PATH).expect("pipe-holder ready path");
        let heartbeat_path =
            std::env::var_os(PIPE_HOLDER_HEARTBEAT_PATH).expect("pipe-holder heartbeat path");
        let stopped_path =
            std::env::var_os(PIPE_HOLDER_STOPPED_PATH).expect("pipe-holder stopped path");
        let mut controller = Command::new(std::env::current_exe().expect("test executable"));
        controller
            .args(["--exact", PIPE_HOLDER_TEST_NAME, "--nocapture"])
            .env(PIPE_HOLDER_CONTROLLER_MODE, "1")
            .env(PIPE_HOLDER_RELEASE_PATH, release_path)
            .env(PIPE_HOLDER_READY_PATH, &ready_path)
            .env(PIPE_HOLDER_HEARTBEAT_PATH, heartbeat_path)
            .env(PIPE_HOLDER_STOPPED_PATH, stopped_path)
            .process_group(0);
        // This controller deliberately outlives its short-lived test-mode parent.
        // It has its own fixed deadline, and the outer guard releases it and waits
        // for its `reaped` acknowledgment after the supervisor's capture returns.
        #[expect(
            clippy::zombie_processes,
            reason = "the bounded separate-group controller outlives this test child and self-reaps its owned holder"
        )]
        let _controller = controller.spawn().expect("spawn pipe-holder controller");

        let deadline = Instant::now() + Duration::from_secs(2);
        while !Path::new(&ready_path).exists() && Instant::now() < deadline {
            thread::park_timeout(Duration::from_millis(10));
        }
        assert!(Path::new(&ready_path).exists(), "controller started holder");
        return true;
    }
    false
}

#[test]
fn strict_capture_rejects_surviving_pipe_holder_after_zero_exit() {
    if run_pipe_holder_test_helper() {
        return;
    }

    let directory = tempfile::tempdir().expect("creating capture fixture");
    let ready_path = directory.path().join("pipe-holder.ready");
    let heartbeat_path = directory.path().join("pipe-holder.heartbeat");
    let release_path = directory.path().join("pipe-holder.release");
    let stopped_path = directory.path().join("pipe-holder.stopped");
    let mut cleanup = PipeHolderCleanup {
        release_path: release_path.clone(),
        stopped_path: stopped_path.clone(),
        completed: false,
        wait_attempted: false,
    };
    let mut child = Command::new(std::env::current_exe().expect("test executable"));
    child
        .args(["--exact", PIPE_HOLDER_TEST_NAME, "--nocapture"])
        .env(PIPE_HOLDER_CHILD_MODE, "1")
        .env(PIPE_HOLDER_READY_PATH, &ready_path)
        .env(PIPE_HOLDER_HEARTBEAT_PATH, &heartbeat_path)
        .env(PIPE_HOLDER_RELEASE_PATH, &release_path)
        .env(PIPE_HOLDER_STOPPED_PATH, &stopped_path);
    let controls = SetupControls::for_test(Duration::from_millis(750));
    let error = capture_strict(
        &mut child,
        &controls,
        EnvironmentMode::Inherit,
        1024 * 1024,
        1024 * 1024,
        CommandBudget::PhaseRemainder,
    )
    .expect_err("an escaped descendant that still holds a pipe is incomplete");
    let failure = error
        .downcast_ref::<super::CaptureFailure>()
        .expect("incomplete strict capture remains typed");
    assert_eq!(failure.kind, CaptureFailureKind::Capture);
    let detail = error.to_string();
    assert!(detail.contains("code=Some(0)"), "{detail}");
    assert!(detail.contains("capture_complete=false"), "{detail}");
    assert!(detail.contains("group_drained=true"), "{detail}");
    assert!(
        detail.contains("stdout_eof=false") || detail.contains("stderr_eof=false"),
        "{detail}"
    );
    assert_eq!(
        fs::read(&ready_path).expect("controller owns holder"),
        b"ready\n"
    );
    let previous_heartbeat = fs::read_to_string(&heartbeat_path)
        .expect("pipe holder publishes a heartbeat after capture returned")
        .parse::<u64>()
        .expect("valid pipe-holder heartbeat");
    let heartbeat_deadline = Instant::now() + Duration::from_secs(1);
    loop {
        thread::park_timeout(Duration::from_millis(20));
        let current_heartbeat = fs::read_to_string(&heartbeat_path)
            .expect("pipe-holder heartbeat remains available")
            .parse::<u64>()
            .expect("valid pipe-holder heartbeat");
        if current_heartbeat > previous_heartbeat {
            break;
        }
        assert!(
            Instant::now() < heartbeat_deadline,
            "pipe holder remains alive after incomplete capture return"
        );
    }
    assert!(
        !stopped_path.exists(),
        "controller has not yet reaped the live pipe holder"
    );
    let diagnostics = detail
        .split_once("diagnostic spools retained at ")
        .expect("failure retains diagnostic spool")
        .1
        .trim();
    let diagnostics = Path::new(diagnostics);
    assert!(
        diagnostics.starts_with(std::env::temp_dir()),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics.file_name().is_some_and(|name| name
            .as_encoded_bytes()
            .starts_with(b"jackin-native-capture-")),
        "{diagnostics:?}"
    );
    assert!(
        cleanup.release_and_wait(),
        "controller released and reaped holder"
    );
    fs::remove_dir_all(diagnostics).expect("removing retained incomplete-capture diagnostics");
}

#[test]
fn strict_capture_retains_bounded_prefix_when_output_overflows() {
    let controls = SetupControls::for_test(Duration::from_secs(10));
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf '123456789'"]);
    let Err(error) = capture_spooled(
        &mut command,
        &controls,
        EnvironmentMode::Inherit,
        4,
        4,
        CommandBudget::Probe,
    ) else {
        panic!("overflow cannot be represented as a complete output")
    };
    let failure = error
        .downcast_ref::<super::CaptureFailure>()
        .expect("strict capture failure is typed");
    assert_eq!(failure.kind, CaptureFailureKind::Capture);
    let diagnostics = error.to_string();
    let retained = diagnostics
        .split_once("diagnostic spools retained at ")
        .expect("failure reports retained spool directory")
        .1
        .trim();
    assert_eq!(
        fs::read(Path::new(retained).join("stdout.log")).unwrap(),
        b"1234"
    );
    fs::remove_dir_all(retained).expect("removing drained overflow spool");
}

#[test]
fn strict_capture_preserves_spool_files_by_descriptor_and_drops_successful_scratch() {
    let controls = SetupControls::for_test(Duration::from_secs(10));
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf 'stdout'; printf 'stderr' >&2"]);
    let spooled = capture_spooled(
        &mut command,
        &controls,
        EnvironmentMode::Inherit,
        1024,
        1024,
        CommandBudget::Probe,
    )
    .expect("strict spooled capture completes");
    let root = spooled.stdout_path().parent().unwrap().to_path_buf();
    assert!(spooled.stdout_path().exists());
    assert!(spooled.stderr_path().exists());
    assert_eq!(spooled.status().code(), Some(0));
    drop(spooled);
    assert!(!root.exists(), "successful capture scratch is disposable");
}

#[test]
fn strict_capture_checks_cancellation_before_spawning() {
    let cancellation = Arc::new(AtomicBool::new(true));
    let controls = SetupControls::new(cancellation, Duration::from_secs(10));
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exit 0"]);
    let error = capture_strict(
        &mut command,
        &controls,
        EnvironmentMode::Inherit,
        1024,
        1024,
        CommandBudget::Probe,
    )
    .expect_err("cancelled setup must not launch another child");
    assert_eq!(
        error.downcast_ref::<super::CaptureFailure>().unwrap().kind,
        CaptureFailureKind::Cancelled
    );
}

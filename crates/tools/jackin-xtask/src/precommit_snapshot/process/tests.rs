// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    ffi::OsString,
    fs,
    os::unix::ffi::OsStringExt,
    path::Path,
    process::Command,
    sync::{Arc, atomic::AtomicBool},
    thread,
    time::{Duration, Instant},
};

mod pipe_holder;
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
        "(trap '' TERM HUP INT; printf x >> \"$1\"; while :; do /bin/sleep 0.02; printf x >> \"$1\"; done) & attempts=0; while [ ! -s \"$1\" ]; do attempts=$((attempts + 1)); [ \"$attempts\" -lt 100 ] || exit 9; /bin/sleep 0.01; done; exit 0",
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

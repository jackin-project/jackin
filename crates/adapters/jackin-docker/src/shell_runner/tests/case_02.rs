// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(unix)]
#[tokio::test]
async fn debug_capture_does_not_emit_command_arguments_or_output() {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;

    jackin_diagnostics::set_debug_mode(true);
    jackin_diagnostics::begin_debug_buffering();
    let mut runner = ShellRunner { debug: true };
    let output = runner
        .capture(
            "sh",
            &[
                "-c",
                "printf telemetry-private-output",
                "telemetry-private-argument",
            ],
            None,
        )
        .await
        .unwrap();
    let lines = jackin_diagnostics::drain_debug_buffer_for_test();
    jackin_diagnostics::set_debug_mode(false);

    assert_eq!(output, "telemetry-private-output");
    let exported = lines.join("\n");
    assert!(!exported.contains("telemetry-private-output"));
    assert!(!exported.contains("telemetry-private-argument"));
}

#[cfg(unix)]
#[tokio::test]
async fn run_times_out_and_kills_sleep() {
    let mut runner = ShellRunner::default();
    let opts = RunOptions {
        timeout: Some(std::time::Duration::from_millis(200)),
        ..RunOptions::default()
    };
    let started = Instant::now();
    let err = runner
        .run("sleep", &["5"], None, &opts)
        .await
        .expect_err("sleep should time out");
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(err.to_string().contains("timed out"), "{}", err);
}

#[cfg(unix)]
#[tokio::test]
async fn run_completes_before_timeout() {
    let mut runner = ShellRunner::default();
    let opts = RunOptions {
        timeout: Some(std::time::Duration::from_millis(200)),
        ..RunOptions::default()
    };
    runner
        .run("sleep", &["0"], None, &opts)
        .await
        .expect("sleep 0 should succeed within timeout");
}

#[cfg(unix)]
#[tokio::test]
async fn run_emits_process_execute_span_name_on_success() {
    let mut runner = ShellRunner { debug: false };
    runner
        .run("true", &[], None, &RunOptions::default())
        .await
        .expect("true succeeds");
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn captured_run_exports_one_privacy_safe_process_span() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let guard = tracing::subscriber::set_default(subscriber);
    let mut runner = ShellRunner::default();
    let temp = tempfile::tempdir().unwrap();
    let private_cwd = temp.path().join("telemetry-private-cwd");
    std::fs::create_dir(&private_cwd).unwrap();
    let opts = RunOptions {
        capture_stdout: true,
        ..RunOptions::default()
    };
    runner
        .run(
            "sh",
            &[
                "-c",
                "printf telemetry-private-output",
                "telemetry-private-argument",
            ],
            Some(&private_cwd),
            &opts,
        )
        .await
        .unwrap();
    drop(guard);
    export.force_flush();

    let process_spans = export
        .finished_spans()
        .into_iter()
        .filter(|span| span.name == jackin_telemetry::schema::spans::PROCESS_COMMAND)
        .collect::<Vec<_>>();
    assert_eq!(process_spans.len(), 1);
    for prohibited in [
        "telemetry-private-output",
        "telemetry-private-argument",
        "telemetry-private-cwd",
    ] {
        assert!(!export.contains_span_text(prohibited));
        assert!(!export.contains_log_text(prohibited));
    }
}

#[test]
fn process_execute_completion_classifies_success() {
    let result = Ok::<_, anyhow::Error>("captured output");
    assert_eq!(
        process_execute_completion(&result),
        (jackin_telemetry::schema::enums::OutcomeValue::Success, None)
    );
}

#[test]
fn executable_classification_is_bounded_and_private() {
    use jackin_telemetry::schema::enums::ProcessExecutableName;

    assert_eq!(
        jackin_telemetry::process::classify_executable(Path::new("git")),
        ProcessExecutableName::Git
    );
    assert_eq!(
        jackin_telemetry::process::classify_executable(Path::new("operator-private-tool")),
        ProcessExecutableName::Other
    );
}

#[test]
fn process_execute_completion_classifies_timeout() {
    let result = Err::<(), _>(
        DockerError::CommandTimeout {
            secs: 1.0,
            program: "tool".to_owned(),
        }
        .into(),
    );
    assert_eq!(
        process_execute_completion(&result),
        (
            jackin_telemetry::schema::enums::OutcomeValue::Timeout,
            Some(jackin_telemetry::schema::enums::ErrorType::Timeout)
        )
    );
}

#[test]
fn process_execute_completion_classifies_nonzero_exit() {
    let result = Err::<(), _>(
        DockerError::CommandFailed {
            program: "tool".to_owned(),
            args: "--private user-value".to_owned(),
        }
        .into(),
    );
    assert_eq!(
        process_execute_completion(&result),
        (
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(jackin_telemetry::schema::enums::ErrorType::ProcessExitNonzero)
        )
    );
}

#[test]
fn process_execute_completion_classifies_spawn_failure() {
    let result = Err::<(), _>(ProcessBoundaryError::Spawn.into());
    assert_eq!(
        process_execute_completion(&result),
        (
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(jackin_telemetry::schema::enums::ErrorType::ProcessSpawnError)
        )
    );
}

#[test]
fn process_execute_completion_classifies_io_failure() {
    let result = Err::<(), anyhow::Error>(ProcessBoundaryError::Io.into());
    assert_eq!(
        process_execute_completion(&result),
        (
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(jackin_telemetry::schema::enums::ErrorType::IoError)
        )
    );
}

#[test]
fn process_execute_span_redacts_env_args_in_attr_input() {
    let args = &["-e", "FOO=bar", "image"];
    let redacted = redact_env_args(args);
    assert_eq!(redacted, vec!["-e", "FOO=<redacted>", "image"]);
}

#[test]
fn build_stderr_summary_takes_tail_and_redacts_temp_paths() {
    // BuildKit reports the cause at the end; the preamble is noise.
    let stderr = b"#1 [internal] load build definition\n#4 naming to img done\n#4 DONE 0.0s\nERROR: failed to solve: open /tmp/.tmpAbC/Dockerfile: no such file\n";
    let summary = summarize_build_stderr(stderr);

    assert!(summary.contains("ERROR: failed to solve"), "{summary}");
    assert!(!summary.contains("load build definition"), "{summary}");
    assert!(!summary.contains("/tmp/"), "{summary}");
    assert!(summary.contains("<redacted-path>"), "{summary}");
}

#[test]
fn build_stderr_summary_reports_empty_capture() {
    assert_eq!(summarize_build_stderr(b"\n  \n"), "(no stderr captured)");
}

#[test]
fn build_stderr_summary_redacts_multiline_credentials() {
    let stderr = b"token: |\n  block-secret-canary\n-----BEGIN PRIVATE KEY-----\npem-secret-canary\n-----END PRIVATE KEY-----\nERROR: build stopped\n";
    let summary = summarize_build_stderr(stderr);

    assert!(summary.contains("<redacted>"), "{summary}");
    assert!(summary.contains("ERROR: build stopped"), "{summary}");
    assert!(!summary.contains("block-secret-canary"), "{summary}");
    assert!(!summary.contains("pem-secret-canary"), "{summary}");
}

#[cfg(unix)]
#[tokio::test]
#[expect(
    clippy::disallowed_methods,
    reason = "fixture pins a real directory before a malicious pathname swap"
)]
async fn descriptor_cwd_survives_repository_path_replacement_for_git_run_and_capture() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let moved = temporary.path().join("pinned-original");
    let attacker = temporary.path().join("attacker");
    let mut runner = ShellRunner::default();
    let original_tip = fixture_bare_repository(&mut runner, &repository).await;
    let attacker_tip = fixture_bare_repository(&mut runner, &attacker).await;
    assert_eq!(
        original_tip, attacker_tip,
        "fixture deliberately shares ref tips"
    );
    let pinned = std::sync::Arc::new(std::fs::File::open(&repository).unwrap());
    std::fs::rename(&repository, &moved).unwrap();
    std::os::unix::fs::symlink(&attacker, &repository).unwrap();
    let opts = RunOptions {
        pinned_cwd: Some(pinned),
        quiet: true,
        null_stdin: true,
        ..RunOptions::default()
    };
    let parent_cwd = std::env::current_dir().unwrap();
    assert_eq!(
        runner
            .capture_with_options(
                "git",
                &["--git-dir=.", "rev-parse", "--verify", "refs/heads/scratch"],
                None,
                &opts,
            )
            .await
            .unwrap(),
        original_tip,
    );
    runner
        .run(
            "git",
            &[
                "-c",
                "core.hooksPath=/dev/null",
                "--git-dir=.",
                "update-ref",
                "-d",
                "refs/heads/scratch",
                &original_tip,
            ],
            None,
            &opts,
        )
        .await
        .unwrap();
    let deleted_ref = runner
        .capture_with_options(
            "git",
            &["--git-dir=.", "rev-parse", "--verify", "refs/heads/scratch"],
            None,
            &opts,
        )
        .await
        .unwrap_err();
    assert!(deleted_ref.to_string().contains("git"));
    assert_eq!(
        runner
            .capture(
                "git",
                &["--git-dir=.", "rev-parse", "--verify", "refs/heads/scratch"],
                Some(&attacker),
            )
            .await
            .unwrap(),
        attacker_tip,
    );
    assert_eq!(std::env::current_dir().unwrap(), parent_cwd);
}

#[cfg(unix)]
#[tokio::test]
#[expect(
    clippy::disallowed_methods,
    reason = "fixture opens directory and non-directory descriptors to test fail-closed behavior"
)]
async fn descriptor_cwd_rejects_pathname_cwd_and_non_directory_without_fallback() {
    let temporary = tempfile::tempdir().unwrap();
    let mut runner = ShellRunner::default();
    let opts = RunOptions {
        pinned_cwd: Some(std::sync::Arc::new(
            std::fs::File::open(temporary.path()).unwrap(),
        )),
        quiet: true,
        ..RunOptions::default()
    };
    let error = runner
        .run("true", &[], Some(temporary.path()), &opts)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("mutually exclusive"));
    let file = temporary.path().join("ordinary-file");
    std::fs::write(&file, b"not a directory").unwrap();
    let opts = RunOptions {
        pinned_cwd: Some(std::sync::Arc::new(std::fs::File::open(file).unwrap())),
        ..RunOptions::default()
    };
    let error = runner
        .capture_with_options("true", &[], None, &opts)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("not a directory"));
}

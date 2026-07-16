// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `shell_runner`.
use super::*;
use std::time::Instant;

#[cfg(unix)]
#[tokio::test]
async fn run_capture_stderr_returns_hint_after_streaming_stderr() {
    let mut runner = ShellRunner::default();
    let opts = RunOptions {
        capture_stderr: true,
        ..RunOptions::default()
    };

    let error = runner
        .run(
            "sh",
            &["-c", "printf 'region blocked\\n' >&2; exit 2"],
            None,
            &opts,
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("see stderr above"));
}

#[cfg(unix)]
#[tokio::test]
async fn run_capture_reports_stderr_when_streaming_is_suppressed() {
    let mut runner = ShellRunner::default();
    let opts = RunOptions {
        capture_stderr: true,
        stream_captured_output: false,
        ..RunOptions::default()
    };

    let error = runner
        .run(
            "sh",
            &["-c", "printf 'region blocked\\n' >&2; exit 2"],
            None,
            &opts,
        )
        .await
        .unwrap_err();
    let message = error.to_string();

    assert!(
        message.contains("region blocked"),
        "suppressed stderr should be summarized: {message}"
    );
    assert!(
        !message.contains("see stderr above"),
        "must not point at terminal output that was not streamed: {message}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn capture_handles_large_stdout() {
    let mut runner = ShellRunner::default();

    let output = runner
        .capture("sh", &["-c", "yes x | head -c 200000"], None)
        .await
        .unwrap();

    assert!(output.len() >= 190_000);
    assert!(output.starts_with('x'));
}

#[test]
fn redact_env_args_masks_dash_e_value() {
    let args = &[
        "run",
        "-e",
        "CLAUDE_CODE_OAUTH_TOKEN=sk-ant-secretvalue",
        "image:tag",
    ];
    let redacted = redact_env_args(args);
    assert_eq!(
        redacted,
        vec![
            "run",
            "-e",
            "CLAUDE_CODE_OAUTH_TOKEN=<redacted>",
            "image:tag",
        ],
    );
}

#[test]
fn redact_env_args_masks_long_env_form() {
    let args = &["run", "--env", "GITHUB_TOKEN=ghp_secret", "image:tag"];
    let redacted = redact_env_args(args);
    assert_eq!(
        redacted,
        vec!["run", "--env", "GITHUB_TOKEN=<redacted>", "image:tag"],
    );
}

#[test]
fn redact_env_args_leaves_host_passthrough_form_unchanged() {
    let args = &["run", "-e", "GITHUB_TOKEN", "image:tag"];
    let redacted = redact_env_args(args);
    assert_eq!(redacted, vec!["run", "-e", "GITHUB_TOKEN", "image:tag"]);
}

#[test]
fn redact_env_args_redacts_multiple_dash_e_values() {
    let args = &[
        "run",
        "-e",
        "TOKEN=secret-a",
        "--name",
        "my-container",
        "-e",
        "API_KEY=secret-b",
        "image:tag",
    ];
    let redacted = redact_env_args(args);
    assert_eq!(
        redacted,
        vec![
            "run",
            "-e",
            "TOKEN=<redacted>",
            "--name",
            "my-container",
            "-e",
            "API_KEY=<redacted>",
            "image:tag",
        ],
    );
}

#[test]
fn redact_env_args_passes_non_env_args_through() {
    let args = &["build", "-t", "image:tag", "--no-cache", "."];
    let redacted = redact_env_args(args);
    assert_eq!(
        redacted,
        vec!["build", "-t", "image:tag", "--no-cache", "."],
    );
}

#[test]
fn redact_env_args_handles_empty_value() {
    let args = &["run", "-e", "EMPTY=", "image:tag"];
    let redacted = redact_env_args(args);
    assert_eq!(redacted, vec!["run", "-e", "EMPTY=<redacted>", "image:tag"]);
}

#[test]
fn redact_env_args_handles_value_containing_equals() {
    let args = &[
        "run",
        "-e",
        "DATABASE_URL=postgres://user:pass@host:5432/db?sslmode=require",
        "image:tag",
    ];
    let redacted = redact_env_args(args);
    assert_eq!(
        redacted,
        vec!["run", "-e", "DATABASE_URL=<redacted>", "image:tag",],
    );
}

#[test]
fn redact_env_args_handles_dash_e_at_end_with_no_value() {
    let args = &["run", "-e"];
    let redacted = redact_env_args(args);
    assert_eq!(redacted, vec!["run", "-e"]);
}

#[test]
fn redact_env_args_masks_build_arg_value() {
    let args = &[
        "build",
        "--build-arg",
        "GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789",
        ".",
    ];
    let redacted = redact_env_args(args);
    assert_eq!(
        redacted,
        vec!["build", "--build-arg", "GITHUB_TOKEN=<redacted>", "."],
    );
}

#[test]
fn redact_env_args_masks_inline_build_arg_value() {
    let args = &[
        "build",
        "--build-arg=OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwxyz0123456789",
        ".",
    ];
    let redacted = redact_env_args(args);
    assert_eq!(
        redacted,
        vec!["build", "--build-arg=OPENAI_API_KEY=<redacted>", "."],
    );
}

#[test]
fn redact_env_args_masks_token_shaped_freeform_args() {
    let args = &[
        "login",
        "--password=sk-abcdefghijklmnopqrstuvwxyz0123456789",
    ];
    let redacted = redact_env_args(args);
    assert_eq!(redacted, vec!["login", "--password=<redacted>"]);
}

#[cfg(unix)]
#[tokio::test]
async fn capture_secret_omits_stderr_from_error_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let secret_file = dir.path().join("s.txt");
    std::fs::write(&secret_file, "xSECRET_STDERR_CONTENTx").unwrap();
    let script = format!("cat '{}' >&2; exit 1", secret_file.display());
    let mut runner = ShellRunner::default();
    let err = runner
        .capture_secret("sh", &["-c", &script], None)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        !msg.contains("xSECRET_STDERR_CONTENTx"),
        "stderr must not appear in error message: {msg}"
    );
    assert!(msg.contains("sh"), "program name must appear: {msg}");
}

#[test]
fn rich_surface_closes_stdin_for_noninteractive_commands() {
    jackin_diagnostics::set_rich_surface_active(false);
    jackin_diagnostics::set_host_screen_owned(false);
    assert!(!should_null_stdin(&RunOptions::default()));

    jackin_diagnostics::set_rich_surface_active(true);
    assert!(should_null_stdin(&RunOptions::default()));
    assert!(!should_null_stdin(&RunOptions {
        interactive: true,
        ..RunOptions::default()
    }));
    jackin_diagnostics::set_rich_surface_active(false);

    jackin_diagnostics::set_host_screen_owned(true);
    assert!(should_null_stdin(&RunOptions::default()));
    assert!(!should_null_stdin(&RunOptions {
        interactive: true,
        ..RunOptions::default()
    }));
    jackin_diagnostics::set_host_screen_owned(false);
}

#[cfg(unix)]
#[tokio::test]
async fn capture_secret_suppresses_stdout_debug_echo() {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;

    let dir = tempfile::tempdir().unwrap();
    let token_file = dir.path().join("t.txt");
    std::fs::write(&token_file, "gho_token_value\n").unwrap();
    let script = format!("cat '{}'", token_file.display());

    jackin_diagnostics::set_debug_mode(true);
    jackin_diagnostics::begin_debug_buffering();
    let mut runner = ShellRunner { debug: true };
    let output = runner
        .capture_secret("sh", &["-c", &script], None)
        .await
        .unwrap();
    let lines = jackin_diagnostics::drain_debug_buffer_for_test();
    jackin_diagnostics::set_debug_mode(false);

    assert_eq!(
        output, "gho_token_value",
        "secret value must still be returned"
    );
    for line in &lines {
        assert!(
            !line.contains("gho_token_value"),
            "secret must not appear in debug output: {line}"
        );
    }
}

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
            Some("timeout")
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
            Some("process_exit_nonzero")
        )
    );
}

#[test]
fn process_execute_completion_classifies_spawn_failure() {
    let result = Err::<(), _>(anyhow::anyhow!("spawn failed for /private/path"));
    assert_eq!(
        process_execute_completion(&result),
        (
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some("process_spawn_error")
        )
    );
}

#[test]
fn process_execute_span_redacts_env_args_in_attr_input() {
    let args = &["-e", "FOO=bar", "image"];
    let redacted = redact_env_args(args);
    assert_eq!(redacted, vec!["-e", "FOO=<redacted>", "image"]);
}

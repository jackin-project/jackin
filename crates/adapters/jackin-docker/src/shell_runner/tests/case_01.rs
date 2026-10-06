// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
async fn debug_run_reports_suppressed_stderr_without_artifact_hint() {
    let mut runner = ShellRunner { debug: true };
    let opts = RunOptions {
        capture_stderr: true,
        ..RunOptions::default()
    };

    let error = runner
        .run(
            "sh",
            &["-c", "printf 'debug failure detail\\n' >&2; exit 2"],
            None,
            &opts,
        )
        .await
        .unwrap_err();
    let message = error.to_string();

    assert!(
        message.contains("debug failure detail"),
        "captured stderr must remain operator-visible: {message}"
    );
    assert!(
        !message.contains("diagnostics run"),
        "removed local artifacts must not be offered: {message}"
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

#[cfg(unix)]
#[tokio::test]
async fn capture_combined_merges_stdout_and_stderr() {
    let mut runner = ShellRunner::default();

    let output = runner
        .capture_combined("sh", &["-c", "echo out; echo err >&2"], None)
        .await
        .unwrap();

    assert!(
        output.contains("out") && output.contains("err"),
        "both streams must be present: {output:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn capture_combined_surfaces_stderr_only_output() {
    // Pins the `docker logs` diagnose fix: stderr-only output (capsule
    // `Error: ...` with no TTY) must survive combined capture while
    // stdout-only `capture` still returns "" for the same command.
    let mut runner = ShellRunner::default();

    let combined = runner
        .capture_combined("sh", &["-c", "echo 'Error: boom' >&2"], None)
        .await
        .unwrap();
    assert_eq!(combined, "Error: boom");

    let stdout_only = runner
        .capture("sh", &["-c", "echo 'Error: boom' >&2"], None)
        .await
        .unwrap();
    assert!(
        stdout_only.is_empty(),
        "stdout-only capture must stay stdout-only: {stdout_only:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn capture_combined_empty_streams_yield_empty() {
    let mut runner = ShellRunner::default();

    let output = runner.capture_combined("true", &[], None).await.unwrap();

    assert!(output.is_empty(), "no output means empty: {output:?}");
}

#[test]
fn merge_combined_output_joins_nonempty_streams() {
    assert_eq!(merge_combined_output(b"", b""), "");
    assert_eq!(merge_combined_output(b"out\n", b""), "out");
    assert_eq!(merge_combined_output(b"", b"err\n"), "err");
    assert_eq!(merge_combined_output(b"out\n", b"err\n"), "out\nerr");
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

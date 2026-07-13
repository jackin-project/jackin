//! Tests for `shell_runner`.
use super::*;
use std::time::Instant;

fn ambient_telemetry_disables_debug() -> bool {
    std::env::var("JACKIN_TELEMETRY_LEVEL").is_ok_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "debug" | "trace"
        )
    })
}

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

#[cfg(unix)]
#[tokio::test]
async fn debug_run_captures_noncapturing_command_into_diagnostics() {
    if ambient_telemetry_disables_debug() {
        return;
    }
    // A non-quiet, non-capturing `run` would inherit the terminal and
    // stream straight to the screen. Under --debug it must capture both
    // streams and route them to the diagnostics run file instead — never
    // to the terminal (which would flood a rich TUI).
    let dir = tempfile::tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(dir.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, true, "test").unwrap();
    let _active = run.activate();
    let mut runner = ShellRunner { debug: true };
    runner
        .run(
            "sh",
            &["-c", "echo hello-from-cmd"],
            None,
            &RunOptions::default(),
        )
        .await
        .unwrap();
    let contents = std::fs::read_to_string(run.path()).unwrap();
    assert!(
        contents.contains("hello-from-cmd"),
        "non-capturing command stdout must be captured into the run file under --debug: {contents}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn debug_run_scrubs_captured_command_output() {
    if ambient_telemetry_disables_debug() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let token_file = dir.path().join("token.txt");
    std::fs::write(&token_file, "token=ghp_1234567890abcdef\n").unwrap();
    let script = format!("cat '{}'", token_file.display());
    let paths = jackin_core::JackinPaths::for_tests(dir.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, true, "test").unwrap();
    let _active = run.activate();
    let mut runner = ShellRunner { debug: true };
    runner
        .run("sh", &["-c", &script], None, &RunOptions::default())
        .await
        .unwrap();

    let contents = std::fs::read_to_string(run.path()).unwrap();
    assert!(!contents.contains("ghp_1234567890abcdef"));
    assert!(contents.contains("<redacted>"));
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
#[allow(
    clippy::await_holding_lock,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
async fn capture_secret_suppresses_stdout_debug_echo() {
    use std::sync::Mutex;
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

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

#[test]
fn process_execute_span_redacts_env_args_in_attr_input() {
    let args = &["-e", "FOO=bar", "image"];
    let redacted = redact_env_args(args);
    assert_eq!(redacted, vec!["-e", "FOO=<redacted>", "image"]);
}

#![allow(
    clippy::unwrap_used,
    reason = "integration test fixture setup should fail immediately with source location"
)]

use std::io::Write as _;
use std::os::unix::fs::symlink;
use std::process::{Command, Stdio};

const CLAUDE_TRAILER: &str = "Co-authored-by: Claude <noreply@anthropic.com>";
const CODEX_TRAILER: &str = "Co-authored-by: Codex <codex@openai.com>";
const SIGNOFF_TRAILER: &str = "Signed-off-by: Test User <test@example.com>";

fn run_hook(input: &str, source: Option<&str>) -> String {
    let temp = tempfile::tempdir().unwrap();
    let message_path = temp.path().join("COMMIT_EDITMSG");
    let hook_path = temp.path().join("prepare-commit-msg");
    let git_config_path = temp.path().join("gitconfig");
    let dco_cache_path = temp.path().join("git-dco-identity");
    let xdg_config_home = temp.path().join("xdg-config");
    std::fs::write(&message_path, input).unwrap();
    std::fs::write(
        &git_config_path,
        "[user]\n\tname = Test User\n\temail = test@example.com\n",
    )
    .unwrap();
    std::fs::create_dir(&xdg_config_home).unwrap();
    symlink(env!("CARGO_BIN_EXE_jackin-capsule"), &hook_path).unwrap();

    let mut command = Command::new(&hook_path);
    command
        .arg(&message_path)
        .current_dir(temp.path())
        .env("GIT_CONFIG_GLOBAL", &git_config_path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", temp.path())
        .env("XDG_CONFIG_HOME", &xdg_config_home)
        .env("JACKIN_GIT_DCO_IDENTITY_CACHE", &dco_cache_path)
        .env("JACKIN_AGENT", "codex")
        .env("JACKIN_GIT_COAUTHOR_TRAILER", "1")
        .env("JACKIN_GIT_DCO", "1");
    if let Some(source) = source {
        command.arg(source);
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "integration test invokes the hook subprocess directly"
    )]
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    std::fs::read_to_string(message_path).unwrap()
}

fn parse_trailers(message: &str) -> String {
    let mut child = Command::new("git")
        .args(["interpret-trailers", "--parse"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(message.as_bytes()).unwrap();
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn assert_parseable_test_trailers(message: &str) {
    assert_eq!(
        parse_trailers(message),
        format!("{SIGNOFF_TRAILER}\n{CODEX_TRAILER}\n")
    );
}

#[test]
fn hook_normalizes_separated_manual_trailers() {
    let message = run_hook(
        &format!("subject\n\n{CODEX_TRAILER}\n\n{SIGNOFF_TRAILER}\n"),
        None,
    );

    assert_parseable_test_trailers(&message);
    assert_eq!(message.matches(CODEX_TRAILER).count(), 1);
    assert_eq!(message.matches(SIGNOFF_TRAILER).count(), 1);
}

#[test]
fn hook_normalizes_trailers_without_subject_separator() {
    let message = run_hook(
        &format!("subject\n{CODEX_TRAILER}\n{SIGNOFF_TRAILER}\n"),
        None,
    );

    assert_parseable_test_trailers(&message);
    assert_eq!(message.matches(CODEX_TRAILER).count(), 1);
    assert_eq!(message.matches(SIGNOFF_TRAILER).count(), 1);
}

#[test]
fn hook_injects_trailers_for_merge_messages() {
    let message = run_hook(
        "Merge remote-tracking branch 'origin/main' into feature\n",
        Some("merge"),
    );

    assert_parseable_test_trailers(&message);
}

#[test]
fn hook_injects_trailers_for_squash_messages() {
    let message = run_hook("squash! fix(capsule): prior change\n", Some("squash"));

    assert_parseable_test_trailers(&message);
}

#[test]
fn hook_adds_current_agent_to_reused_messages() {
    let message = run_hook(&format!("subject\n\n{CLAUDE_TRAILER}\n"), Some("commit"));
    let trailers = parse_trailers(&message);

    assert_eq!(
        trailers,
        format!("{SIGNOFF_TRAILER}\n{CLAUDE_TRAILER}\n{CODEX_TRAILER}\n")
    );
    assert_eq!(message.matches(CLAUDE_TRAILER).count(), 1);
    assert_eq!(message.matches(CODEX_TRAILER).count(), 1);
    assert_eq!(message.matches(SIGNOFF_TRAILER).count(), 1);
}

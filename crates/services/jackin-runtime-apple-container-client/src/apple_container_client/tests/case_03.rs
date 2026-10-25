// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::task::Poll;

struct FixtureCommandRunner {
    requests: Mutex<Vec<(PathBuf, Vec<String>)>>,
    stdout: Vec<u8>,
    success: bool,
}

impl FixtureCommandRunner {
    fn new(stdout: impl AsRef<[u8]>, success: bool) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            stdout: stdout.as_ref().to_vec(),
            success,
        }
    }

    fn assert_list_request(&self) {
        let requests = self.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, PathBuf::from("container"));
        assert_eq!(
            requests[0].1,
            vec![
                "list".to_owned(),
                "--all".to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
            ]
        );
    }
}

impl AppleContainerCommandRunner for FixtureCommandRunner {
    async fn exec(
        &self,
        request: &jackin_process::ExecRequest,
    ) -> Result<jackin_process::ExecResult> {
        std::future::poll_fn(|_| {
            self.requests.lock().unwrap().push((
                request.program.clone(),
                request
                    .args
                    .iter()
                    .map(|argument| argument.to_string_lossy().into_owned())
                    .collect(),
            ));
            Poll::Ready(Ok(jackin_process::ExecResult {
                code: Some(i32::from(!self.success)),
                success: self.success,
                stdout: self.stdout.clone(),
                stderr: Vec::new(),
                duration: std::time::Duration::ZERO,
                timed_out: false,
            }))
        })
        .await
    }
}

struct HelpFixtureRunner {
    requests: Mutex<Vec<(PathBuf, Vec<String>)>>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    success: bool,
    error: bool,
}

impl HelpFixtureRunner {
    fn new(stdout: impl AsRef<[u8]>, stderr: impl AsRef<[u8]>, success: bool) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            stdout: stdout.as_ref().to_vec(),
            stderr: stderr.as_ref().to_vec(),
            success,
            error: false,
        }
    }

    fn failing() -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            stdout: Vec::new(),
            stderr: Vec::new(),
            success: false,
            error: true,
        }
    }

    fn assert_help_request(&self) {
        let requests = self.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0, PathBuf::from("container"));
        assert_eq!(requests[0].1, vec!["run".to_owned(), "--help".to_owned()]);
    }
}

impl AppleContainerCommandRunner for HelpFixtureRunner {
    async fn exec(
        &self,
        request: &jackin_process::ExecRequest,
    ) -> Result<jackin_process::ExecResult> {
        std::future::poll_fn(move |_| {
            self.requests.lock().unwrap().push((
                request.program.clone(),
                request
                    .args
                    .iter()
                    .map(|argument| argument.to_string_lossy().into_owned())
                    .collect(),
            ));
            let result = if self.error {
                Err(anyhow::anyhow!("fixture help command failed"))
            } else {
                Ok(jackin_process::ExecResult {
                    code: Some(i32::from(!self.success)),
                    success: self.success,
                    stdout: self.stdout.clone(),
                    stderr: self.stderr.clone(),
                    duration: std::time::Duration::ZERO,
                    timed_out: false,
                })
            };
            Poll::Ready(result)
        })
        .await
    }
}

#[tokio::test]
async fn list_client_uses_exact_all_json_command_and_parses_stopped_rows() {
    let runner = FixtureCommandRunner::new(
        br#"[
            {"id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":"running"}},
            {"id":"jk-a-extra","configuration":{"id":"jk-a-extra"},"status":{"state":"stopped"}}
        ]"#,
        true,
    );
    let listed = AppleContainerClient::new()
        .list_containers_with_command_runner(&runner, "jk-a")
        .await
        .unwrap();

    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, "jk-a");
    assert_eq!(listed[0].status, AppleContainerStatus::Running);
    assert_eq!(listed[1].id, "jk-a-extra");
    assert_eq!(listed[1].status, AppleContainerStatus::Stopped);
    runner.assert_list_request();
}

#[tokio::test]
async fn inspection_matches_exact_id_when_prefix_neighbor_is_first() {
    let runner = FixtureCommandRunner::new(
        br#"[
            {"id":"jk-a-extra","configuration":{"id":"jk-a-extra"},"status":{"state":"stopped"}},
            {"id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":"running"}}
        ]"#,
        true,
    );
    let inspected = AppleContainerClient::new()
        .inspect_container_with_command_runner(&runner, "jk-a")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(inspected.id, "jk-a");
    assert_eq!(inspected.status, AppleContainerStatus::Running);
    runner.assert_list_request();
}

#[tokio::test]
async fn malformed_unrelated_row_fails_before_prefix_filtering() {
    let runner = FixtureCommandRunner::new(
        br#"[
            {"id":"jk-match","configuration":{"id":"jk-match"},"status":{"state":"running"}},
            {"id":"other","configuration":{"id":"other"},"status":{"state":"future-state"}}
        ]"#,
        true,
    );
    AppleContainerClient::new()
        .list_containers_with_command_runner(&runner, "jk-match")
        .await
        .unwrap_err();
    runner.assert_list_request();
}

#[tokio::test]
async fn malformed_non_utf8_and_nonzero_list_results_are_errors() {
    for (output, success) in [
        (b"[not-json]".to_vec(), true),
        (vec![b'[', 0xff, b']'], true),
        (b"[]".to_vec(), false),
    ] {
        let runner = FixtureCommandRunner::new(output, success);
        AppleContainerClient::new()
            .list_containers_with_command_runner(&runner, "jk-a")
            .await
            .unwrap_err();
        runner.assert_list_request();
    }
}

#[tokio::test]
async fn production_listing_rejects_a_substring_state_lookalike() {
    let runner = FixtureCommandRunner::new(
        br#"[{"id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":"not-running"}}]"#,
        true,
    );
    AppleContainerClient::new()
        .list_containers_with_command_runner(&runner, "jk-a")
        .await
        .unwrap_err();
    runner.assert_list_request();
}

#[tokio::test]
async fn listing_parse_errors_do_not_echo_untrusted_inventory_strings() {
    const CANARY: &str = "inventory-secret-canary-7bba";
    let stdout =
        format!(r#"[{{"id":"jk-a","configuration":"{CANARY}","status":{{"state":"running"}}}}]"#);
    let runner = FixtureCommandRunner::new(stdout.as_bytes(), true);
    let error = AppleContainerClient::new()
        .list_containers_with_command_runner(&runner, "jk-a")
        .await
        .unwrap_err();

    let display = format!("{error}");
    let alternate = format!("{error:#}");
    let debug = format!("{error:?}");
    assert!(!display.contains(CANARY), "Display leaked input: {display}");
    assert!(
        !alternate.contains(CANARY),
        "alternate Display leaked input: {alternate}"
    );
    assert!(!debug.contains(CANARY), "Debug leaked input: {debug}");
    runner.assert_list_request();
}

#[tokio::test]
async fn launch_generation_support_probe_uses_run_help_and_exact_label_option() {
    let runner = HelpFixtureRunner::new(
        b"Usage: container run [OPTIONS] IMAGE\nOptions:\n  --label <KEY=VALUE>\n  --name <NAME>\n",
        b"",
        true,
    );
    assert!(
        AppleContainerClient::new()
            .launch_generation_label_supported_with_runner(&runner)
            .await
    );
    runner.assert_help_request();
}

#[tokio::test]
async fn launch_generation_support_probe_fails_closed_on_errors_or_ambiguous_help() {
    let mut invalid_utf8 = b"Options:\n  --label <KEY=VALUE>\n".to_vec();
    invalid_utf8.push(0xff);
    let cases = [
        HelpFixtureRunner::new(b"Options:\n  --labelish <value>\n", b"", true),
        HelpFixtureRunner::new(
            b"Options:\n  --label <KEY=VALUE>\n  --label <OTHER>\n",
            b"",
            true,
        ),
        HelpFixtureRunner::new(
            b"Options:\n  --label <KEY=VALUE>\n",
            b"Options:\n  --label <OTHER>\n",
            true,
        ),
        HelpFixtureRunner::new(invalid_utf8, b"", true),
        HelpFixtureRunner::new(b"Options:\n  --label <KEY=VALUE>\n", b"", false),
        HelpFixtureRunner::failing(),
    ];

    for runner in cases {
        assert!(
            !AppleContainerClient::new()
                .launch_generation_label_supported_with_runner(&runner)
                .await
        );
        runner.assert_help_request();
    }
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! GitHub pull-request context lookup for the capsule status bar.

use std::path::Path;
#[cfg(test)]
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use crate::git_context::GH_PULL_REQUEST_COMMAND_TIMEOUT;
use crate::pull_request::{PullRequestChecks, PullRequestInfo};
use crate::util::{WaitOutcome, wait_child_with_timeout};
use jackin_tui::sanitize_terminal_title;

use std::sync::Arc;

/// Distinguishes "lookup succeeded but command was unavailable / failed
/// in a way that means we should not cache" from "lookup succeeded with
/// data (or with no data)". The string carries an operator-readable
/// reason for the daemon log; callers should not parse it.
#[derive(Debug)]
pub(crate) enum LookupError {
    Failed(String),
}

impl std::fmt::Display for LookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LookupError::Failed(reason) => f.write_str(reason),
        }
    }
}

fn build_gh_command(workdir: &Path) -> jackin_process::ExecRequest {
    jackin_process::ExecRequest::new("gh", None::<&str>)
        .cwd(workdir)
        .envs([("GH_PROMPT_DISABLED", "1"), ("GH_NO_UPDATE_NOTIFIER", "1")])
}

#[derive(Deserialize)]
struct GhCheck {
    bucket: String,
    #[serde(default)]
    link: String,
}

#[derive(Deserialize)]
struct GhPullRequestView {
    #[serde(rename = "statusCheckRollup", default)]
    status_check_rollup: Vec<GhStatusCheck>,
}

#[derive(Deserialize)]
struct GhStatusCheck {
    #[serde(default)]
    status: String,
    #[serde(default)]
    conclusion: String,
    #[serde(rename = "detailsUrl", default)]
    details_url: String,
}

/// Run `gh <args>` and parse stdout as JSON. `Ok(None)` means
/// `gh` exited successfully (per `accepted_statuses`) with empty
/// stdout, the documented "no rows" shape. Failure is mapped to
/// `LookupError::Failed` with the JSON parse error and a payload
/// prefix so the operator can triage via governed telemetry /
/// `--debug` traces.
fn gh_json<T: serde::de::DeserializeOwned>(
    workdir: &Path,
    label: &str,
    args: &[&str],
    accepted_statuses: &[i32],
) -> Result<Option<T>, LookupError> {
    let mut request = build_gh_command(workdir);
    request.args.extend(args.iter().map(Into::into));
    let json =
        run_command_capturing_output(&request, GH_PULL_REQUEST_COMMAND_TIMEOUT, accepted_statuses)?;
    let Some(json) = json else {
        return Ok(None);
    };
    let parsed = serde_json::from_str::<T>(&json).map_err(|e| {
        LookupError::Failed(format!(
            "{label} JSON parse failed: {e}; payload prefix: {json:.200?}"
        ))
    })?;
    Ok(Some(parsed))
}

pub(crate) fn gh_pull_request_info(
    workdir: &Path,
    branch: &str,
) -> Result<Option<Arc<PullRequestInfo>>, LookupError> {
    #[derive(Deserialize)]
    struct GhPullRequest {
        number: u64,
        title: String,
        url: String,
        #[serde(rename = "isDraft")]
        is_draft: bool,
    }

    // `gh pr list` with no matching PR prints an empty JSON array `[]`,
    // which `gh_json` parses to `Some(vec![])`. An empty stdout
    // surfaces as `Ok(None)`. Either shape collapses to "no PR".
    let Some(prs) = gh_json::<Vec<GhPullRequest>>(
        workdir,
        "gh pr list",
        &[
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            "open",
            "--limit",
            "1",
            "--json",
            "number,title,url,isDraft",
        ],
        &[0],
    )?
    else {
        return Ok(None);
    };
    let Some(pr) = prs.into_iter().next() else {
        return Ok(None);
    };
    if url::Url::parse(&pr.url)
        .ok()
        .as_ref()
        .is_none_or(|u| !matches!(u.scheme(), "http" | "https"))
    {
        return Err(LookupError::Failed(format!(
            "gh pr list returned non-http(s) url: {:?}",
            pr.url
        )));
    }
    // Checks lookup is best-effort — a parse failure on checks should
    // not poison the PR cache. Demote any error to `None` checks.
    let checks = gh_pull_request_checks(workdir, &pr.url)
        .map_err(|e| {
            jackin_diagnostics::telemetry_info!(
                "capsule",
                "pull-request-context: gh pr checks failed: {e}"
            );
        })
        .ok()
        .flatten();
    // GitHub does not sanitize PR titles for terminal safety; strip
    // control bytes here so the dialog body, the bottom bar, and the
    // OSC 2 outer-terminal title can all consume the field directly.
    // A crafted title like `bad\x1b[2J\x1b]2;evil\x07` would otherwise
    // execute its escapes the first time an operator opens the dialog.
    Ok(Some(Arc::new(PullRequestInfo {
        number: pr.number,
        title: sanitize_terminal_title(&pr.title),
        url: pr.url,
        is_draft: pr.is_draft,
        checks,
    })))
}

fn gh_pull_request_checks(
    workdir: &Path,
    url: &str,
) -> Result<Option<PullRequestChecks>, LookupError> {
    // `gh pr checks` exits with `8` when checks are pending and `0`
    // otherwise; both are accepted statuses.
    let Some(checks) = gh_json::<Vec<GhCheck>>(
        workdir,
        "gh pr checks",
        &["pr", "checks", url, "--json", "bucket,link,name,workflow"],
        &[0, 8],
    )?
    else {
        return Ok(None);
    };
    for check in &checks {
        if !matches!(
            check.bucket.as_str(),
            "pass" | "fail" | "pending" | "skipping" | "cancel"
        ) {
            jackin_diagnostics::telemetry_debug!(
                "capsule",
                "pull-request-context: unknown gh pr checks bucket {:?}",
                check.bucket
            );
        }
    }
    let ci_url = best_check_url(&checks)
        .or_else(|| {
            gh_status_check_rollup_url(workdir, url)
                .map_err(|e| {
                    jackin_diagnostics::telemetry_debug!(
                        "capsule",
                        "pull-request-context: gh pr view statusCheckRollup failed: {e}"
                    );
                })
                .ok()
                .flatten()
        })
        .or_else(|| pr_checks_tab_url(url));
    Ok(Some(
        PullRequestChecks::from_buckets(checks.iter().map(|c| c.bucket.as_str()))
            .with_ci_url(ci_url),
    ))
}

fn best_check_url(checks: &[GhCheck]) -> Option<String> {
    ["fail", "cancel", "pending", "pass", "skipping"]
        .into_iter()
        .find_map(|bucket| {
            checks
                .iter()
                .filter(|check| check.bucket == bucket)
                .find_map(|check| validated_http_url(&check.link))
        })
}

fn gh_status_check_rollup_url(workdir: &Path, url: &str) -> Result<Option<String>, LookupError> {
    let Some(view) = gh_json::<GhPullRequestView>(
        workdir,
        "gh pr view",
        &["pr", "view", url, "--json", "statusCheckRollup"],
        &[0],
    )?
    else {
        return Ok(None);
    };
    Ok(best_status_check_url(&view.status_check_rollup))
}

fn best_status_check_url(checks: &[GhStatusCheck]) -> Option<String> {
    [0, 1, 2, 3].into_iter().find_map(|priority| {
        checks
            .iter()
            .filter(|check| status_check_priority(check) == priority)
            .find_map(|check| validated_http_url(&check.details_url))
    })
}

fn status_check_priority(check: &GhStatusCheck) -> u8 {
    match check.conclusion.as_str() {
        "FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" => 0,
        "SUCCESS" | "NEUTRAL" => 2,
        "SKIPPED" => 3,
        _ if !matches!(check.status.as_str(), "COMPLETED" | "") => 1,
        _ => 3,
    }
}

fn pr_checks_tab_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    Some(format!("{}/checks", url.trim_end_matches('/')))
}

fn validated_http_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| url.to_owned())
}

#[cfg(test)]
pub(crate) fn command_stdout_trimmed(command: &mut Command) -> Option<String> {
    let mut request = jackin_process::ExecRequest::new(command.get_program(), command.get_args())
        .stdout_mode(jackin_process::StdioMode::Capture)
        .stderr_mode(jackin_process::StdioMode::Null);
    if let Some(cwd) = command.get_current_dir() {
        request = request.cwd(cwd);
    }
    crate::util::command_stdout_trimmed_with_timeout(
        &request,
        crate::git_context::GIT_CONTEXT_COMMAND_TIMEOUT,
    )
}

/// Result-returning command runner that distinguishes success (returns
/// `Ok(Some(stdout))` or `Ok(None)` for empty stdout) from genuine
/// failure (returns `Err(LookupError::Failed)`). Used by the gh
/// helpers so cache-poisoning can be avoided.
///
/// Differences from `command_stdout_trimmed_with_timeout`:
/// - stdin is set to `Stdio::null()` so a misbehaving subprocess never
///   blocks reading from the daemon's stdin awaiting a prompt.
/// - stderr is captured into a bounded buffer and surfaced in the error
///   reason — the operator can see "gh: not logged in" / "HTTP 401"
///   when triaging via governed telemetry.
fn run_command_capturing_output(
    request: &jackin_process::ExecRequest,
    timeout: Duration,
    accepted_statuses: &[i32],
) -> Result<Option<String>, LookupError> {
    let program = request.program.display().to_string();
    let mut child = match jackin_process::spawn_sync(request) {
        Ok(child) => child,
        Err(e) => {
            return Err(LookupError::Failed(format!("{program}: spawn failed: {e}")));
        }
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| LookupError::Failed(format!("{program}: stdout pipe missing")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| LookupError::Failed(format!("{program}: stderr pipe missing")))?;
    let stdout_label: &'static str = "stdout";
    let stderr_label: &'static str = "stderr";
    let stdout_reader = read_pipe_bounded(program.clone(), stdout_label, stdout, 64 * 1024);
    let stderr_reader = read_pipe_bounded(program.clone(), stderr_label, stderr, 4 * 1024);
    let status_success: Option<bool> = match wait_child_with_timeout(&mut child, &program, timeout)
    {
        WaitOutcome::Exited(status) => Some(
            status
                .code()
                .is_some_and(|code| accepted_statuses.contains(&code)),
        ),
        WaitOutcome::Reaped => None,
        WaitOutcome::TimedOut => {
            drop(stdout_reader.join());
            drop(stderr_reader.join());
            return Err(LookupError::Failed(format!(
                "{program}: timed out after {timeout:?}"
            )));
        }
        WaitOutcome::Failed(e) => {
            drop(stdout_reader.join());
            drop(stderr_reader.join());
            return Err(LookupError::Failed(format!(
                "{program}: try_wait failed: {e} (errno={:?})",
                e.raw_os_error()
            )));
        }
    };
    let stdout_bytes = stdout_reader
        .join()
        .map_err(|_| LookupError::Failed(format!("{program}: stdout reader panicked")))?
        .map_err(|e| LookupError::Failed(format!("{program}: stdout read failed: {e}")))?;
    let stderr_bytes = stderr_reader
        .join()
        .unwrap_or(Ok(Vec::new()))
        .unwrap_or_default();
    command_output_or_lookup_error(&program, status_success, &stdout_bytes, &stderr_bytes)
}

pub(crate) fn command_output_or_lookup_error(
    program: &str,
    status_success: Option<bool>,
    stdout_bytes: &[u8],
    stderr_bytes: &[u8],
) -> Result<Option<String>, LookupError> {
    let stderr_nonempty = stderr_bytes.iter().any(|b| !b.is_ascii_whitespace());
    let trimmed_stderr = || String::from_utf8_lossy(stderr_bytes).trim().to_owned();
    let value = String::from_utf8_lossy(stdout_bytes).trim().to_owned();
    match status_success {
        Some(false) => Err(LookupError::Failed(format!(
            "{program}: non-accepted status; stderr: {}",
            trimmed_stderr()
        ))),
        None if value.is_empty() && stderr_nonempty => Err(LookupError::Failed(format!(
            "{program}: status unavailable; stderr: {}",
            trimmed_stderr()
        ))),
        _ if value.is_empty() => Ok(None),
        _ => Ok(Some(value)),
    }
}

fn read_pipe_bounded<R: std::io::Read + Send + 'static>(
    program: String,
    stream: &'static str,
    mut pipe: R,
    cap: usize,
) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
    jackin_telemetry::spawn::thread_stream(
        "pr_context.stdout",
        move || -> std::io::Result<Vec<u8>> {
            let mut bytes = Vec::with_capacity(cap.min(16 * 1024));
            let mut buf = [0u8; 4096];
            let mut truncated = false;
            loop {
                let n = pipe.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                let take = (cap - bytes.len()).min(n);
                bytes.extend_from_slice(&buf[..take]);
                if bytes.len() >= cap {
                    // Cap reached; drain remaining bytes so the writer
                    // doesn't block on SIGPIPE waiting for us.
                    truncated = true;
                    while pipe.read(&mut buf)? > 0 {}
                    break;
                }
            }
            if truncated {
                jackin_diagnostics::telemetry_debug!(
                    "capsule",
                    "read_pipe_bounded[{program} {stream}]: capped at {cap} bytes; downstream parsing may fail"
                );
            }
            Ok(bytes)
        },
    )
}

//! jackin-pr-trailers: PR trailer rewrite helper binary.
//!
//! **Architecture Invariant:** T1.
//! Entry point: [`main`] — binary entry for trailer rewrites.

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use jackin_process::{ExecRequest, ExecResult, StdioMode};
use std::collections::HashSet;

#[derive(Parser)]
#[command(
    name = "jackin-pr-trailers",
    about = "Extract git trailers from PR commits for squash merge messages"
)]
struct Args {
    /// GitHub PR number. If omitted, auto-detects the current branch name,
    /// finds the corresponding PR (if any), verifies the branch is in sync with
    /// remote, and extracts trailers from the PR or local branch commits.
    #[arg(short, long)]
    pr: Option<u64>,

    /// Repository in owner/repo form
    #[arg(short, long, default_value = "jackin-project/jackin")]
    repo: String,

    /// Path to a file that already contains the prepared PR body text.
    /// If provided, the extracted trailers will be appended to this file
    /// (after a blank line). Otherwise trailers are printed to stdout.
    #[arg(long)]
    body_file: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let commit_messages = commit_messages_for_args(&args)?;
    let trailer_block = trailer_block_from_messages(commit_messages)?;

    if let Some(path) = &args.body_file {
        if !trailer_block.is_empty() {
            append_trailer_block(path, &trailer_block)?;
            print_appended_message(path);
        }
    } else if !trailer_block.is_empty() {
        print_trailer_block(&trailer_block);
    }

    Ok(())
}

fn commit_messages_for_args(args: &Args) -> Result<Vec<String>> {
    if let Some(pr) = args.pr {
        return commit_messages_from_pr(pr, &args.repo);
    }

    let branch = current_branch()?;
    let found_pr = find_pr_for_branch(&branch)?;
    ensure_branch_in_sync(&branch)?;

    if let Some(pr) = found_pr {
        commit_messages_from_pr(pr, &args.repo)
    } else {
        commit_messages_from_local_branch()
    }
}

fn current_branch() -> Result<String> {
    let output = run_command(
        ExecRequest::new("git", ["rev-parse", "--abbrev-ref", "HEAD"]),
        None,
    )
    .context("failed to determine current branch")?;
    ensure_success(output, "git rev-parse --abbrev-ref HEAD").and_then(|stdout| {
        let branch = stdout.trim().to_owned();
        if branch.is_empty() || branch == "HEAD" {
            Err(anyhow!("not on a branch (detached HEAD?)"))
        } else {
            Ok(branch)
        }
    })
}

fn find_pr_for_branch(branch: &str) -> Result<Option<u64>> {
    let output = run_command(
        ExecRequest::new(
            "gh",
            [
                "pr",
                "list",
                "--head",
                branch,
                "--json",
                "number",
                "--jq",
                ".[0].number // 0",
            ],
        ),
        None,
    )
    .context("failed to find PR for current branch")?;
    let stdout = ensure_success(output, "gh pr list")?;
    let number = stdout
        .trim()
        .parse::<u64>()
        .with_context(|| format!("failed to parse PR number from gh output: {stdout:?}"))?;
    Ok((number != 0).then_some(number))
}

fn ensure_branch_in_sync(branch: &str) -> Result<()> {
    let _fetch = run_command(ExecRequest::new("git", ["fetch", "origin"]), None)
        .context("failed to fetch origin")?;

    let local = ensure_success(
        run_command(ExecRequest::new("git", ["rev-parse", "HEAD"]), None)
            .context("failed to get local HEAD")?,
        "git rev-parse HEAD",
    )?;

    let remote_ref = format!("origin/{branch}");
    let remote_output = run_command(ExecRequest::new("git", ["rev-parse", &remote_ref]), None)
        .context("failed to get remote HEAD")?;
    if !remote_output.success {
        return Err(anyhow!(
            "{}",
            sync_error_message(branch, SyncError::MissingRemote)
        ));
    }
    let remote = String::from_utf8_lossy(&remote_output.stdout)
        .trim()
        .to_owned();

    if local.trim() != remote {
        return Err(anyhow!(
            "{}",
            sync_error_message(branch, SyncError::Diverged)
        ));
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncError {
    MissingRemote,
    Diverged,
}

fn sync_error_message(branch: &str, error: SyncError) -> String {
    match error {
        SyncError::MissingRemote => {
            format!("remote branch origin/{branch} not found — push the branch first")
        }
        SyncError::Diverged => {
            format!("local HEAD differs from origin/{branch} — push your commits first")
        }
    }
}

fn commit_messages_from_pr(pr: u64, repo: &str) -> Result<Vec<String>> {
    let pr_arg = pr.to_string();
    let output = run_command(
        ExecRequest::new(
            "gh",
            ["pr", "view", &pr_arg, "--repo", repo, "--json", "commits"],
        ),
        None,
    )
    .context("failed to run gh pr view")?;
    let stdout = ensure_success(output, "gh pr view")?;
    let json: serde_json::Value =
        serde_json::from_str(&stdout).context("failed to parse gh JSON output")?;

    let commits = json["commits"]
        .as_array()
        .ok_or_else(|| anyhow!("no commits in PR"))?;

    let mut messages = Vec::with_capacity(commits.len());
    for commit in commits {
        let headline = commit["messageHeadline"].as_str().unwrap_or_default();
        let body = commit["messageBody"].as_str().unwrap_or_default();
        messages.push(format!("{headline}\n\n{body}"));
    }
    Ok(messages)
}

fn commit_messages_from_local_branch() -> Result<Vec<String>> {
    let merge_base = run_command(
        ExecRequest::new("git", ["merge-base", "origin/main", "HEAD"]),
        None,
    )
    .context("failed to run git merge-base")?;
    let base = ensure_success(merge_base, "git merge-base origin/main HEAD")?;
    let base = base.trim();
    if base.is_empty() {
        return Err(anyhow!("could not determine merge-base with origin/main"));
    }

    let range = format!("{base}..HEAD");
    let log_output = run_command(
        ExecRequest::new("git", ["log", "--format=%B%x00", &range]),
        None,
    )
    .context("failed to run git log")?;
    let stdout = ensure_success(log_output, "git log --format=%B%x00")?;
    Ok(commit_messages_from_nul_log(&stdout))
}

fn commit_messages_from_nul_log(log: &str) -> Vec<String> {
    log.split('\0')
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(str::to_owned)
        .collect()
}

fn trailer_block_from_messages(messages: Vec<String>) -> Result<String> {
    let mut trailers = Vec::new();
    let mut seen = HashSet::new();

    for message in messages {
        for (key, value) in interpret_trailers(&message)? {
            let normalized = format!("{}: {}", key.to_lowercase(), value.trim());
            if seen.insert(normalized) {
                trailers.push((key, value));
            }
        }
    }

    Ok(format_trailer_block(trailers))
}

fn interpret_trailers(message: &str) -> Result<Vec<(String, String)>> {
    let output = run_command(
        ExecRequest::new(
            "git",
            [
                "interpret-trailers",
                "--parse",
                "--only-trailers",
                "--unfold",
            ],
        ),
        Some(message),
    )
    .context("failed to run git interpret-trailers")?;
    let stdout = ensure_success(output, "git interpret-trailers")?;

    Ok(stdout
        .lines()
        .filter_map(|line| line.split_once(": "))
        .map(|(key, value)| (key.to_owned(), value.trim().to_owned()))
        .collect())
}

fn format_trailer_block(trailers: Vec<(String, String)>) -> String {
    let mut signed_off = Vec::new();
    let mut co_authored = Vec::new();
    let mut others = Vec::new();

    for (key, value) in trailers {
        match key.to_lowercase().as_str() {
            "signed-off-by" => signed_off.push((key, value)),
            "co-authored-by" => co_authored.push((key, value)),
            _ => others.push((key, value)),
        }
    }

    let mut lines = Vec::new();
    for (key, value) in signed_off.into_iter().chain(co_authored).chain(others) {
        lines.push(format!("{key}: {value}"));
    }
    lines.join("\n")
}

fn append_trailer_block(path: &str, trailer_block: &str) -> Result<()> {
    let mut body = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read body file {path}"))?;
    body.push_str("\n\n");
    body.push_str(trailer_block);
    body.push('\n');
    std::fs::write(path, body).with_context(|| format!("failed to write body file {path}"))
}

#[expect(
    clippy::print_stdout,
    reason = "jackin-pr-trailers is a CLI; the trailer block is its output"
)]
fn print_trailer_block(trailer_block: &str) {
    println!("{trailer_block}");
}

#[expect(
    clippy::print_stderr,
    reason = "jackin-pr-trailers is a CLI; body-file append status belongs on stderr"
)]
fn print_appended_message(path: &str) {
    eprintln!("Appended trailers to {path}");
}

fn ensure_success(output: ExecResult, command: &str) -> Result<String> {
    if output.success {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!("{command} failed: {stderr}"))
    }
}

fn run_command(mut request: ExecRequest, stdin: Option<&str>) -> Result<ExecResult> {
    if let Some(stdin_text) = stdin {
        request.stdin = Some(stdin_text.as_bytes().to_vec());
        request.stdin_mode = StdioMode::Capture;
    }
    jackin_process::exec_sync(&request).context("failed to run command")
}

#[cfg(test)]
mod tests;

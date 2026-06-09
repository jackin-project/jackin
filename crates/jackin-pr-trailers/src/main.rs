use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::collections::HashSet;
use std::process::Command;

#[derive(Parser)]
#[command(name = "jackin-pr-trailers", about = "Extract git trailers from PR commits for squash merge messages")]
struct Args {
    /// GitHub PR number. If omitted, auto-detects the current branch name,
    /// finds the corresponding PR (if any), verifies the branch is in sync with
    /// remote, and extracts trailers from the branch's commits (since merge-base
    /// with origin/main).
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

    if args.pr.is_none() {
        // Get current branch name
        let branch_out = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .context("failed to determine current branch")?;
        let branch = String::from_utf8_lossy(&branch_out.stdout).trim().to_string();
        if branch.is_empty() || branch == "HEAD" {
            return Err(anyhow!("not on a branch (detached HEAD?)"));
        }

        // From the branch name, validate if we have a pull request for that and find this pull request.
        let pr_find = Command::new("gh")
            .args([
                "pr", "list",
                "--head", &branch,
                "--json", "number",
                "--jq", ".[0].number // 0",
            ])
            .output()
            .context("failed to find PR for current branch")?;
        let pr_str = String::from_utf8_lossy(&pr_find.stdout).trim().to_string();
        let found_pr: u64 = pr_str.parse().unwrap_or(0);
        if found_pr != 0 {
            eprintln!("Found PR #{} for branch {}", found_pr, branch);
        } else {
            eprintln!("No open PR found for branch {} (proceeding with branch extraction)", branch);
        }

        // Then compare all the commits in this branch to the remote server.
        // (best effort fetch)
        let _ = Command::new("git").args(["fetch", "origin"]).status();

        let local_out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .context("failed to get local HEAD")?;
        let local = String::from_utf8_lossy(&local_out.stdout).trim().to_string();

        let remote_ref = format!("origin/{}", branch);
        let remote_out = Command::new("git")
            .args(["rev-parse", &remote_ref])
            .output();
        let remote = match remote_out {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => {
                eprintln!(
                    "Branch {} is different than the remote branch. You need to push those changes. Otherwise we cannot create the extract, since you have dirty things that are not extracted.",
                    branch
                );
                std::process::exit(1);
            }
        };

        if local != remote {
            eprintln!(
                "Branch {} is different than the remote branch. You need to push those changes. Otherwise we cannot create the extract, since you have dirty things that are not extracted.",
                branch
            );
            std::process::exit(1);
        }
    }

    let commit_messages = if let Some(pr) = args.pr {
        // Fetch via gh for exact PR commits (handles force-pushes, etc.)
        let output = Command::new("gh")
            .args([
                "pr", "view", &pr.to_string(),
                "--repo", &args.repo,
                "--json", "commits",
            ])
            .output()
            .context("failed to run gh pr view")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("gh pr view failed: {}", stderr));
        }

        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .context("failed to parse gh JSON output")?;

        let commits = json["commits"]
            .as_array()
            .ok_or_else(|| anyhow!("no commits in PR"))?;

        let mut msgs = vec![];
        for commit in commits {
            let headline = commit["messageHeadline"].as_str().unwrap_or("");
            let body = commit["messageBody"].as_str().unwrap_or("");
            msgs.push(format!("{}\n\n{}", headline, body));
        }
        msgs
    } else {
        // Use current branch (already verified above) as the "PR" source.
        // Get commits since merge-base with origin/main.
        let merge_base = Command::new("git")
            .args(["merge-base", "origin/main", "HEAD"])
            .output()
            .context("failed to run git merge-base")?;

        if !merge_base.status.success() {
            let stderr = String::from_utf8_lossy(&merge_base.stderr);
            return Err(anyhow!("git merge-base failed (is origin/main fetched and are you on a feature branch?): {}", stderr));
        }

        let base = String::from_utf8_lossy(&merge_base.stdout).trim().to_string();
        if base.is_empty() {
            return Err(anyhow!("could not determine merge-base with origin/main"));
        }

        let log_output = Command::new("git")
            .args(["log", "--pretty=fuller", &format!("{}..HEAD", base)])
            .output()
            .context("failed to run git log")?;

        if !log_output.status.success() {
            let stderr = String::from_utf8_lossy(&log_output.stderr);
            return Err(anyhow!("git log failed: {}", stderr));
        }

        let log_str = String::from_utf8_lossy(&log_output.stdout);
        parse_commit_messages_from_git_log(&log_str)
    };

    let mut trailers: Vec<(String, String)> = vec![];
    let mut seen = HashSet::new();

    for msg in commit_messages {
        let commit_trailers = extract_trailers(&msg);
        for (key, value) in commit_trailers {
            let norm = format!("{}: {}", key.to_lowercase(), value.trim());
            if !seen.contains(&norm) {
                seen.insert(norm);
                trailers.push((key, value));
            }
        }
    }

    // Output in a nice order: Signed-off-by first, then Co-authored-by, then others
    let mut signed_off = vec![];
    let mut co_authored = vec![];
    let mut others = vec![];

    for (key, value) in trailers {
        let lower = key.to_lowercase();
        if lower == "signed-off-by" {
            signed_off.push((key, value));
        } else if lower == "co-authored-by" {
            co_authored.push((key, value));
        } else {
            others.push((key, value));
        }
    }

    let mut trailer_block = String::new();
    if !signed_off.is_empty() || !co_authored.is_empty() || !others.is_empty() {
        trailer_block.push_str("\n\n");
        for (k, v) in &signed_off {
            trailer_block.push_str(&format!("{}: {}\n", k, v));
        }
        for (k, v) in &co_authored {
            trailer_block.push_str(&format!("{}: {}\n", k, v));
        }
        for (k, v) in &others {
            trailer_block.push_str(&format!("{}: {}\n", k, v));
        }
        trailer_block.pop(); // remove trailing \n
    }

    if let Some(path) = &args.body_file {
        if !trailer_block.is_empty() {
            use std::fs::OpenOptions;
            use std::io::Write;
            let mut file = OpenOptions::new()
                .append(true)
                .open(path)
                .with_context(|| format!("failed to open/append to body file {}", path))?;
            writeln!(file, "{}", trailer_block)?;
            eprintln!("Appended trailers to {}", path);
        }
    } else if !trailer_block.is_empty() {
        println!("{}", trailer_block.trim_start());
    }

    Ok(())
}

/// Parse full commit messages (subject + body + trailers) from `git log --pretty=fuller` output.
fn parse_commit_messages_from_git_log(log: &str) -> Vec<String> {
    let mut messages = vec![];
    let mut current = String::new();
    let mut in_body = false;

    for line in log.lines() {
        if line.starts_with("commit ") {
            if !current.trim().is_empty() {
                messages.push(current.trim().to_string());
            }
            current.clear();
            in_body = false;
            continue;
        }

        if line.starts_with("Author:") || line.starts_with("AuthorDate:") ||
           line.starts_with("Commit:") || line.starts_with("CommitDate:") {
            continue;
        }

        if line.trim().is_empty() && !in_body {
            in_body = true;
            continue;
        }

        if in_body {
            current.push_str(line);
            current.push('\n');
        }
    }

    if !current.trim().is_empty() {
        messages.push(current.trim().to_string());
    }

    messages
}

/// Simple trailer extractor.
/// Collects consecutive trailer-like lines from the end of the message
/// (after the last blank line / body). Deduplicates by normalized key+value.
fn extract_trailers(message: &str) -> Vec<(String, String)> {
    let mut collected = vec![];
    let lines: Vec<&str> = message.lines().rev().collect();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !collected.is_empty() {
                break; // blank line separates body from trailers
            }
            continue;
        }

        if let Some((key, value)) = parse_trailer_line(trimmed) {
            collected.push((key, value));
        } else {
            break; // hit body or non-trailer line
        }
    }

    collected.reverse();

    // dedup preserving order (case-insensitive key, trimmed value)
    let mut seen = HashSet::new();
    let mut result = vec![];
    for (key, value) in collected {
        let norm = format!("{}:{}", key.to_lowercase(), value.trim().to_lowercase());
        if !seen.contains(&norm) {
            seen.insert(norm);
            result.push((key, value));
        }
    }
    result
}

fn parse_trailer_line(line: &str) -> Option<(String, String)> {
    // Support "Key: value"
    if let Some(colon_pos) = line.find(": ") {
        let key = line[..colon_pos].trim().to_string();
        let value = line[colon_pos + 2..].trim().to_string();
        if is_valid_trailer_key(&key) && !value.is_empty() {
            return Some((key, value));
        }
    }

    // Support "Key #value" (some trailers use this)
    if let Some(hash_pos) = line.find(" #") {
        let key = line[..hash_pos].trim().to_string();
        let value = line[hash_pos + 2..].trim().to_string();
        if is_valid_trailer_key(&key) && !value.is_empty() {
            return Some((key, value));
        }
    }

    None
}

fn is_valid_trailer_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '-' || c == ' ' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_trailers_basic() {
        let msg = r#"feat: something

This is the body.

Signed-off-by: Alice <alice@example.com>
Co-authored-by: Bob <bob@example.com>
"#;
        let t = extract_trailers(msg);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0], ("Signed-off-by".to_string(), "Alice <alice@example.com>".to_string()));
        assert_eq!(t[1], ("Co-authored-by".to_string(), "Bob <bob@example.com>".to_string()));
    }

    #[test]
    fn test_dedup() {
        let msg = r#"feat: foo

Signed-off-by: Alice <a@example.com>
Signed-off-by: Alice <a@example.com>
"#;
        let t = extract_trailers(msg);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn test_no_trailers() {
        let msg = "feat: foo\n\nJust body.";
        let t = extract_trailers(msg);
        assert!(t.is_empty());
    }
}

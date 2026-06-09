use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::collections::HashSet;
use std::process::Command;

#[derive(Parser)]
#[command(name = "jackin-pr-trailers", about = "Extract git trailers from PR commits for squash merge messages")]
struct Args {
    /// GitHub PR number. If omitted, extracts trailers from commits on the current
    /// branch (the ones since merge-base with origin/main), as if it were the PR branch.
    #[arg(short, long)]
    pr: Option<u64>,

    /// Repository in owner/repo form
    #[arg(short, long, default_value = "jackin-project/jackin")]
    repo: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

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
        // No PR provided: use current branch as the "PR" source.
        // Get commits since merge-base with origin/main (the ones that would be in the PR).
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

    for (k, v) in signed_off {
        println!("{}: {}", k, v);
    }
    for (k, v) in co_authored {
        println!("{}: {}", k, v);
    }
    for (k, v) in others {
        println!("{}: {}", k, v);
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

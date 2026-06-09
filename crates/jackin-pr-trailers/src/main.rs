use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::collections::HashSet;
use std::process::Command;

#[derive(Parser)]
#[command(name = "jackin-pr-trailers", about = "Extract git trailers from PR commits for squash merge messages")]
struct Args {
    /// PR number
    #[arg(short, long)]
    pr: u64,

    /// Repository in owner/repo form
    #[arg(short, long, default_value = "jackin-project/jackin")]
    repo: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Fetch commits via gh (assumes gh is installed and authenticated)
    let output = Command::new("gh")
        .args([
            "pr", "view", &args.pr.to_string(),
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

    let mut trailers: Vec<(String, String)> = vec![];
    let mut seen = HashSet::new();

    for commit in commits {
        let headline = commit["messageHeadline"].as_str().unwrap_or("");
        let body = commit["messageBody"].as_str().unwrap_or("");
        let full_msg = format!("{}\n\n{}", headline, body);

        let commit_trailers = extract_trailers(&full_msg);
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

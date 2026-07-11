//! Brand-prose gate: `cargo xtask docs brand`.
//!
//! Enforces RULES.md: product name is always `jackin❯` in prose. Forbidden:
//! `jackin'`, `Jackin`, `Jackin'`. Bare `jackin` is legal for code identifiers.
//! Code fences, inline code spans, and URL tokens are stripped before matching.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Allowlist of `path:substring` pairs that may carry forbidden spellings in
/// prose (rule-example sentences). Empty at birth — fence/inline stripping
/// covers RULES.md / AGENTS.md examples which only use backticks.
const ALLOWLIST: &[(&str, &str)] = &[];

/// Forbidden brand spellings (case-sensitive; `Jackin` is its own entry).
const FORBIDDEN: &[&str] = &["jackin'", "Jackin'", "Jackin"];

pub(super) fn check_brand(root: &Path) -> Result<()> {
    let mut files = Vec::new();
    collect_prose_files(root, &mut files)?;
    files.sort();

    let mut problems = Vec::new();
    for path in &files {
        let rel = relative(root, path);
        let text =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let stripped = strip_code_regions(&text);
        for (line_no, line) in stripped.lines().enumerate() {
            for token in FORBIDDEN {
                if !line.contains(token) {
                    continue;
                }
                if is_allowlisted(&rel, line) {
                    continue;
                }
                if *token == "Jackin" && !contains_standalone_jackin_capital(line) {
                    continue;
                }
                problems.push(format!(
                    "{rel}:{}: forbidden brand spelling `{token}` — write `jackin❯`; for possessives rewrite the sentence (RULES.md). matched line: {}",
                    line_no + 1,
                    line.trim()
                ));
            }
        }
    }

    if problems.is_empty() {
        emit(&format!("brand gate OK — {} files scanned", files.len()));
        return Ok(());
    }
    bail!(
        "{} brand violation(s):\n  {}",
        problems.len(),
        problems.join("\n  ")
    )
}

fn contains_standalone_jackin_capital(line: &str) -> bool {
    let bytes = line.as_bytes();
    let needle = b"Jackin";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let after = i + needle.len();
            let after_ok = after == bytes.len()
                || (!bytes[after].is_ascii_alphanumeric() && bytes[after] != b'_');
            if before_ok && after_ok {
                return true;
            }
            i += needle.len();
        } else {
            i += 1;
        }
    }
    false
}

fn is_allowlisted(rel: &str, line: &str) -> bool {
    ALLOWLIST
        .iter()
        .any(|(path, substr)| rel == *path && line.contains(substr))
}

/// Strip fenced code blocks, inline `` `...` `` spans, and bare URL tokens.
pub(super) fn strip_code_regions(text: &str) -> String {
    let without_fences = strip_fenced_blocks(text);
    let without_inline = strip_inline_code(&without_fences);
    strip_urls(&without_inline)
}

fn strip_fenced_blocks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_fence = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            out.push('\n');
            continue;
        }
        if !in_fence {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn strip_inline_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '`' {
            for next in chars.by_ref() {
                if next == '`' {
                    break;
                }
            }
            out.push(' ');
            continue;
        }
        out.push(c);
    }
    out
}

fn strip_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(idx) = rest.find("http") {
        out.push_str(&rest[..idx]);
        let after = &rest[idx..];
        let end = after
            .find(|c: char| c.is_whitespace() || c == ')' || c == ']' || c == '>')
            .unwrap_or(after.len());
        out.push(' ');
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn collect_prose_files(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let path = entry?.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
    let crates_dir = root.join("crates");
    if crates_dir.is_dir() {
        for entry in fs::read_dir(&crates_dir)
            .with_context(|| format!("reading {}", crates_dir.display()))?
        {
            let path = entry?.path();
            if !path.is_dir() {
                continue;
            }
            for name in ["README.md", "AGENTS.md"] {
                let candidate = path.join(name);
                if candidate.is_file() {
                    out.push(candidate);
                }
            }
        }
        let crates_agents = crates_dir.join("AGENTS.md");
        if crates_agents.is_file() {
            out.push(crates_agents);
        }
    }
    let content = root.join("docs/content");
    if content.is_dir() {
        walk_mdx(&content, out)?;
    }
    Ok(())
}

fn walk_mdx(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| {
                matches!(
                    n.to_str(),
                    Some("node_modules" | ".output" | ".tanstack" | ".astro")
                )
            }) {
                continue;
            }
            walk_mdx(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext == "mdx" || ext == "md")
        {
            out.push(path);
        }
    }
    Ok(())
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).map_or_else(
        |_| path.to_string_lossy().into_owned(),
        |p| p.to_string_lossy().replace('\\', "/"),
    )
}

fn emit(line: &str) {
    #[expect(
        clippy::print_stdout,
        reason = "jackin-xtask is a CLI; the brand report is its output"
    )]
    {
        println!("{line}");
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn strips_fenced_blocks() {
        let text = "prose jackin'\n```\njackin'\n```\nmore";
        let stripped = strip_code_regions(text);
        assert!(!stripped.contains("```"));
        assert_eq!(stripped.matches("jackin'").count(), 1);
    }

    #[test]
    fn strips_inline_code() {
        let text = "see `jackin'` and Jackin in prose";
        let stripped = strip_code_regions(text);
        assert!(!stripped.contains("`jackin'`"));
        assert!(stripped.contains("Jackin"));
    }

    #[test]
    fn strips_urls() {
        let text = "link http://example.com/jackin' end";
        let stripped = strip_code_regions(text);
        assert!(!stripped.contains("jackin'"));
        assert!(stripped.contains("end"));
    }

    #[test]
    fn detects_real_violation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("NOTE.md"), "The jackin' product is great.\n").unwrap();
        let err = check_brand(root).unwrap_err().to_string();
        assert!(err.contains("jackin'"), "{err}");
        assert!(err.contains("NOTE.md"), "{err}");
    }

    #[test]
    fn clean_file_passes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("NOTE.md"), "The jackin❯ product is great.\n").unwrap();
        check_brand(root).unwrap();
    }
}

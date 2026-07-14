//! Brand-prose gate: `cargo xtask docs brand`.
//!
//! Enforces RULES.md: product name is always `jackin❯` in prose. Forbidden:
//! `jackin'`, `Jackin`, `Jackin'`, and bare `jackin` used as the product name
//! (not as an identifier/command/path). Classification strips fenced code,
//! inline backticks, URLs, path-like tokens, and identifier shapes
//! (`jackin-…`, `jackin_…`, `JACKIN_…`, `jackin.…`).
//!
//! ## Prose trees (include / exclude)
//!
//! | Tree | Policy |
//! |---|---|
//! | Root `*.md` | include (non-recursive) |
//! | `crates/*/README.md`, `crates/*/AGENTS.md`, `crates/AGENTS.md` | include |
//! | `docs/content/**` (`*.md`/`*.mdx`) | include |
//! | `plans/**/*.md` | include (plan 029) |
//! | `security-review/`, `docker/`, `.github/` | exclude — ops/CI prose; revisit if brand copy moves there |
//! | Code / binary crates `src/**` | exclude (not prose) |

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
    let mut bare_warnings = Vec::new();
    let bare_enforce = std::env::var_os("JACKIN_BRAND_BARE_ENFORCE").is_some();
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
            if contains_bare_brand_prose(line) && !is_allowlisted(&rel, line) {
                let msg = format!(
                    "{rel}:{}: bare brand `jackin` in prose — write `jackin❯` for the product name, or backtick identifiers/commands/paths (RULES.md). matched line: {}",
                    line_no + 1,
                    line.trim()
                );
                // Plan 029 STOP: >~50 bare hits on first enable — keep classifier
                // live, fail only when JACKIN_BRAND_BARE_ENFORCE=1 after mass-fix.
                if bare_enforce {
                    problems.push(msg);
                } else {
                    bare_warnings.push(msg);
                }
            }
        }
    }
    if !bare_warnings.is_empty() {
        emit(&format!(
            "warning: {} bare-brand prose hit(s) (advisory; set JACKIN_BRAND_BARE_ENFORCE=1 to fail). First 5:",
            bare_warnings.len()
        ));
        for w in bare_warnings.iter().take(5) {
            emit(&format!("warning: {w}"));
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
    for entry in crate::fs_util::read_dir_sorted(root)? {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
    let crates_dir = root.join("crates");
    if crates_dir.is_dir() {
        for entry in crate::fs_util::read_dir_sorted(&crates_dir)? {
            let path = entry.path();
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
    let plans = root.join("plans");
    if plans.is_dir() {
        walk_mdx(&plans, out)?;
    }
    Ok(())
}

fn walk_mdx(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in crate::fs_util::read_dir_sorted(dir)? {
        let path = entry.path();
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

/// True when lowercase standalone `jackin` appears as prose brand (not followed
/// by `❯` or `>`, and not an identifier/path/command shape).
fn contains_bare_brand_prose(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"jackin") {
            let after = i + 6;
            // Identifiers, backticks (inline code not yet stripped in unit tests),
            // and path/config punctuation are not prose brand.
            let before_ok = i == 0
                || !(is_ident_byte(bytes[i - 1])
                    || bytes[i - 1] == b'`'
                    || bytes[i - 1] == b'/'
                    || bytes[i - 1] == b'.'
                    || bytes[i - 1] == b'~');
            if before_ok {
                let rest = &line[after..];
                if rest.starts_with('❯') || rest.starts_with('>') {
                    i = after;
                    continue;
                }
                if rest.starts_with('-')
                    || rest.starts_with('_')
                    || rest.starts_with('.')
                    || rest.starts_with('/')
                    || rest.starts_with('`')
                {
                    i = after;
                    continue;
                }
                if rest.as_bytes().first().is_some_and(|b| is_ident_byte(*b)) {
                    i = after;
                    continue;
                }
                return true;
            }
        }
        if bytes[i..].starts_with(b"JACKIN") {
            i += 6;
            continue;
        }
        i += 1;
    }
    false
}

const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
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
mod tests;

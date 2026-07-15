//! Documented-command drift gate: every fenced `jackin …` invocation in the
//! docs tree must parse against the real clap command tree.
//!
//! Roadmap Phase 5 item 11: command invocations parse against clap and the
//! persisted config-field inventory matches the configuration reference.
//!
//! Research/speculative pages under paths containing `research` are excluded
//! (operator ruling from the first reviewed implementation): those documents
//! intentionally describe unbuilt surface.

// Tests allow unwrap/expect/panic via clippy.toml valves; no module-level expect.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use clap::error::ErrorKind;
use jackin::cli::Cli;
use syn::{Fields, Item, Visibility};

const CONFIG_KEY_MARKER: &str = "<!-- config-key: ";

fn schema_config_keys(source: &str) -> syn::Result<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    for item in syn::parse_file(source)?.items {
        let Item::Struct(item) = item else { continue };
        if !matches!(item.vis, Visibility::Public(_)) || !derives_serde(&item.attrs)? {
            continue;
        }
        let Fields::Named(fields) = item.fields else {
            continue;
        };
        let struct_name = item.ident;
        for field in fields.named {
            if matches!(field.vis, Visibility::Public(_))
                && let Some(ident) = field.ident
            {
                keys.insert(format!("{struct_name}.{ident}"));
            }
        }
    }
    Ok(keys)
}

fn derives_serde(attrs: &[syn::Attribute]) -> syn::Result<bool> {
    for attr in attrs {
        if !attr.path().is_ident("derive") {
            continue;
        }
        let mut found = false;
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("Serialize") || meta.path.is_ident("Deserialize") {
                found = true;
            }
            Ok(())
        })?;
        if found {
            return Ok(true);
        }
    }
    Ok(false)
}

fn documented_config_keys(source: &str) -> BTreeSet<String> {
    source
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix(CONFIG_KEY_MARKER)?
                .strip_suffix(" -->")
                .map(str::to_owned)
        })
        .collect()
}

fn config_key_drift(
    schema: &BTreeSet<String>,
    documented: &BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    (
        documented.difference(schema).cloned().collect(),
        schema.difference(documented).cloned().collect(),
    )
}

/// Deliberately unparseable illustrative invocations. Shrink-only: every
/// entry must still match a candidate at `(file, line)` or the test fails.
/// Reasons are required.
const SKIP: &[(&str, u32, &str)] = &[
    // Add only when an invocation is intentionally illustrative and cannot
    // parse against today's clap tree. Prefer fixing docs.
];

/// Fence languages the extractor treats as shell-like command surfaces.
const SHELL_FENCE_LANGS: &[&str] = &["", "sh", "bash", "shell", "console", "text"];

/// Shell operators / expansions that make a line a compound shell statement
/// rather than a plain `jackin` argv.
fn is_compound_shell(line: &str) -> bool {
    line.contains('|')
        || line.contains("&&")
        || line.contains('>')
        || line.contains("$(")
        || line.contains('`')
}

/// Walk MDX text; return `(1-based line of the first physical line, raw
/// joined command line)` for every fenced `jackin` invocation.
fn extract_invocations(mdx: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = mdx.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if !trimmed.starts_with("```") {
            i += 1;
            continue;
        }
        let info = trimmed[3..].trim();
        let lang = info.split_whitespace().next().unwrap_or("");
        i += 1;
        if !SHELL_FENCE_LANGS.contains(&lang) {
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                i += 1;
            }
            if i < lines.len() {
                i += 1;
            }
            continue;
        }
        while i < lines.len() {
            let fence_line = lines[i];
            if fence_line.trim_start().starts_with("```") {
                i += 1;
                break;
            }
            // Join backslash continuations before matching.
            let start_line = i + 1;
            let mut joined = fence_line.to_owned();
            while joined.trim_end().ends_with('\\') {
                joined = joined.trim_end().trim_end_matches('\\').to_owned();
                joined.push(' ');
                i += 1;
                if i >= lines.len() || lines[i].trim_start().starts_with("```") {
                    break;
                }
                joined.push_str(lines[i].trim_start());
            }
            if let Some(cmd) = match_jackin_line(&joined) {
                out.push((start_line, cmd));
            }
            i += 1;
        }
    }
    out
}

/// If `line` is a (possibly env-prefixed, `$ `-prompted) `jackin …` invocation,
/// return the command portion starting at `jackin`. Skip `jackin-dev` /
/// `jackin-capsule` / brand-chevron and compound shell lines.
fn match_jackin_line(line: &str) -> Option<String> {
    let mut s = line.trim();
    if s.is_empty() || s.starts_with('#') {
        return None;
    }
    if let Some(rest) = s.strip_prefix("$ ") {
        s = rest.trim_start();
    }
    // Strip leading NAME=value env prefixes.
    loop {
        if s.is_empty() {
            return None;
        }
        // ENV_NAME=value or ENV_NAME="value"
        if let Some(eq) = s.find('=') {
            let name = &s[..eq];
            if !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && !name.contains(' ')
            {
                let rest = &s[eq + 1..];
                let after_value = if let Some(stripped) = rest.strip_prefix('"') {
                    let end = stripped.find('"').map(|i| i + 1)?;
                    &stripped[end..]
                } else if let Some(stripped) = rest.strip_prefix('\'') {
                    let end = stripped.find('\'').map(|i| i + 1)?;
                    &stripped[end..]
                } else {
                    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                    &rest[end..]
                };
                s = after_value.trim_start();
                continue;
            }
        }
        break;
    }
    // Word-boundary: literal `jackin ` then not `-` (jackin-dev/capsule).
    if !s.starts_with("jackin ") {
        // bare `jackin` with nothing else, or jackin-…
        if s == "jackin" {
            return Some("jackin".to_owned());
        }
        return None;
    }
    // Reject jackin-dev / jackin-capsule / jackin❯ — already excluded by the
    // space after jackin, but also reject brand chevron attached weirdly.
    if s.starts_with("jackin-") || s.starts_with("jackin❯") {
        return None;
    }
    // Sample-output version lines (`jackin 0.6.0-dev`) and prose inside
    // fences (`jackin (Tier 5) provides…`) are not invocations.
    let after = s["jackin ".len()..].trim_start();
    if after.is_empty() {
        // bare `jackin` with trailing space only
    } else {
        let first = after.split_whitespace().next().unwrap_or("");
        if first
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '(')
        {
            return None;
        }
    }
    if is_compound_shell(s) {
        return None;
    }
    // Strip trailing `# comment`
    let s = if let Some(idx) = s.find(" #") {
        s[..idx].trim_end()
    } else {
        s.trim_end()
    };
    // Strip trailing help-prose after a wide column gap (sample output
    // tables: `jackin status --detail              include per-instance…`).
    let s = strip_trailing_help_prose(s);
    Some(s.to_owned())
}

/// Drop man-page style trailing description after two-or-more spaces when the
/// remainder is not flag/arg shaped.
fn strip_trailing_help_prose(s: &str) -> &str {
    if let Some(idx) = s.find("  ") {
        let head = s[..idx].trim_end();
        let tail = s[idx..].trim_start();
        // Keep if tail looks like more argv (starts with - or < or [).
        if tail.starts_with('-') || tail.starts_with('<') || tail.starts_with('[') {
            return s;
        }
        // Otherwise treat as column-aligned prose.
        if !tail.is_empty() && tail.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            return head;
        }
    }
    s
}

/// Expand man-page synopsis notation into plain argv structure so clap can
/// validate the shape. Optional markers are dropped; required placeholders
/// become `x`.
fn expand_synopsis_notation(cmd: &str) -> String {
    let mut s = cmd.to_owned();
    // Drop value-alternative markers inside already-compound-skipped lines —
    // when we get here `|` was not present as a shell pipe (e.g. none).
    // Remove optional flag groups: [--foo], [--foo BAR], [--format human|json]
    // handled by compound skip when `|` present; simple [--flag] drop here.
    loop {
        let before = s.clone();
        // [OPTIONS] / [options]
        s = s.replace("[OPTIONS]", "").replace("[options]", "");
        // Nested optional positionals: [WORKSPACE [INSTANCE_ID]] → x x
        // Simple: repeatedly unwrap innermost [TOKEN] forms.
        if let Some(open) = s.rfind('[')
            && let Some(rel_close) = s[open..].find(']')
        {
            let close = open + rel_close;
            let inner = s[open + 1..close].trim();
            let replacement = if inner.starts_with("--") || inner.starts_with('-') {
                // optional flag group — drop
                String::new()
            } else if inner.eq_ignore_ascii_case("OPTIONS") {
                String::new()
            } else if inner.contains('[') {
                // shouldn't happen with rfind innermost
                "x".to_owned()
            } else {
                // optional positional shown — include as placeholder
                "x".to_owned()
            };
            s = format!("{}{}{}", &s[..open], replacement, &s[close + 1..]);
            s = s.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        if s == before {
            break;
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Normalize a raw `jackin …` line into clap argv tokens.
fn normalize_to_tokens(cmd: &str) -> Vec<String> {
    let expanded = expand_synopsis_notation(cmd);
    let tokens = shell_split(&expanded);
    tokens
        .into_iter()
        .filter_map(|tok| {
            if tok.is_empty() {
                return None;
            }
            // Angle-bracket placeholders → x
            if tok.starts_with('<') && tok.ends_with('>') && tok.len() >= 2 {
                return Some("x".to_owned());
            }
            // Home / env expansions that clap only needs as structure.
            if tok.starts_with('~') || tok.starts_with('$') || tok.contains('$') {
                return Some("x".to_owned());
            }
            Some(tok)
        })
        .collect()
}

/// Minimal shell-style splitter honoring single/double quotes. No escapes
/// beyond the backslash-continuation already joined upstream.
fn shell_split(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let chars = input.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    for c in chars {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn docs_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/content/docs")
}

fn is_research_path(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s.contains("research")
    })
}

fn rel_docs_path(path: &Path) -> String {
    let root = docs_root();
    path.strip_prefix(&root).map_or_else(
        |_| path.to_string_lossy().into_owned(),
        |p| p.to_string_lossy().into_owned(),
    )
}

fn walk_mdx_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                if is_research_path(&path) {
                    continue;
                }
                walk(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("mdx") {
                out.push(path);
            }
        }
    }
    walk(root, &mut files);
    files.sort();
    files
}

#[test]
fn extractor_shapes() {
    // prompt + plain
    let mdx = "```sh\n$ jackin doctor\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].1, "jackin doctor");

    // env-prefix
    let mdx = "```bash\nJACKIN_TELEMETRY_LEVEL=trace jackin console --debug\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].1, "jackin console --debug");

    // continuation
    let mdx =
        "```\njackin config mount add gradle-cache \\\n  --src ~/x \\\n  --dst /jackin/x\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    assert!(v[0].1.contains("jackin config mount add gradle-cache"));
    assert!(v[0].1.contains("--dst /jackin/x"));
    assert!(!v[0].1.contains('\\'));

    // placeholder + tilde
    let mdx = "```sh\njackin role create ChainArgos/Rustacean \"$HOME/Projects\"\njackin load foo . --workdir ~/Projects/my-app\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 2);
    let toks = normalize_to_tokens(&v[0].1);
    assert_eq!(toks[0], "jackin");
    assert!(toks.iter().any(|t| t == "x"), "quoted $HOME → x: {toks:?}");

    // prose outside fence ignored
    let mdx = "jackin load foo .\n\n```sh\njackin doctor\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].1, "jackin doctor");

    // jackin-dev exclusion
    let mdx = "```sh\njackin-dev pr 12\njackin doctor\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].1, "jackin doctor");

    // compound skip
    let mdx = "```sh\njackin status | head\njackin doctor && true\njackin logs > /tmp/x\njackin doctor\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].1, "jackin doctor");

    // non-shell fence skipped
    let mdx = "```rust\njackin load never\n```\n```toml\njackin = 1\n```\n";
    assert!(extract_invocations(mdx).is_empty());

    // trailing comment
    let mdx = "```sh\njackin doctor # full health check\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v[0].1, "jackin doctor");
}

#[test]
fn docs_command_invocations_parse_against_clap() {
    let root = docs_root();
    assert!(
        root.is_dir(),
        "docs root missing at {} — CARGO_MANIFEST_DIR={}",
        root.display(),
        env!("CARGO_MANIFEST_DIR")
    );

    let files = walk_mdx_files(&root);
    let mut failures: Vec<String> = Vec::new();
    let mut parsed = 0usize;
    let mut skipped_compound = 0usize;
    let mut skip_hits: Vec<bool> = vec![false; SKIP.len()];

    // Recount compounds for the reviewer-facing log (extractor already
    // drops them; re-scan fences to count).
    for path in &files {
        let text = fs::read_to_string(path).unwrap_or_default();
        for line in text.lines() {
            let t = line.trim_start().trim_start_matches("$ ").trim_start();
            // Rough: jackin lines with compound operators anywhere in file
            // that the extractor would have seen inside fences only — count
            // is informational via the extracted set below.
            let _ = t;
        }
        let rel = rel_docs_path(path);
        let invocations = extract_invocations(&text);
        for (line_no, raw) in invocations {
            // Compound count: extract already skipped; track via re-check of
            // nothing. We only have the kept set.
            if let Some(idx) = SKIP
                .iter()
                .position(|(f, l, _)| *f == rel && *l == line_no as u32)
            {
                skip_hits[idx] = true;
                continue;
            }
            let tokens = normalize_to_tokens(&raw);
            match Cli::try_parse_from(&tokens) {
                Ok(_) => parsed += 1,
                Err(e)
                    if matches!(
                        e.kind(),
                        ErrorKind::DisplayHelp
                            | ErrorKind::DisplayVersion
                            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                    ) =>
                {
                    // `--help` / `--version` intentionally surface as Err.
                    parsed += 1;
                }
                Err(e) => {
                    failures.push(format!(
                        "{rel}:{line_no}: `{raw}`\n  tokens={tokens:?}\n  clap: {e}"
                    ));
                }
            }
        }
        // Count compound skips by re-walking fences naively for this file.
        skipped_compound += count_compound_skips(&text);
    }

    // Stale skip detection
    for (i, (f, l, reason)) in SKIP.iter().enumerate() {
        if !skip_hits[i] {
            failures.push(format!(
                "stale SKIP entry: {f}:{l} ({reason}) — no candidate at that location"
            ));
        }
    }

    eprintln!(
        "docs_commands: parsed={parsed} failures={} skips_used={} compound_skipped={skipped_compound}",
        failures.len(),
        skip_hits.iter().filter(|h| **h).count(),
    );

    assert!(
        parsed >= 150,
        "parsed count {parsed} < 150 — extractor may be skipping too aggressively (compound_skipped={skipped_compound})"
    );
    assert!(
        SKIP.len() <= 10,
        "SKIP ledger has {} entries; cap is 10 — improve normalization instead",
        SKIP.len()
    );

    assert!(
        failures.is_empty(),
        "{} documented command(s) failed clap parse:\n\n{}\n",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Count fenced `jackin` lines skipped as compound shell (for diagnostics).
fn count_compound_skips(mdx: &str) -> usize {
    let lines: Vec<&str> = mdx.lines().collect();
    let mut n = 0;
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if !trimmed.starts_with("```") {
            i += 1;
            continue;
        }
        let info = trimmed[3..].trim();
        let lang = info.split_whitespace().next().unwrap_or("");
        i += 1;
        if !SHELL_FENCE_LANGS.contains(&lang) {
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                i += 1;
            }
            if i < lines.len() {
                i += 1;
            }
            continue;
        }
        while i < lines.len() {
            if lines[i].trim_start().starts_with("```") {
                i += 1;
                break;
            }
            let mut joined = lines[i].to_owned();
            while joined.trim_end().ends_with('\\') {
                joined = joined.trim_end().trim_end_matches('\\').to_owned();
                joined.push(' ');
                i += 1;
                if i >= lines.len() || lines[i].trim_start().starts_with("```") {
                    break;
                }
                joined.push_str(lines[i].trim_start());
            }
            let mut s = joined.trim();
            if let Some(rest) = s.strip_prefix("$ ") {
                s = rest.trim_start();
            }
            // strip env prefixes loosely
            while let Some(eq) = s.find('=') {
                let name = &s[..eq];
                if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !name.is_empty()
                    && !name.contains(' ')
                    && name
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                {
                    let rest = &s[eq + 1..];
                    let after = rest.find(char::is_whitespace).unwrap_or(rest.len());
                    s = rest[after..].trim_start();
                    continue;
                }
                break;
            }
            if s.starts_with("jackin ") && is_compound_shell(s) {
                n += 1;
            }
            i += 1;
        }
    }
    n
}

#[test]
fn config_key_drift_reports_both_directions() {
    let schema = BTreeSet::from([
        "AppConfig.version".to_owned(),
        "AppConfig.runtime".to_owned(),
    ]);
    let docs = BTreeSet::from([
        "AppConfig.version".to_owned(),
        "AppConfig.removed".to_owned(),
    ]);
    let (documented_but_gone, schema_but_undocumented) = config_key_drift(&schema, &docs);
    assert_eq!(documented_but_gone, ["AppConfig.removed"]);
    assert_eq!(schema_but_undocumented, ["AppConfig.runtime"]);
}

#[test]
fn config_reference_matches_public_schema_fields() -> anyhow::Result<()> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let schema_paths = [
        manifest.join("../jackin-config/src/schema.rs"),
        manifest.join("../jackin-config/src/app_config.rs"),
        manifest.join("../jackin-config/src/auth.rs"),
    ];
    let docs_path = manifest.join("../../docs/content/docs/reference/runtime/configuration.mdx");
    let mut schema = BTreeSet::new();
    for path in &schema_paths {
        schema.extend(schema_config_keys(&fs::read_to_string(path)?)?);
    }
    let documented = documented_config_keys(&fs::read_to_string(&docs_path)?);
    let (documented_but_gone, schema_but_undocumented) = config_key_drift(&schema, &documented);
    assert!(
        documented_but_gone.is_empty() && schema_but_undocumented.is_empty(),
        "config-key drift:\n  documented but gone: {documented_but_gone:#?}\n  schema but undocumented: {schema_but_undocumented:#?}"
    );
    Ok(())
}

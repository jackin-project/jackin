use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use clap::error::ErrorKind;
use jackin::cli::Cli;
use syn::{Fields, Item, Visibility};

const CONFIG_KEY_MARKER: &str = "{/* config-key: ";

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
                .strip_suffix(" */}")
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

const SKIP: &[(&str, u32, &str)] = &[];

const SHELL_FENCE_LANGS: &[&str] = &["", "sh", "bash", "shell", "console", "text"];

fn is_compound_shell(line: &str) -> bool {
    line.contains('|')
        || line.contains("&&")
        || line.contains('>')
        || line.contains("$(")
        || line.contains('`')
}

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

fn match_jackin_line(line: &str) -> Option<String> {
    let mut s = line.trim();
    if s.is_empty() || s.starts_with('#') {
        return None;
    }
    if let Some(rest) = s.strip_prefix("$ ") {
        s = rest.trim_start();
    }
    loop {
        if s.is_empty() {
            return None;
        }
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
    if !s.starts_with("jackin ") {
        if s == "jackin" {
            return Some("jackin".to_owned());
        }
        return None;
    }
    if s.starts_with("jackin-") || s.starts_with("jackin❯") {
        return None;
    }
    let after = s["jackin ".len()..].trim_start();
    if after.is_empty() {
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
    let s = if let Some(idx) = s.find(" #") {
        s[..idx].trim_end()
    } else {
        s.trim_end()
    };
    let s = strip_trailing_help_prose(s);
    Some(s.to_owned())
}

fn strip_trailing_help_prose(s: &str) -> &str {
    if let Some(idx) = s.find("  ") {
        let head = s[..idx].trim_end();
        let tail = s[idx..].trim_start();
        if tail.starts_with('-') || tail.starts_with('<') || tail.starts_with('[') {
            return s;
        }
        if !tail.is_empty() && tail.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            return head;
        }
    }
    s
}

fn expand_synopsis_notation(cmd: &str) -> String {
    let mut s = cmd.to_owned();
    loop {
        let before = s.clone();
        s = s.replace("[OPTIONS]", "").replace("[options]", "");
        if let Some(open) = s.rfind('[')
            && let Some(rel_close) = s[open..].find(']')
        {
            let close = open + rel_close;
            let inner = s[open + 1..close].trim();
            let replacement = if inner.starts_with("--")
                || inner.starts_with('-')
                || inner.eq_ignore_ascii_case("OPTIONS")
            {
                String::new()
            } else {
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

fn normalize_to_tokens(cmd: &str) -> Vec<String> {
    let expanded = expand_synopsis_notation(cmd);
    let mut tokens = shell_split(&expanded).into_iter().peekable();
    let mut normalized = Vec::new();
    while let Some(tok) = tokens.next() {
        if tok == "<"
            && tokens
                .peek()
                .is_some_and(|path| !matches!(path.as_str(), "<" | ">" | "|" | "&&"))
        {
            tokens.next();
            continue;
        }
        if tok.is_empty() {
            continue;
        }
        if tok.starts_with('<') && tok.ends_with('>') && tok.len() >= 2 {
            normalized.push("x".to_owned());
            continue;
        }
        if tok.starts_with('~') || tok.starts_with('$') || tok.contains('$') {
            normalized.push("x".to_owned());
            continue;
        }
        normalized.push(tok);
    }
    normalized
}

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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/content")
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
    let mdx = "```sh\n$ jackin doctor\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].1, "jackin doctor");

    let mdx = "```bash\nJACKIN_TELEMETRY_LEVEL=trace jackin console --debug\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].1, "jackin console --debug");

    let mdx =
        "```\njackin config mount add gradle-cache \\\n  --src ~/x \\\n  --dst /jackin/x\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    assert!(v[0].1.contains("jackin config mount add gradle-cache"));
    assert!(v[0].1.contains("--dst /jackin/x"));
    assert!(!v[0].1.contains('\\'));

    let mdx = "```sh\njackin role create ChainArgos/Rustacean \"$HOME/Projects\"\njackin load foo . --workdir ~/Projects/my-app\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 2);
    let toks = normalize_to_tokens(&v[0].1);
    assert_eq!(toks[0], "jackin");
    assert!(toks.iter().any(|t| t == "x"), "quoted $HOME → x: {toks:?}");

    let mdx = "jackin load foo .\n\n```sh\njackin doctor\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].1, "jackin doctor");

    let mdx = "```sh\njackin-dev pr 12\njackin doctor\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].1, "jackin doctor");

    let mdx = "```sh\njackin status | head\njackin doctor && true\njackin logs > /tmp/x\njackin doctor\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].1, "jackin doctor");

    let mdx = "```rust\njackin load never\n```\n```toml\njackin = 1\n```\n";
    assert!(extract_invocations(mdx).is_empty());

    let mdx = "```sh\njackin doctor # full health check\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v[0].1, "jackin doctor");

    let mdx = "```sh\njackin usage statusline ingest --session-only --format json < statusline.json\n```\n";
    let v = extract_invocations(mdx);
    assert_eq!(v.len(), 1);
    let tokens = normalize_to_tokens(&v[0].1);
    assert_eq!(
        tokens,
        [
            "jackin",
            "usage",
            "statusline",
            "ingest",
            "--session-only",
            "--format",
            "json"
        ]
    );
    Cli::try_parse_from(&tokens).unwrap();

    let tokens = normalize_to_tokens("jackin usage statusline ingest --session-only <");
    assert_eq!(tokens.last().map(String::as_str), Some("<"));
    Cli::try_parse_from(&tokens).unwrap_err();
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

    for path in &files {
        let text = fs::read_to_string(path).unwrap_or_default();
        for line in text.lines() {
            let t = line.trim_start().trim_start_matches("$ ").trim_start();
            let _ = t;
        }
        let rel = rel_docs_path(path);
        let invocations = extract_invocations(&text);
        for (line_no, raw) in invocations {
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
                    parsed += 1;
                }
                Err(e) => {
                    failures.push(format!(
                        "{rel}:{line_no}: `{raw}`\n  tokens={tokens:?}\n  clap: {e}"
                    ));
                }
            }
        }
        skipped_compound += count_compound_skips(&text);
    }

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
fn config_key_markers_use_valid_mdx_comments() {
    let keys = documented_config_keys(
        "{/* config-key: AppConfig.version */}\n{/* config-key: AppConfig.runtime */}\n",
    );
    assert_eq!(
        keys,
        BTreeSet::from([
            "AppConfig.runtime".to_owned(),
            "AppConfig.version".to_owned(),
        ])
    );
}

#[test]
fn config_reference_matches_public_schema_fields() -> anyhow::Result<()> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let schema_paths = [
        manifest.join("../../core/jackin-config/src/schema.rs"),
        manifest.join("../../core/jackin-config/src/app_config.rs"),
        manifest.join("../../core/jackin-config/src/auth.rs"),
        manifest.join("../../core/jackin-config/src/accounts.rs"),
    ];
    let docs_path = manifest.join("../../../docs/content/reference/runtime/configuration.mdx");
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

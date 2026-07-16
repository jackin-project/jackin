//! Stable documentation cache contracts used by GitHub Actions.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::cmd;

#[derive(Args)]
pub(crate) struct DocsContractArgs {
    /// Documentation surface whose cache key is required.
    #[arg(value_enum)]
    kind: ContractKind,
    /// Git revision to inspect.
    #[arg(long, default_value = "HEAD")]
    git_ref: String,
    /// Reproduce the retired full-site link contract during cache migration.
    #[arg(long)]
    legacy: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum ContractKind {
    Link,
    LinkSurface,
    Site,
    Lychee,
}

pub(crate) fn run(args: DocsContractArgs) -> Result<()> {
    let digest = match args.kind {
        ContractKind::Link => link_contract(&args.git_ref, args.legacy)?,
        ContractKind::LinkSurface => surface_contract(&args.git_ref)?,
        ContractKind::Site => site_contract(&args.git_ref)?,
        ContractKind::Lychee => lychee_contract(&args.git_ref)?,
    };
    writeln!(std::io::stdout().lock(), "{digest}")?;
    Ok(())
}

pub(crate) fn run_ci_link_result() -> Result<()> {
    let repository = required_env("REPOSITORY")?;
    let repository_id = required_env("REPOSITORY_ID")?
        .parse::<u64>()
        .context("REPOSITORY_ID is not an integer")?;
    let runner_os = required_env("RUNNER_OS")?;
    let output = required_env("GITHUB_OUTPUT")?;
    let name = format!(
        "docs-links-v2-{runner_os}-{}",
        link_contract("HEAD", false)?
    );

    let mut artifact = find_artifact(&repository, repository_id, &name)?;
    let mut migrated = false;
    if artifact.is_none()
        && let Some(base) = usable_base_sha(env::var("BASE_SHA").ok())
    {
        git_fetch(&base)?;
        if surface_contract("HEAD")? == surface_contract(&base)? {
            let legacy_name = format!("docs-links-v2-{runner_os}-{}", link_contract(&base, true)?);
            artifact = find_artifact(&repository, repository_id, &legacy_name)?;
            if artifact.is_some() {
                migrated = true;
                writeln!(
                    std::io::stdout().lock(),
                    "::notice::reusing the base link proof for an unchanged link surface"
                )?;
            }
        }
    }

    let verified = artifact
        .as_ref()
        .is_some_and(|artifact| artifact_is_recent(artifact).unwrap_or(false));
    let refresh = !verified || migrated;
    fs::write(
        &output,
        format!("name={name}\nrefresh={refresh}\nverified={verified}\n"),
    )
    .with_context(|| format!("writing GitHub output at {output}"))?;
    Ok(())
}

struct Artifact {
    created_at: String,
}

fn find_artifact(repository: &str, repository_id: u64, name: &str) -> Result<Option<Artifact>> {
    let endpoint = format!("repos/{repository}/actions/artifacts?name={name}&per_page=10");
    let mut command = cmd::command("gh");
    command.args(["api", &endpoint]);
    let response: JsonValue = serde_json::from_slice(&cmd::output(&mut command)?)
        .context("parsing GitHub artifact response")?;
    let artifact = response["artifacts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|artifact| artifact["expired"] == JsonValue::Bool(false))
        .filter(|artifact| {
            artifact["workflow_run"]["head_repository_id"].as_u64() == Some(repository_id)
        })
        .filter_map(|artifact| {
            artifact["created_at"].as_str().map(|created_at| Artifact {
                created_at: created_at.to_owned(),
            })
        })
        .max_by(|left, right| left.created_at.cmp(&right.created_at));
    Ok(artifact)
}

fn artifact_is_recent(artifact: &Artifact) -> Result<bool> {
    let mut command = cmd::command("date");
    command.args(["-u", "-d", &artifact.created_at, "+%s"]);
    let created = String::from_utf8(cmd::output(&mut command)?)
        .context("date output is not UTF-8")?
        .trim()
        .parse::<u64>()
        .context("date output is not an epoch")?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs();
    Ok(now.saturating_sub(created) < 259_200)
}

fn git_fetch(git_ref: &str) -> Result<()> {
    let mut command = cmd::command("git");
    command.args(["fetch", "--no-tags", "--depth=1", "origin", git_ref]);
    cmd::run(&mut command)
}

fn usable_base_sha(base: Option<String>) -> Option<String> {
    base.filter(|base| !base.is_empty() && !base.bytes().all(|byte| byte == b'0'))
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is not set"))
}

fn link_contract(git_ref: &str, legacy: bool) -> Result<String> {
    let mut input = Vec::new();
    if legacy {
        input.extend_from_slice(b"docs-link-contract-v1\n");
        push_line(&mut input, &site_contract(git_ref)?);
    } else {
        input.extend_from_slice(b"docs-link-contract-v2\n");
        push_line(&mut input, &surface_contract(git_ref)?);
        push_line(
            &mut input,
            &object_id(git_ref, "crates/jackin-xtask/src/docs/contract.rs")?,
        );
    }
    push_line(&mut input, &lychee_contract(git_ref)?);
    push_line(
        &mut input,
        &object_id(git_ref, "scripts/ci/docs-link-check.sh")?,
    );
    Ok(hash(&input))
}

fn site_contract(git_ref: &str) -> Result<String> {
    let mut input = b"docs-site-contract-v2\n".to_vec();
    for entry in tree(git_ref)? {
        if is_site_input(&entry.path) {
            push_tree_entry(&mut input, &entry);
        }
    }
    input.extend(selected_assignment_lines(
        &blob(git_ref, "mise.toml")?,
        &["bun", "node"],
    ));
    input.extend(selected_tool_sections(
        &blob(git_ref, "mise.lock")?,
        &["bun", "node"],
    ));
    Ok(hash(&input))
}

fn lychee_contract(git_ref: &str) -> Result<String> {
    let config = String::from_utf8(blob(git_ref, "docs/lychee.toml")?)
        .context("docs/lychee.toml is not UTF-8")?;
    let parsed: toml::Value = toml::from_str(&config).context("parsing docs/lychee.toml")?;
    let canonical = canonical_json(parsed);
    let mut input = serde_json::to_vec(&canonical).context("serializing docs/lychee.toml")?;
    input.extend(selected_assignment_lines(
        &blob(git_ref, "mise.toml")?,
        &["lychee"],
    ));
    input.extend(selected_tool_sections(
        &blob(git_ref, "mise.lock")?,
        &["lychee"],
    ));
    Ok(hash(&input))
}

fn surface_contract(git_ref: &str) -> Result<String> {
    let mut input = Vec::new();
    for entry in tree(git_ref)? {
        if is_full_surface_input(&entry.path) {
            push_tree_entry(&mut input, &entry);
        } else if is_link_markup_input(&entry.path) {
            input.extend_from_slice(entry.path.as_bytes());
            input.push(0);
            let content = String::from_utf8(blob(git_ref, &entry.path)?)
                .with_context(|| format!("{} is not UTF-8", entry.path))?;
            input.extend(extract_link_surface(&content));
        }
    }
    Ok(hash(&input))
}

fn extract_link_surface(content: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let mut markdown_target = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if markdown_target {
            output.extend_from_slice(trimmed.as_bytes());
            output.push(b'\n');
            markdown_target = !line.contains(')');
            continue;
        }
        if is_heading(trimmed) || trimmed.starts_with("import ") || trimmed.starts_with("export ") {
            output.extend_from_slice(trimmed.as_bytes());
            output.push(b'\n');
        }
        for target in markdown_targets(line) {
            output.extend_from_slice(b"md:");
            output.extend_from_slice(target.as_bytes());
            output.push(b'\n');
        }
        if let Some((_, target)) = trimmed.split_once("]:")
            && is_reference_definition(trimmed)
        {
            output.extend_from_slice(b"ref:");
            output.extend_from_slice(target.trim().as_bytes());
            output.push(b'\n');
        }
        for url in line
            .split_ascii_whitespace()
            .filter(|token| token.contains("http://") || token.contains("https://"))
        {
            output.extend_from_slice(b"url:");
            output.extend_from_slice(
                url.trim_matches(|ch: char| "<>()[]{}\"'`,.;".contains(ch))
                    .as_bytes(),
            );
            output.push(b'\n');
        }
        for attribute in link_attributes(line) {
            output.extend_from_slice(b"attr:");
            output.extend_from_slice(attribute.as_bytes());
            output.push(b'\n');
        }
        if contains_link_component(line) {
            output.extend_from_slice(b"link-component\n");
        }
        if line.trim_end().ends_with("](") {
            markdown_target = true;
        }
    }
    output
}

fn markdown_targets(line: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    let mut remainder = line;
    while let Some((_, after_open)) = remainder.split_once("](") {
        let Some((target, after_close)) = after_open.split_once(')') else {
            break;
        };
        targets.push(target.trim());
        remainder = after_close;
    }
    targets
}

fn link_attributes(line: &str) -> Vec<String> {
    let mut attributes = Vec::new();
    for name in ["href", "src", "to", "url", "path", "id", "redirect"] {
        let mut remainder = line;
        while let Some(index) = remainder.find(name) {
            remainder = &remainder[index + name.len()..];
            let after_name = remainder.trim_start();
            let Some(after_equals) = after_name.strip_prefix('=') else {
                continue;
            };
            let value = after_equals.trim_start();
            let end = match value.as_bytes().first() {
                Some(b'\"') => value[1..].find('\"').map(|index| index + 2),
                Some(b'\'') => value[1..].find('\'').map(|index| index + 2),
                Some(b'{') => value.find('}').map(|index| index + 1),
                _ => value.find(|ch: char| ch.is_ascii_whitespace() || ch == '>'),
            }
            .unwrap_or(value.len());
            attributes.push(format!("{name}={}", &value[..end]));
            remainder = &value[end..];
        }
    }
    attributes
}

fn is_heading(line: &str) -> bool {
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    (1..=6).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ')
}

fn is_reference_definition(line: &str) -> bool {
    line.starts_with('[') && line.contains("]:")
}

fn contains_link_component(line: &str) -> bool {
    ["a", "Link", "Image", "RepoFile", "Card", "Cards"]
        .iter()
        .any(|name| line.contains(&format!("<{name} ")) || line.contains(&format!("<{name}>")))
}

fn is_site_input(path: &str) -> bool {
    path == "Cargo.toml"
        || (path.starts_with("crates/")
            && (path.ends_with("/Cargo.toml") || path.ends_with("/README.md")))
        || matches!(path, "docs/bun.lock" | "docs/package.json")
        || [
            "docs/src/",
            "docs/content/",
            "docs/public/",
            "docs/scripts/",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || (path.starts_with("docs/") && has_any_extension(path, &["ts", "json"]))
}

fn is_full_surface_input(path: &str) -> bool {
    path == "Cargo.toml"
        || (path.starts_with("crates/") && path.ends_with("/Cargo.toml"))
        || matches!(path, "docs/bun.lock" | "docs/package.json")
        || ["docs/src/", "docs/scripts/", "docs/public/"]
            .iter()
            .any(|prefix| path.starts_with(prefix))
        || (path.starts_with("docs/") && has_any_extension(path, &["ts", "json"]))
}

fn is_link_markup_input(path: &str) -> bool {
    (path.starts_with("crates/") && path.ends_with("/README.md"))
        || (path.starts_with("docs/content/") && has_any_extension(path, &["mdx"]))
}

fn has_any_extension(path: &str, extensions: &[&str]) -> bool {
    Path::new(path).extension().is_some_and(|extension| {
        extensions
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    })
}

struct TreeEntry {
    object: String,
    path: String,
}

fn tree(git_ref: &str) -> Result<Vec<TreeEntry>> {
    let output = git(&["ls-tree", "-r", "-z", git_ref])?;
    output
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let tab = entry
                .iter()
                .position(|byte| *byte == b'\t')
                .context("invalid git ls-tree entry")?;
            let metadata =
                String::from_utf8(entry[..tab].to_vec()).context("invalid tree metadata")?;
            let path = String::from_utf8(entry[tab + 1..].to_vec()).context("non-UTF-8 path")?;
            let object = metadata
                .split_ascii_whitespace()
                .next_back()
                .context("tree entry has no object")?
                .to_owned();
            Ok(TreeEntry { object, path })
        })
        .collect()
}

fn blob(git_ref: &str, path: &str) -> Result<Vec<u8>> {
    git(&["show", &format!("{git_ref}:{path}")])
}

fn object_id(git_ref: &str, path: &str) -> Result<String> {
    let output = git(&["rev-parse", &format!("{git_ref}:{path}")])?;
    Ok(String::from_utf8(output)
        .context("git object ID is not UTF-8")?
        .trim()
        .to_owned())
}

fn git(args: &[&str]) -> Result<Vec<u8>> {
    let mut command = cmd::command("git");
    command.args(args);
    cmd::output(&mut command)
}

fn push_tree_entry(output: &mut Vec<u8>, entry: &TreeEntry) {
    output.extend_from_slice(entry.path.as_bytes());
    output.push(0);
    push_line(output, &entry.object);
}

fn push_line(output: &mut Vec<u8>, line: &str) {
    output.extend_from_slice(line.as_bytes());
    output.push(b'\n');
}

fn selected_assignment_lines(content: &[u8], names: &[&str]) -> Vec<u8> {
    String::from_utf8_lossy(content)
        .lines()
        .filter(|line| {
            names
                .iter()
                .any(|name| line.starts_with(&format!("{name} = ")))
        })
        .flat_map(|line| format!("{line}\n").into_bytes())
        .collect()
}

fn selected_tool_sections(content: &[u8], names: &[&str]) -> Vec<u8> {
    let text = String::from_utf8_lossy(content);
    let mut output = Vec::new();
    let mut selected = false;
    for line in text.lines() {
        let tool_header = line.starts_with("[[tools.");
        if tool_header {
            if selected {
                output.extend_from_slice(line.as_bytes());
                output.push(b'\n');
            }
            selected = names.iter().any(|name| line == format!("[[tools.{name}]]"));
            if selected {
                output.extend_from_slice(line.as_bytes());
                output.push(b'\n');
            }
        } else if selected {
            output.extend_from_slice(line.as_bytes());
            output.push(b'\n');
        }
    }
    output
}

fn canonical_json(value: toml::Value) -> JsonValue {
    match value {
        toml::Value::String(value) => JsonValue::String(value),
        toml::Value::Integer(value) => JsonValue::Number(value.into()),
        toml::Value::Float(value) => {
            serde_json::Number::from_f64(value).map_or(JsonValue::Null, JsonValue::Number)
        }
        toml::Value::Boolean(value) => JsonValue::Bool(value),
        toml::Value::Datetime(value) => JsonValue::String(value.to_string()),
        toml::Value::Array(values) => {
            JsonValue::Array(values.into_iter().map(canonical_json).collect())
        }
        toml::Value::Table(values) => {
            let sorted: BTreeMap<_, _> = values
                .into_iter()
                .map(|(key, value)| (key, canonical_json(value)))
                .collect();
            JsonValue::Object(sorted.into_iter().collect())
        }
    }
}

fn hash(input: &[u8]) -> String {
    hex::encode(Sha256::digest(input))
}

#[cfg(test)]
mod tests;

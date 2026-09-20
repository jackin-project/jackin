// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Read-only enumerator for `.hermes/` credential stores.
//!
//! A store is a directory holding up to three inputs:
//!
//! - `config.yaml`: optional top-level `profiles:` mapping of profile name to
//!   a `{ provider: <slug> }` mapping;
//! - `profiles/*.yaml` (or `*.yml`): one mapping per file, shaped like the
//!   inline entries; the filename stem is the profile name and file entries
//!   override inline entries of the same name;
//! - `auth.json`: provider-keyed secrets using the shared entry shapes
//!   (`{ "type": "api", "key": ... }` and `{ "type": "oauth", ... }`).
//!
//! One [`StoreCandidate`](super::StoreCandidate) is emitted per
//! profile whose provider has a usable `auth.json` entry, sorted by profile
//! name. Profiles without a provider, providers without an entry, and absent
//! inputs yield no candidates rather than errors.
//!
//! `config.yaml` and the profile files are parsed as a minimal mapping-only
//! YAML subset (nested `key: value` mappings, `#` comments, bare and quoted
//! scalars) because this crate has no YAML dependency. Anything outside the
//! subset — sequences, flow styles, anchors, tags, block scalars, tab
//! indentation — fails closed with [`StoreError::Malformed`].

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;

use super::{StoreCandidate, StoreError, StoreKind, read_store_file, select_entry_secret};

/// Maximum YAML input size accepted for enumeration.
const YAML_LIMIT: u64 = 1024 * 1024;
/// Maximum `auth.json` size accepted for enumeration.
const AUTH_JSON_LIMIT: u64 = 1024 * 1024;

/// Enumerate usable profile credentials from a `.hermes/` directory.
///
/// A missing directory, or any missing input within it, yields no
/// candidates. Pure parsing from the given directory: only the documented
/// `config.yaml`, `profiles/`, and `auth.json` locations are read, nothing
/// is written.
///
/// # Errors
///
/// Returns [`StoreError`] for unreadable, oversized, or malformed sources.
pub(crate) fn enumerate_hermes_store(dir: &Path) -> Result<Vec<StoreCandidate>, StoreError> {
    let mut profiles: BTreeMap<String, String> = BTreeMap::new();
    if let Some(bytes) = read_store_file(&dir.join("config.yaml"), YAML_LIMIT)? {
        let text = str::from_utf8(&bytes).map_err(|_| StoreError::Malformed)?;
        collect_inline_profiles(&parse_simple_yaml(text)?, &mut profiles)?;
    }
    collect_file_profiles(&dir.join("profiles"), &mut profiles)?;
    let auth_path = dir.join("auth.json");
    let Some(auth_bytes) = read_store_file(&auth_path, AUTH_JSON_LIMIT)? else {
        return Ok(Vec::new());
    };
    let auth: serde_json::Value =
        serde_json::from_slice(&auth_bytes).map_err(|_| StoreError::Malformed)?;
    let entries = auth.as_object().ok_or(StoreError::Malformed)?;
    let mut candidates = Vec::new();
    for (profile, provider) in &profiles {
        let Some(entry) = entries.get(provider) else {
            continue;
        };
        let Some((kind, field, secret)) = select_entry_secret(entry) else {
            continue;
        };
        candidates.push(StoreCandidate::new(
            StoreKind::Hermes,
            provider.as_str().to_owned(),
            Some(profile.as_str().to_owned()),
            auth_path.clone(),
            kind,
            field.to_owned(),
            secret.to_owned(),
        ));
    }
    Ok(candidates)
}

/// Prove that the Hermes directory contains one profile and one auth entry
/// before a whole-store sync is allowed. The directory provisioner cannot
/// safely rewrite unknown profile/state files, so any extra entry fails closed.
pub(crate) fn validate_single_profile_store(dir: &Path) -> Result<(), StoreError> {
    let candidates = enumerate_hermes_store(dir)?;
    let Some(candidate) = candidates.first() else {
        return Err(StoreError::Unsupported(
            "Hermes credential store has no usable profile",
        ));
    };
    if candidates.len() != 1 {
        return Err(StoreError::Unsupported(
            "Hermes credential store contains multiple profiles",
        ));
    }

    let auth_path = dir.join("auth.json");
    let auth_bytes = read_store_file(&auth_path, AUTH_JSON_LIMIT)?.ok_or(
        StoreError::Unsupported("Hermes credential store has no auth.json"),
    )?;
    let auth: serde_json::Value =
        serde_json::from_slice(&auth_bytes).map_err(|_| StoreError::Malformed)?;
    let entries = auth.as_object().ok_or(StoreError::Malformed)?;
    if entries.len() != 1 || !entries.contains_key(&candidate.provider) {
        return Err(StoreError::Unsupported(
            "Hermes auth.json contains multiple provider entries",
        ));
    }

    let mut profiles = BTreeMap::new();
    if let Some(bytes) = read_store_file(&dir.join("config.yaml"), YAML_LIMIT)? {
        let text = str::from_utf8(&bytes).map_err(|_| StoreError::Malformed)?;
        collect_inline_profiles(&parse_simple_yaml(text)?, &mut profiles)?;
    }
    collect_file_profiles(&dir.join("profiles"), &mut profiles)?;
    if profiles.len() != 1
        || profiles
            .get(candidate.profile.as_deref().unwrap_or_default())
            .is_none_or(|provider| provider != &candidate.provider)
    {
        return Err(StoreError::Unsupported(
            "Hermes credential store contains multiple profile entries",
        ));
    }
    Ok(())
}

/// Merge inline `profiles:` entries from `config.yaml` into `profiles`.
fn collect_inline_profiles(
    doc: &BTreeMap<String, YamlNode>,
    profiles: &mut BTreeMap<String, String>,
) -> Result<(), StoreError> {
    let Some(node) = doc.get("profiles") else {
        return Ok(());
    };
    let YamlNode::Map(inline) = node else {
        return Err(StoreError::Malformed);
    };
    for (name, entry) in inline {
        if let YamlNode::Map(attrs) = entry
            && let Some(YamlNode::Scalar(provider)) = attrs.get("provider")
            && !provider.trim().is_empty()
        {
            profiles.insert(name.as_str().to_owned(), provider.trim().to_owned());
        }
    }
    Ok(())
}

/// Merge `profiles/*.yaml|*.yml` entries into `profiles`, overriding inline.
fn collect_file_profiles(
    profiles_dir: &Path,
    profiles: &mut BTreeMap<String, String>,
) -> Result<(), StoreError> {
    let entries = match std::fs::read_dir(profiles_dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(StoreError::Unreadable),
        Ok(entries) => entries,
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry.map_err(|_| StoreError::Unreadable)?.path();
        let is_yaml = path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml"));
        if is_yaml {
            files.push(path);
        }
    }
    files.sort();
    for file in files {
        let Some(bytes) = read_store_file(&file, YAML_LIMIT)? else {
            continue;
        };
        let text = str::from_utf8(&bytes).map_err(|_| StoreError::Malformed)?;
        let doc = parse_simple_yaml(text)?;
        let stem = file
            .file_stem()
            .and_then(OsStr::to_str)
            .filter(|stem| !stem.trim().is_empty())
            .ok_or(StoreError::Malformed)?;
        if let Some(YamlNode::Scalar(provider)) = doc.get("provider")
            && !provider.trim().is_empty()
        {
            profiles.insert(stem.to_owned(), provider.trim().to_owned());
        }
    }
    Ok(())
}

/// A node in the mapping-only YAML subset.
#[derive(Debug, Clone, PartialEq, Eq)]
enum YamlNode {
    /// A bare or quoted scalar value.
    Scalar(String),
    /// A nested `key: value` mapping.
    Map(BTreeMap<String, YamlNode>),
}

/// One comment-stripped content line with its space indentation.
#[derive(Debug)]
struct YamlLine<'a> {
    indent: usize,
    content: &'a str,
}

/// Parse the mapping-only YAML subset into a top-level mapping.
fn parse_simple_yaml(text: &str) -> Result<BTreeMap<String, YamlNode>, StoreError> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        if let Some(line) = preprocess_line(raw)? {
            lines.push(line);
        }
    }
    let mut pos = 0_usize;
    parse_block(&lines, &mut pos, 0)
}

/// Strip line endings, indentation, and comments; `None` skips the line.
fn preprocess_line(raw: &str) -> Result<Option<YamlLine<'_>>, StoreError> {
    let line = raw.strip_suffix('\r').unwrap_or(raw);
    let mut indent = 0_usize;
    let mut rest: Option<&str> = None;
    for (index, char) in line.char_indices() {
        if char == ' ' {
            indent = indent.saturating_add(1);
        } else if char == '\t' {
            return Err(StoreError::Malformed);
        } else {
            rest = line.get(index..);
            break;
        }
    }
    let Some(content) = rest.map(str::trim_end) else {
        return Ok(None);
    };
    if content.is_empty() {
        return Ok(None);
    }
    let content = strip_comment(content);
    if content.is_empty() || content == "---" || content == "..." {
        return Ok(None);
    }
    Ok(Some(YamlLine { indent, content }))
}

/// Cut an unquoted `#` comment; quote-aware so `#` inside scalars survives.
fn strip_comment(content: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, char) in content.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(open) = quote {
            if open == '"' && char == '\\' {
                escaped = true;
            } else if char == open {
                quote = None;
            }
            continue;
        }
        match char {
            '\'' | '"' => quote = Some(char),
            '#' => return content.get(..index).map_or(content, str::trim_end),
            _ => {}
        }
    }
    content
}

/// Parse consecutive mapping lines at exactly `indent`.
fn parse_block(
    lines: &[YamlLine<'_>],
    pos: &mut usize,
    indent: usize,
) -> Result<BTreeMap<String, YamlNode>, StoreError> {
    let mut map = BTreeMap::new();
    while let Some(line) = lines.get(*pos) {
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(StoreError::Malformed);
        }
        let (key, value) = split_mapping(line.content)?;
        *pos = pos.saturating_add(1);
        let node = if value.is_empty() {
            let nested = lines.get(*pos).map_or(indent, |line| line.indent);
            if nested > indent {
                YamlNode::Map(parse_block(lines, pos, nested)?)
            } else {
                YamlNode::Map(BTreeMap::new())
            }
        } else {
            YamlNode::Scalar(parse_scalar(value)?)
        };
        if map.insert(key, node).is_some() {
            return Err(StoreError::Malformed);
        }
    }
    Ok(map)
}

/// Split `key: value` on the first colon followed by blank or end of line.
fn split_mapping(content: &str) -> Result<(String, &str), StoreError> {
    if content.starts_with('-') && content.get(1..2).is_none_or(|next| next != "-") {
        return Err(StoreError::Malformed);
    }
    let mut quote: Option<char> = None;
    let mut split: Option<usize> = None;
    for (index, char) in content.char_indices() {
        if let Some(open) = quote {
            if char == open {
                quote = None;
            }
            continue;
        }
        match char {
            '\'' | '"' => quote = Some(char),
            ':' => {
                let after = content.get(index.saturating_add(1)..).unwrap_or("");
                if after.is_empty() || after.starts_with([' ', '\t']) {
                    split = Some(index);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(at) = split else {
        return Err(StoreError::Malformed);
    };
    let key = content.get(..at).map_or("", str::trim);
    let value = content.get(at.saturating_add(1)..).map_or("", str::trim);
    if key.is_empty() {
        return Err(StoreError::Malformed);
    }
    Ok((unquote_key(key)?, value))
}

/// Unquote a mapping key; bare keys pass through unchanged.
fn unquote_key(key: &str) -> Result<String, StoreError> {
    if key.starts_with(['\'', '"']) {
        parse_scalar(key)
    } else if key.contains(['\'', '"', '[', ']', '{', '}']) {
        Err(StoreError::Malformed)
    } else {
        Ok(key.to_owned())
    }
}

/// Parse one scalar: reject block/flow/anchor/tag markers, unquote the rest.
fn parse_scalar(value: &str) -> Result<String, StoreError> {
    match value.chars().next() {
        None => Err(StoreError::Malformed),
        Some('"') => parse_double_quoted(value),
        Some('\'') => parse_single_quoted(value),
        Some('[' | '{' | '*' | '&' | '!' | '|' | '>' | '@' | '`') => Err(StoreError::Malformed),
        Some(_) => Ok(value.to_owned()),
    }
}

/// Unquote a double-quoted scalar with a minimal escape subset.
fn parse_double_quoted(value: &str) -> Result<String, StoreError> {
    if value.len() < 2 || !value.ends_with('"') {
        return Err(StoreError::Malformed);
    }
    let inner = value
        .get(1..value.len().saturating_sub(1))
        .ok_or(StoreError::Malformed)?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(char) = chars.next() {
        if char != '\\' {
            out.push(char);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(_) | None => return Err(StoreError::Malformed),
        }
    }
    Ok(out)
}

/// Unquote a single-quoted scalar, where `''` escapes one quote.
fn parse_single_quoted(value: &str) -> Result<String, StoreError> {
    if value.len() < 2 || !value.ends_with('\'') {
        return Err(StoreError::Malformed);
    }
    let inner = value
        .get(1..value.len().saturating_sub(1))
        .ok_or(StoreError::Malformed)?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(char) = chars.next() {
        if char != '\'' {
            out.push(char);
            continue;
        }
        if chars.next() != Some('\'') {
            return Err(StoreError::Malformed);
        }
        out.push('\'');
    }
    Ok(out)
}

#[cfg(test)]
mod tests;

// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::super::anchored_fs::RepositoryDirectory;
use super::Names;
use anyhow::{Context, Result, ensure};
use std::collections::HashSet;
use std::path::Path;

#[derive(Default)]
pub(super) struct Inventory {
    pub(super) xctest: Names,
    pub(super) testing: Names,
}

pub(super) fn inventory(repository: &RepositoryDirectory, relative: &Path) -> Result<Inventory> {
    let mut sources = Vec::new();
    for path in repository.list_files(relative, "swift", 100_000)? {
        let bytes = repository.read_file(&path, 2 * 1024 * 1024)?;
        let source = std::str::from_utf8(&bytes).context("Swift test source is not UTF-8")?;
        sources.push(source.to_owned());
    }
    sources_inventory(&sources)
}

pub(super) fn sources_inventory(sources: &[String]) -> Result<Inventory> {
    let known_xctest_types = sources
        .iter()
        .try_fold(HashSet::new(), |mut known, source| {
            known.extend(declared_xctest_types(&swift_tokens(source)?)?);
            Ok::<_, anyhow::Error>(known)
        })?;
    let mut inventory = Inventory::default();
    for source in sources {
        let found = source_inventory_with_types(source, &known_xctest_types)?;
        for (actual, names) in [
            (&mut inventory.xctest, found.xctest),
            (&mut inventory.testing, found.testing),
        ] {
            for name in names {
                ensure!(
                    actual.insert(name.clone()),
                    "duplicate native source test {name}"
                );
            }
        }
    }
    Ok(inventory)
}

fn swift_tokens(source: &str) -> Result<Vec<String>> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"//") {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if bytes[index..].starts_with(b"/*") {
            index += 2;
            let mut depth = 1;
            while depth > 0 && index < bytes.len() {
                if bytes[index..].starts_with(b"/*") {
                    depth += 1;
                    index += 2;
                } else if bytes[index..].starts_with(b"*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            ensure!(depth == 0, "unterminated Swift comment");
        } else if bytes[index] == b'"'
            || (bytes[index] == b'#' && bytes.get(index + 1) == Some(&b'"'))
        {
            let raw = usize::from(bytes[index] == b'#');
            index += raw;
            let quotes = if bytes[index..].starts_with(b"\"\"\"") {
                3
            } else {
                1
            };
            index += quotes;
            let mut end = vec![b'"'; quotes];
            if raw == 1 {
                end.push(b'#');
            }
            let mut closed = false;
            while index < bytes.len() {
                if bytes[index..].starts_with(&end) {
                    index += end.len();
                    closed = true;
                    break;
                }
                if raw == 0 && bytes[index] == b'\\' {
                    index += 1;
                }
                index += 1;
            }
            ensure!(closed, "unterminated Swift string");
        } else if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(source[start..index].to_owned());
        } else {
            if !bytes[index].is_ascii_whitespace() {
                tokens.push(char::from(bytes[index]).to_string());
            }
            index += 1;
        }
    }
    Ok(tokens)
}

fn declared_xctest_types(tokens: &[String]) -> Result<HashSet<String>> {
    let mut result = HashSet::new();
    for (index, token) in tokens.iter().enumerate() {
        if token != "class" || tokens.get(index + 1).is_some_and(|next| next == "func") {
            continue;
        }
        let name = tokens.get(index + 1).context("missing Swift class name")?;
        let end = tokens[index + 2..]
            .iter()
            .position(|token| token == "{")
            .context("missing Swift class body")?
            + index
            + 2;
        if tokens[index + 2..end]
            .iter()
            .any(|token| token == "XCTestCase")
        {
            result.insert(name.clone());
        }
    }
    Ok(result)
}

fn source_inventory_with_types(
    source: &str,
    known_xctest_types: &HashSet<String>,
) -> Result<Inventory> {
    source_inventory_with_tokens(swift_tokens(source)?, known_xctest_types)
}

fn source_inventory_with_tokens(
    tokens: Vec<String>,
    known_xctest_types: &HashSet<String>,
) -> Result<Inventory> {
    let mut inventory = Inventory::default();
    let mut scopes: Vec<Option<(String, bool)>> = Vec::new();
    let mut pending_type = None;
    let mut pending_test = false;
    for (index, token) in tokens.iter().enumerate() {
        match token.as_str() {
            "class" | "struct" | "enum" | "extension" => {
                if tokens.get(index + 1).is_some_and(|next| next == "func") {
                    continue;
                }
                let name = tokens
                    .get(index + 1)
                    .context("missing Swift type name")?
                    .clone();
                let end = tokens[index + 2..]
                    .iter()
                    .position(|token| token == "{")
                    .context("missing Swift type body")?
                    + index
                    + 2;
                let xctest = known_xctest_types.contains(&name)
                    || tokens[index + 2..end]
                        .iter()
                        .any(|token| token == "XCTestCase");
                pending_type = Some((name, xctest));
            }
            "Test" if index > 0 && tokens[index - 1] == "@" => {
                ensure!(!pending_test, "unmatched Swift @Test declaration");
                pending_test = true;
            }
            "func" => {
                let name = tokens
                    .get(index + 1)
                    .context("missing Swift function name")?;
                let owner = scopes.last().and_then(Option::as_ref);
                let is_xctest =
                    owner.is_some_and(|(_, xctest)| *xctest) && name.starts_with("test");
                if is_xctest || pending_test {
                    ensure!(
                        tokens.get(index + 2).is_some_and(|token| token == "(")
                            && tokens.get(index + 3).is_some_and(|token| token == ")"),
                        "native inventory requires parameterless test declarations: {name}"
                    );
                    let identity =
                        owner.map_or_else(|| name.clone(), |(owner, _)| format!("{owner}.{name}"));
                    let names = if pending_test {
                        &mut inventory.testing
                    } else {
                        &mut inventory.xctest
                    };
                    ensure!(
                        names.insert(identity.clone()),
                        "duplicate Swift test declaration {identity}"
                    );
                    pending_test = false;
                }
            }
            "{" => scopes.push(pending_type.take()),
            "}" => {
                scopes.pop().context("unbalanced Swift source braces")?;
            }
            _ => {}
        }
    }
    ensure!(
        scopes.is_empty() && !pending_test,
        "incomplete Swift test source"
    );
    Ok(inventory)
}

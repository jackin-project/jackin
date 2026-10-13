// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;
use syn::visit::Visit;

use crate::cmd;

#[cfg(test)]
mod tests;

#[derive(Args, Debug)]
pub(crate) struct CiDocExamplesArgs {
    #[arg(long)]
    package: String,
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    manifest_path: PathBuf,
}

pub(crate) fn run(args: CiDocExamplesArgs) -> Result<()> {
    let mut metadata = Command::new("cargo");
    metadata.args([
        "metadata",
        "--format-version",
        "1",
        "--no-deps",
        "--locked",
        "--offline",
    ]);
    let metadata: Metadata = serde_json::from_slice(
        &cmd::output(&mut metadata).context("reading Cargo metadata for doctest ownership")?,
    )
    .context("parsing Cargo metadata for doctest ownership")?;
    let Some(package) = metadata
        .packages
        .iter()
        .find(|package| package.name == args.package)
    else {
        bail!("workspace package not found: {}", args.package);
    };
    let crate_dir = package
        .manifest_path
        .parent()
        .context("package manifest has no parent directory")?;
    let source_dir = crate_dir.join("src");
    let mut rust_files = Vec::new();
    collect_rust_files(&source_dir, &mut rust_files)?;
    rust_files.sort();

    let mut violations = Vec::new();
    for path in rust_files {
        let source = fs::read_to_string(&path)
            .with_context(|| format!("read documentation source {}", path.display()))?;
        let parsed = syn::parse_file(&source)
            .with_context(|| format!("parse documentation source {}", path.display()))?;
        let mut docs = Documentation::default();
        docs.visit_file(&parsed);
        if has_runnable_doc_fence(&docs.markdown) {
            violations.push(path.display().to_string());
        }
    }
    if !violations.is_empty() {
        bail!(
            "runnable documentation examples must be mirrored as nextest-discoverable tests, then marked `text` or `ignore`:\n{}",
            violations.join("\n")
        );
    }
    Ok(())
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in crate::fs_util::read_dir_sorted(directory)
        .with_context(|| format!("read source directory {}", directory.display()))?
    {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("read file type for {}", path.display()))?;
        if file_type.is_dir() {
            collect_rust_files(&path, files)?;
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        {
            files.push(path);
        }
    }
    Ok(())
}

#[derive(Default)]
struct Documentation {
    markdown: String,
}

impl<'ast> Visit<'ast> for Documentation {
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if attribute.path().is_ident("doc")
            && let syn::Meta::NameValue(value) = &attribute.meta
            && let syn::Expr::Lit(expression) = &value.value
            && let syn::Lit::Str(documentation) = &expression.lit
        {
            self.markdown.push_str(&documentation.value());
            self.markdown.push('\n');
        }
        syn::visit::visit_attribute(self, attribute);
    }
}

fn has_runnable_doc_fence(markdown: &str) -> bool {
    let mut inside_fence = false;
    for line in markdown.lines() {
        let Some(info) = line.trim_start().strip_prefix("```") else {
            continue;
        };
        if inside_fence {
            inside_fence = false;
            continue;
        }
        inside_fence = true;
        let tags = info.split(',').map(str::trim).collect::<Vec<_>>();
        let language = tags.first().copied().unwrap_or_default();
        let ignored = tags.contains(&"ignore");
        if !ignored
            && matches!(
                language,
                "" | "rust" | "no_run" | "should_panic" | "compile_fail"
            )
        {
            return true;
        }
    }
    false
}

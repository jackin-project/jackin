//! Resolve the complete per-crate CI routing contract without shell glue.

use std::collections::BTreeMap;
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;
use serde_json::Value;

use crate::cmd;

#[derive(Args, Debug)]
pub(crate) struct CiRouteArgs {
    #[arg(long)]
    metadata: PathBuf,
    #[arg(long, default_value = "")]
    base_ref: String,
    #[arg(long, default_value = "")]
    before_sha: String,
    #[arg(long)]
    event_name: String,
    #[arg(long, default_value = "")]
    force_package: String,
    #[arg(long)]
    common_contract_key: String,
    #[arg(long)]
    construct_image_changed: String,
    #[arg(long)]
    docker_contract_key: String,
    #[arg(long)]
    docker_e2e: String,
    #[arg(long)]
    source_sha: String,
    #[arg(long)]
    repository: String,
}

#[derive(Deserialize)]
struct Lookup {
    hit: bool,
}

pub(crate) fn run(args: CiRouteArgs) -> Result<()> {
    let selection = selection_args(&args)?;
    let mut selected: Vec<String> =
        invoke_json("affected-crates", &args.metadata, &selection, false)?;
    let cache_keys: BTreeMap<String, String> =
        invoke_json("affected-crates", &args.metadata, &selection, true)?;
    if !args.force_package.is_empty() {
        if !cache_keys.contains_key(&args.force_package) {
            bail!("unknown workspace crate: {}", args.force_package);
        }
        selected = vec![args.force_package.clone()];
    }

    let mut packages = Vec::new();
    let mut reused_packages = Vec::new();
    let mut target_results = serde_json::Map::new();
    for package in selected {
        let cache_key = cache_keys
            .get(&package)
            .with_context(|| format!("missing dependency-closure key for {package}"))?;
        let result: Lookup = invoke(&[
            "ci-result",
            "find",
            "--package",
            &package,
            "--cache-key",
            cache_key,
            "--all-features",
            "true",
            "--docker-e2e",
            &args.docker_e2e,
            "--construct-image-changed",
            &args.construct_image_changed,
            "--common-contract-key",
            &args.common_contract_key,
            "--docker-contract-key",
            &args.docker_contract_key,
            "--runner-os",
            "Linux",
            "--runner-arch",
            "X64",
            "--source-sha",
            &args.source_sha,
            "--repository",
            &args.repository,
        ])?;
        if package != args.force_package && result.hit {
            reused_packages.push(package);
            continue;
        }
        let target: Value = invoke(&[
            "ci-target",
            "find",
            "--package",
            &package,
            "--cache-key",
            cache_key,
            "--all-features",
            "true",
            "--runner-os",
            "Linux",
            "--runner-arch",
            "X64",
            "--repository",
            &args.repository,
        ])?;
        target_results.insert(package.clone(), target);
        packages.push(package);
    }

    write_output("packages", &serde_json::to_string(&packages)?)?;
    write_output("cache-keys", &serde_json::to_string(&cache_keys)?)?;
    write_output("reused-packages", &serde_json::to_string(&reused_packages)?)?;
    write_output("target-results", &serde_json::to_string(&target_results)?)?;
    write_summary(&packages, &reused_packages)
}

fn selection_args(args: &CiRouteArgs) -> Result<Vec<String>> {
    if !args.force_package.is_empty() || args.event_name == "workflow_dispatch" {
        return Ok(vec!["--all".to_owned()]);
    }
    if !args.base_ref.is_empty() {
        let destination = format!("{}:refs/remotes/origin/{}", args.base_ref, args.base_ref);
        cmd::run(Command::new("git").args([
            "fetch",
            "--no-tags",
            "--depth=1",
            "origin",
            &destination,
        ]))?;
        return Ok(vec![
            "--base".to_owned(),
            format!("origin/{}", args.base_ref),
        ]);
    }
    if !args.before_sha.is_empty() && !args.before_sha.bytes().all(|byte| byte == b'0') {
        cmd::run(Command::new("git").args([
            "fetch",
            "--no-tags",
            "--depth=1",
            "origin",
            &args.before_sha,
        ]))?;
        return Ok(vec!["--base".to_owned(), args.before_sha.clone()]);
    }
    Ok(vec!["--all".to_owned()])
}

fn invoke_json<T: serde::de::DeserializeOwned>(
    command: &str,
    metadata: &Path,
    selection: &[String],
    cache_keys: bool,
) -> Result<T> {
    let mut arguments = vec![command.to_owned(), "--metadata".to_owned()];
    arguments.push(metadata.to_string_lossy().into_owned());
    arguments.extend_from_slice(selection);
    if cache_keys {
        arguments.push("--cache-keys".to_owned());
    }
    let references = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    invoke(&references)
}

fn invoke<T: serde::de::DeserializeOwned>(arguments: &[&str]) -> Result<T> {
    let executable = env::current_exe().context("finding current jackin-xtask executable")?;
    let output = cmd::output(Command::new(executable).args(arguments))?;
    serde_json::from_slice(&output)
        .with_context(|| format!("parsing `jackin-xtask {}` output", arguments.join(" ")))
}

fn write_output(name: &str, value: &str) -> Result<()> {
    append_env_file("GITHUB_OUTPUT", format_args!("{name}={value}\n"))
}

fn write_summary(packages: &[String], reused: &[String]) -> Result<()> {
    let mut text = format!(
        "### Crate test routing\n\nReused: {}; scheduled: {}.\n",
        reused.len(),
        packages.len()
    );
    for package in reused {
        text.push_str(&format!("- reused `{package}`\n"));
    }
    for package in packages {
        text.push_str(&format!("- scheduled `{package}`\n"));
    }
    append_env_file("GITHUB_STEP_SUMMARY", format_args!("{text}"))
}

fn append_env_file(name: &str, contents: std::fmt::Arguments<'_>) -> Result<()> {
    let path = env::var_os(name).with_context(|| format!("{name} must be set"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", Path::new(&path).display()))?;
    file.write_fmt(contents)
        .with_context(|| format!("writing {}", Path::new(&path).display()))
}

#[cfg(test)]
mod tests;

// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

mod reports;
#[cfg(test)]
mod tests;

use anyhow::{Context, Result, ensure};
use reports::{MAX_EVIDENCE_BYTES, parse_inventory, parse_junit, require_exact};
use sha2::Digest;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};
use toml::Value as TomlValue;

use super::{Artifact, RunDirectory, TargetDirectory};
use crate::cmd;

const MAX_CONFIG_BYTES: u64 = 256 * 1024;

#[derive(Debug)]
pub(super) struct TestEvidence {
    pub(super) selected: Vec<String>,
    pub(super) executed: Vec<String>,
    pub(super) artifacts: Vec<Artifact>,
}

pub(super) fn run(
    root: &Path,
    target: &TargetDirectory,
    run: &RunDirectory,
    binary: &str,
    filter: &str,
    expected: &BTreeSet<String>,
    capsule: Option<&Path>,
) -> Result<TestEvidence> {
    ensure!(
        !expected.is_empty(),
        "consumer gate must require at least one test"
    );
    let config_path = nextest_config(target, run)?;
    target.verify_run_directory(run)?;
    let mut list = nextest_command(root, "list", binary, filter, capsule, &config_path);
    list.args(["--message-format", "json"]);
    let inventory = cmd::output_timeout_bounded(
        &mut list,
        Duration::from_mins(30),
        MAX_EVIDENCE_BYTES,
        1024 * 1024,
    )?;
    target.verify_run_directory(run)?;
    let selected = parse_inventory(&inventory, binary)?;
    require_exact("nextest selection", &selected, expected)?;

    let mut execute = nextest_command(root, "run", binary, filter, capsule, &config_path);
    execute.args([
        "--retries",
        "0",
        "--flaky-result",
        "fail",
        "--no-tests",
        "fail",
    ]);
    target.verify_run_directory(run)?;
    cmd::run_streaming_timeout(&mut execute, Duration::from_hours(1))?;
    target.verify_run_directory(run)?;
    let junit_relative = Path::new("nextest-store/docker-e2e/junit.xml");
    let report = run
        .read_relative_file(target, junit_relative, MAX_EVIDENCE_BYTES as u64)
        .context("nextest produced no fresh JUnit report")?;
    let executed = parse_junit(&report)?;
    require_exact("executed JUnit tests", &executed, expected)?;
    let artifact_path = format!(
        "target/consumer-ci/{}/{}",
        run.id()?.to_string_lossy(),
        junit_relative.display()
    );
    let artifact = Artifact {
        path: artifact_path,
        sha256: hex::encode(sha2::Sha256::digest(&report)),
    };
    Ok(TestEvidence {
        selected: selected.into_iter().collect(),
        executed: executed.into_iter().collect(),
        artifacts: vec![artifact],
    })
}

fn nextest_config(target: &TargetDirectory, run: &RunDirectory) -> Result<PathBuf> {
    let bytes = target
        .repository()
        .read_file(Path::new(".config/nextest.toml"), MAX_CONFIG_BYTES)?;
    let mut config: TomlValue = toml::from_str(std::str::from_utf8(&bytes)?)
        .context("parsing repository nextest configuration")?;
    let root = config
        .as_table_mut()
        .context("nextest config root is not a table")?;
    let store = root
        .entry("store")
        .or_insert_with(|| TomlValue::Table(toml::map::Map::default()))
        .as_table_mut()
        .context("nextest store config is not a table")?;
    store.insert(
        "dir".to_owned(),
        TomlValue::String(
            run.path()
                .join("nextest-store")
                .to_string_lossy()
                .into_owned(),
        ),
    );
    let profiles = root
        .get_mut("profile")
        .and_then(TomlValue::as_table_mut)
        .context("nextest profiles are missing")?;
    let docker = profiles
        .get_mut("docker-e2e")
        .and_then(TomlValue::as_table_mut)
        .context("docker-e2e nextest profile is missing")?;
    docker.insert("retries".to_owned(), TomlValue::Integer(0));
    docker.insert(
        "flaky-result".to_owned(),
        TomlValue::String("fail".to_owned()),
    );
    let junit = docker
        .entry("junit")
        .or_insert_with(|| TomlValue::Table(toml::map::Map::default()))
        .as_table_mut()
        .context("docker-e2e JUnit config is not a table")?;
    junit.insert("path".to_owned(), TomlValue::String("junit.xml".to_owned()));
    let encoded = toml::to_string(&config).context("serializing isolated nextest configuration")?;
    run.write_file(target, "nextest.toml", encoded.as_bytes())?;
    Ok(run.path().join("nextest.toml"))
}

fn nextest_command(
    root: &Path,
    action: &str,
    binary: &str,
    filter: &str,
    capsule: Option<&Path>,
    config: &Path,
) -> std::process::Command {
    let mut command = cmd::command("cargo");
    command.current_dir(root);
    if let Some(capsule) = capsule {
        command.env("JACKIN_CAPSULE_BIN", capsule);
    }
    command.args(["nextest", action, "--config-file"]);
    command.arg(config).args([
        "-p",
        "jackin",
        "--features",
        "e2e",
        "--test",
        binary,
        "--profile",
        "docker-e2e",
        "-E",
        filter,
    ]);
    command
}

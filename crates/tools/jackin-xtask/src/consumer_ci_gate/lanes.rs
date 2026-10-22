// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};

use super::{Artifact, RunDirectory, TargetDirectory, capsule_artifact};

pub(super) fn docker_preflight(root: &Path, orbstack: bool) -> Result<()> {
    let mut docker = crate::cmd::command("docker");
    docker
        .current_dir(root)
        .args(["info", "--format", "{{.OperatingSystem}}"]);
    let identity = String::from_utf8(crate::cmd::output_timeout_bounded(
        &mut docker,
        Duration::from_secs(30),
        16 * 1024,
        16 * 1024,
    )?)?;
    ensure!(
        !identity.trim().is_empty(),
        "Docker daemon identity missing"
    );
    if orbstack {
        ensure!(
            std::env::consts::OS == "macos" && std::env::consts::ARCH == "aarch64",
            "OrbStack gate requires Apple Silicon macOS"
        );
        ensure!(
            identity.trim() == "OrbStack",
            "selected Docker daemon is not OrbStack"
        );
        let mut system = crate::cmd::command("sw_vers");
        system.args(["-productVersion"]);
        let version = String::from_utf8(crate::cmd::output_timeout_bounded(
            &mut system,
            Duration::from_secs(30),
            1024,
            4096,
        )?)?;
        ensure!(
            version.trim().starts_with("26."),
            "OrbStack gate requires macOS 26"
        );
    }
    Ok(())
}

pub(super) fn build_capsule(
    root: &Path,
    target: &TargetDirectory,
    run: &RunDirectory,
) -> Result<(PathBuf, Artifact)> {
    const CAPSULE_BUILD_TIMEOUT: Duration = Duration::from_mins(30);
    let home = run.create_child_directory("capsule-home")?;
    let config = run.create_child_directory("capsule-config")?;
    let mut build = crate::cmd::command("cargo");
    build
        .current_dir(root)
        .env("JACKIN_HOME_DIR", &home)
        .env("JACKIN_CONFIG_DIR", &config)
        .args([
            "run",
            "--locked",
            "--bin",
            "build-jackin-capsule",
            "--",
            "--export",
        ]);
    let output = crate::cmd::output_timeout_bounded(
        &mut build,
        CAPSULE_BUILD_TIMEOUT,
        32 * 1024,
        1024 * 1024,
    )?;
    let export = String::from_utf8(output)?;
    let path = PathBuf::from(parse_capsule_export(&export)?);
    ensure!(
        path.starts_with(&home),
        "Capsule export is outside the fresh lane home"
    );
    let relative = target.artifact_path(&path)?;
    let artifact = capsule_artifact(target.repository(), target, &relative)
        .context("missing or unsafe built production Capsule")?;
    target.verify_link()?;
    Ok((path, artifact))
}

pub(super) fn parse_capsule_export(output: &str) -> Result<String> {
    let value = output
        .trim()
        .strip_prefix("export JACKIN_CAPSULE_BIN='")
        .and_then(|value| value.strip_suffix('\''))
        .context("invalid Capsule export; shell code is never evaluated")?;
    let path = value.replace("'\\''", "'");
    ensure!(
        !path.contains(['\n', '\r']) && Path::new(&path).is_absolute(),
        "invalid exported Capsule path"
    );
    Ok(path)
}

pub(super) fn broker_cases() -> BTreeSet<String> {
    [
        "docker::usage_broker_desktop_and_two_docker_capsules_make_one_provider_call",
        "docker::usage_broker_desktop_and_twenty_docker_capsules_make_one_provider_call",
        "docker::usage_broker_capsule_refresh_is_same_updating_generation_in_desktop",
        "docker::usage_broker_docker_capsule_cannot_access_another_account_or_global_tree",
        "docker::usage_broker_timeout_holds_ownership_until_provider_returns",
        "docker::usage_broker_distinct_accounts_run_concurrently_within_bound",
        "docker::usage_broker_failure_and_rate_deadline_are_identical_for_all_waiters",
        "docker::usage_broker_unavailable_state_makes_zero_provider_calls",
        "usage_broker_two_host_processes_make_one_provider_call",
        "usage_broker_twenty_host_processes_make_one_provider_call",
        "recovery::usage_broker_killed_owner_recovers_once_without_a_herd",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

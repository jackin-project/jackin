// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Executed consumer checks. Transport fixtures and production launches remain
//! distinct obligations; neither certifies live provider authentication.

mod evidence;
mod native;
mod nextest;

use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use evidence::{artifact, capsule_artifact, fresh_destination, write_fresh};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum Lane {
    BrokerFixtures,
    OrbStackBrokerFixtures,
    Native,
    NativeUi,
    ProductionCapsule,
}

#[derive(Debug, Args)]
pub(crate) struct ConsumerCiGateArgs {
    #[arg(long, value_enum)]
    lane: Lane,
    /// Fresh repository-relative Velnor scenario report.
    #[arg(long)]
    report: PathBuf,
}

#[derive(Serialize)]
struct Evidence<'a> {
    schema: u32,
    source: &'static str,
    head: &'a str,
    check_id: &'a str,
    platform: &'a str,
    scenarios: Vec<Scenario>,
}

#[derive(Serialize)]
struct Scenario {
    id: String,
    executed: bool,
    status: &'static str,
}

#[derive(Serialize)]
struct Inventory<'a> {
    schema: u32,
    head: &'a str,
    scope: &'static str,
    executed_scenarios: &'a [String],
    enumerated_tests: &'a [String],
    executed_tests: &'a [String],
    artifacts: Vec<Artifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    native_toolchain: Option<native::Toolchain>,
}

#[derive(Serialize)]
struct Artifact {
    path: String,
    sha256: String,
}

pub(crate) fn run(args: ConsumerCiGateArgs) -> Result<()> {
    let root = crate::docs::repo_root()?.canonicalize()?;
    let head = source_head(&root)?;
    let expected_head =
        std::env::var("VELNOR_CHECK_HEAD").context("VELNOR_CHECK_HEAD is required")?;
    ensure!(
        head == expected_head,
        "checked source HEAD differs from required HEAD"
    );
    let check_id = std::env::var("VELNOR_CHECK_ID").context("VELNOR_CHECK_ID is required")?;
    ensure!(!check_id.is_empty(), "empty check identity");
    let platform =
        std::env::var("VELNOR_CHECK_PLATFORM").context("VELNOR_CHECK_PLATFORM is required")?;
    verify_platform(&platform)?;
    require_clean_source(&root)?;
    let report = fresh_destination(&root, &args.report)?;
    let detail_relative = args.report.with_extension("inventory.json");
    let detail = fresh_destination(&root, &detail_relative)?;
    let mut artifacts = Vec::new();
    let mut tests = Vec::new();
    let mut native_toolchain = None;
    let (scope, mut executed) = match args.lane {
        Lane::Native | Lane::NativeUi => {
            let native = native::run(&root, matches!(args.lane, Lane::NativeUi))?;
            tests = native.tests;
            native_toolchain = Some(native.toolchain);
            for binary in ["JackinDesktop", "jackin-usage-broker"] {
                artifacts.push(artifact(
                    &root,
                    &root
                        .join("native/dist/JackinDesktop.app/Contents/MacOS")
                        .join(binary),
                )?);
            }
            if matches!(args.lane, Lane::NativeUi) {
                for relative in [
                    "native/DerivedData/Build/Products/Debug/JackinDesktop.app/Contents/MacOS/JackinDesktop",
                    "native/DerivedData/Build/Products/Debug/JackinDesktopUITests-Runner.app/Contents/MacOS/JackinDesktopUITests-Runner",
                    "native/DerivedData/Build/Products/Debug/JackinDesktopUITests-Runner.app/Contents/PlugIns/JackinDesktopUITests.xctest/Contents/MacOS/JackinDesktopUITests",
                ] {
                    artifacts.push(artifact(&root, &root.join(relative))?);
                }
            }
            (
                "native rendered/unit execution; no live provider claim",
                native.scenarios,
            )
        }
        Lane::BrokerFixtures | Lane::OrbStackBrokerFixtures => {
            docker_preflight(&root, matches!(args.lane, Lane::OrbStackBrokerFixtures))?;
            let cases = nextest::run(
                &root,
                "usage_broker_e2e",
                "test(/^docker::/) | test(/^usage_broker_(two|twenty)_host_processes_make_one_provider_call$/) | test(/^recovery::/)",
                &broker_cases(),
                None,
            )?;
            (
                "Python transport fixtures and host broker processes; not production Capsule",
                cases,
            )
        }
        Lane::ProductionCapsule => {
            docker_preflight(&root, false)?;
            let (capsule, capsule_artifact) = build_capsule(&root)?;
            artifacts.push(capsule_artifact);
            let expected =
                BTreeSet::from(["multi_account_tabs_isolate_and_preserve_bindings".to_owned()]);
            let cases = nextest::run(
                &root,
                "multi_account_tabs_e2e",
                "test(/^multi_account_tabs_isolate_and_preserve_bindings$/)",
                &expected,
                Some(&capsule),
            )?;
            ensure!(
                artifact(&root, &capsule)?.sha256 == artifacts[0].sha256,
                "Capsule artifact changed during acceptance"
            );
            (
                "production Capsule normal launch/account isolation/reconnect/restore with fake agents; no live auth/inference claim",
                cases,
            )
        }
    };
    ensure!(!executed.is_empty(), "zero required scenarios executed");
    executed.sort();
    if tests.is_empty() {
        tests = executed.clone();
    }
    tests.sort();
    ensure!(
        executed.windows(2).all(|pair| pair[0] != pair[1]),
        "duplicate executed scenario"
    );
    ensure!(
        source_head(&root)? == head,
        "source HEAD changed during acceptance"
    );
    require_clean_source(&root)?;
    if let Some(ref identity) = native_toolchain {
        native::verify_identity(identity)?;
    }
    let inventory = Inventory {
        schema: 1,
        head: &head,
        scope,
        executed_scenarios: &executed,
        enumerated_tests: &tests,
        executed_tests: &tests,
        artifacts,
        native_toolchain,
    };
    write_fresh(&root, &detail, &serde_json::to_vec_pretty(&inventory)?)?;
    let evidence = Evidence {
        schema: 1,
        source: "mise-task-v1",
        head: &head,
        check_id: &check_id,
        platform: &platform,
        scenarios: executed
            .into_iter()
            .map(|id| Scenario {
                id,
                executed: true,
                status: "passed",
            })
            .collect(),
    };
    write_fresh(&root, &report, &serde_json::to_vec_pretty(&evidence)?)
}

fn source_head(root: &Path) -> Result<String> {
    let mut command = crate::cmd::command("git");
    command.current_dir(root).args(["rev-parse", "HEAD"]);
    let head = String::from_utf8(crate::cmd::output_timeout(
        &mut command,
        Duration::from_secs(30),
    )?)?
    .trim()
    .to_owned();
    ensure!(
        head.len() == 40 && head.bytes().all(|c| c.is_ascii_hexdigit()),
        "invalid source HEAD"
    );
    Ok(head)
}

fn require_clean_source(root: &Path) -> Result<()> {
    let mut command = crate::cmd::command("git");
    command
        .current_dir(root)
        .args(["status", "--porcelain", "--untracked-files=normal"]);
    ensure!(
        crate::cmd::output_timeout(&mut command, Duration::from_secs(30))?.is_empty(),
        "acceptance requires a clean committed source checkout"
    );
    Ok(())
}

fn verify_platform(platform: &str) -> Result<()> {
    let actual = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux_x64",
        ("macos", "aarch64") => "macos_arm64",
        ("macos", "x86_64") => "macos_x64",
        _ => anyhow::bail!("unsupported acceptance platform"),
    };
    ensure!(
        platform == actual,
        "required platform differs from actual platform"
    );
    Ok(())
}

fn docker_preflight(root: &Path, orbstack: bool) -> Result<()> {
    let mut docker = crate::cmd::command("docker");
    docker
        .current_dir(root)
        .args(["info", "--format", "{{.OperatingSystem}}"]);
    let identity = String::from_utf8(crate::cmd::output_timeout(
        &mut docker,
        Duration::from_secs(30),
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
        ensure!(
            String::from_utf8(crate::cmd::output_timeout(
                &mut system,
                Duration::from_secs(30)
            )?)?
            .trim()
            .starts_with("26."),
            "OrbStack gate requires macOS 26"
        );
    }
    Ok(())
}

fn build_capsule(root: &Path) -> Result<(PathBuf, Artifact)> {
    const CAPSULE_BUILD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
    let target = root.join("target");
    match fs::symlink_metadata(&target) {
        Ok(metadata) => ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "production target directory must be real"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&target)?,
        Err(error) => return Err(error.into()),
    }
    let artifact_home = tempfile::Builder::new()
        .prefix("consumer-capsule-")
        .tempdir_in(&target)?
        .keep();
    let mut build = crate::cmd::command("cargo");
    build
        .current_dir(root)
        .env("CARGO_TARGET_DIR", &target)
        .env("JACKIN_HOME_DIR", &artifact_home)
        .env("JACKIN_CONFIG_DIR", artifact_home.join("config"))
        .args([
            "run",
            "--locked",
            "--bin",
            "build-jackin-capsule",
            "--",
            "--export",
        ]);
    let output = crate::cmd::output_timeout(&mut build, CAPSULE_BUILD_TIMEOUT)?;
    let export = String::from_utf8(output)?;
    let path = parse_capsule_export(&export)?;
    let path = PathBuf::from(path);
    ensure!(
        path.starts_with(root),
        "Capsule export escapes source checkout"
    );
    let artifact =
        capsule_artifact(root, &path).context("missing or unsafe built production Capsule")?;
    Ok((path, artifact))
}

fn parse_capsule_export(output: &str) -> Result<String> {
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

fn broker_cases() -> BTreeSet<String> {
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

#[cfg(test)]
mod tests;

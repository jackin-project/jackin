// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Executed consumer checks. Transport fixtures and production launches remain
//! distinct obligations; neither certifies live provider authentication.

mod anchored_fs;
mod evidence;
mod lanes;
mod managed_target;
mod native;
mod nextest;

use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use evidence::{RunDirectory, TargetDirectory, artifact, capsule_artifact};
use lanes::{broker_cases, build_capsule, docker_preflight};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    ffi::OsString,
    path::{Component, Path, PathBuf},
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

#[derive(Clone, Debug, Serialize)]
pub(super) struct Artifact {
    path: String,
    sha256: String,
}

pub(super) struct LaneResult {
    pub(super) scope: &'static str,
    pub(super) scenarios: Vec<String>,
    pub(super) enumerated_tests: Vec<String>,
    pub(super) executed_tests: Vec<String>,
    pub(super) artifacts: Vec<Artifact>,
    native_toolchain: Option<native::Toolchain>,
}

pub(crate) fn run(args: ConsumerCiGateArgs) -> Result<()> {
    let root = crate::docs::repo_root()?.canonicalize()?;
    let head = source_head(&root)?;
    let expected_head = required_env("VELNOR_CHECK_HEAD")?;
    ensure!(
        head == expected_head,
        "checked source HEAD differs from required HEAD"
    );
    let check_id = required_env("VELNOR_CHECK_ID")?;
    ensure!(
        check_id.len() <= 256 && !check_id.chars().any(char::is_control),
        "invalid check identity"
    );
    let platform = required_env("VELNOR_CHECK_PLATFORM")?;
    verify_platform(&platform)?;
    require_clean_source(&root)?;

    let target = TargetDirectory::open(&root)?;
    let (report_name, detail_name) = report_names(&args.report)?;
    target.preflight_report(&report_name)?;
    target.preflight_report(&detail_name)?;
    let lane_directory = target.create_run_directory()?;

    let mut lane = match args.lane {
        Lane::Native | Lane::NativeUi => {
            native::run(&root, &target, matches!(args.lane, Lane::NativeUi))?
        }
        Lane::BrokerFixtures | Lane::OrbStackBrokerFixtures => {
            docker_preflight(&root, matches!(args.lane, Lane::OrbStackBrokerFixtures))?;
            let result = nextest::run(
                &root,
                &target,
                &lane_directory,
                "usage_broker_e2e",
                "test(/^docker::/) | test(/^usage_broker_(two|twenty)_host_processes_make_one_provider_call$/) | test(/^recovery::/)",
                &broker_cases(),
                None,
            )?;
            LaneResult {
                scope: "Python transport fixtures and host broker processes; not production Capsule",
                scenarios: result.executed.clone(),
                enumerated_tests: result.selected,
                executed_tests: result.executed,
                artifacts: result.artifacts,
                native_toolchain: None,
            }
        }
        Lane::ProductionCapsule => {
            docker_preflight(&root, false)?;
            let (capsule, capsule_before) = build_capsule(&root, &target, &lane_directory)?;
            let expected =
                BTreeSet::from(["multi_account_tabs_isolate_and_preserve_bindings".to_owned()]);
            let result = nextest::run(
                &root,
                &target,
                &lane_directory,
                "multi_account_tabs_e2e",
                "test(/^multi_account_tabs_isolate_and_preserve_bindings$/)",
                &expected,
                Some(&capsule),
            )?;
            let capsule_relative = target.artifact_path(&capsule)?;
            let capsule_after = capsule_artifact(target.repository(), &target, &capsule_relative)?;
            ensure!(
                capsule_before.sha256 == capsule_after.sha256,
                "production Capsule changed during acceptance"
            );
            let mut artifacts = vec![capsule_before];
            artifacts.extend(result.artifacts);
            LaneResult {
                scope: "production Capsule normal launch/account isolation/reconnect/restore with fake agents; no live auth/inference claim",
                scenarios: result.executed.clone(),
                enumerated_tests: result.selected,
                executed_tests: result.executed,
                artifacts,
                native_toolchain: None,
            }
        }
    };

    ensure!(
        !lane.scenarios.is_empty(),
        "zero required scenarios executed"
    );
    lane.scenarios.sort();
    lane.enumerated_tests.sort();
    lane.executed_tests.sort();
    ensure!(
        lane.scenarios.windows(2).all(|pair| pair[0] != pair[1]),
        "duplicate executed scenario"
    );
    ensure!(
        lane.enumerated_tests == lane.executed_tests,
        "selected and executed test inventories differ"
    );
    ensure!(
        source_head(&root)? == head,
        "source HEAD changed during acceptance"
    );
    require_clean_source(&root)?;
    target.verify_link()?;
    if let Some(ref identity) = lane.native_toolchain {
        native::verify_identity(identity)?;
    }
    verify_artifacts(&target, &lane.artifacts)?;
    let inventory = Inventory {
        schema: 1,
        head: &head,
        scope: lane.scope,
        executed_scenarios: &lane.scenarios,
        enumerated_tests: &lane.enumerated_tests,
        executed_tests: &lane.executed_tests,
        artifacts: lane.artifacts.clone(),
        native_toolchain: lane.native_toolchain.clone(),
    };
    let detail_bytes = serde_json::to_vec_pretty(&inventory)?;
    target.write_report(&detail_name, &detail_bytes)?;
    ensure!(
        source_head(&root)? == head,
        "source HEAD changed before receipt publication"
    );
    require_clean_source(&root)?;
    target.verify_link()?;
    if let Some(ref identity) = lane.native_toolchain {
        native::verify_identity(identity)?;
    }
    verify_artifacts(&target, &lane.artifacts)?;
    let evidence = Evidence {
        schema: 1,
        source: "mise-task-v1",
        head: &head,
        check_id: &check_id,
        platform: &platform,
        scenarios: lane
            .scenarios
            .into_iter()
            .map(|id| Scenario {
                id,
                executed: true,
                status: "passed",
            })
            .collect(),
    };
    target.write_report(&report_name, &serde_json::to_vec_pretty(&evidence)?)
}

fn verify_artifacts(target: &TargetDirectory, expected: &[Artifact]) -> Result<()> {
    for expected_artifact in expected {
        let actual = artifact(
            target.repository(),
            target,
            Path::new(&expected_artifact.path),
        )?;
        ensure!(
            actual.sha256 == expected_artifact.sha256,
            "evidence artifact changed before receipt publication: {}",
            expected_artifact.path
        );
    }
    Ok(())
}

fn required_env(name: &str) -> Result<String> {
    let value = std::env::var(name).with_context(|| format!("{name} is required"))?;
    ensure!(!value.trim().is_empty(), "{name} is empty");
    Ok(value)
}

fn report_names(report: &Path) -> Result<(OsString, OsString)> {
    let components = report.components().collect::<Vec<_>>();
    ensure!(
        matches!(components.as_slice(), [Component::Normal(target), Component::Normal(directory), Component::Normal(_)] if *target == std::ffi::OsStr::new("target") && *directory == std::ffi::OsStr::new("consumer-ci")),
        "report must be a repository-relative target/consumer-ci/<name>.json path"
    );
    let name = report.file_name().context("report has no filename")?;
    let name_text = name.to_str().context("report filename is not UTF-8")?;
    ensure!(
        report.extension() == Some(std::ffi::OsStr::new("json"))
            && name_text.len() > 5
            && name_text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')),
        "report must be a JSON filename"
    );
    let stem = report
        .file_stem()
        .context("report has no stem")?
        .to_str()
        .context("report stem is not UTF-8")?;
    let detail = OsString::from(format!("{stem}.inventory.json"));
    Ok((name.to_os_string(), detail))
}

fn source_head(root: &Path) -> Result<String> {
    let mut command = crate::cmd::command("git");
    command.current_dir(root).args(["rev-parse", "HEAD"]);
    let head = String::from_utf8(crate::cmd::output_timeout_bounded(
        &mut command,
        Duration::from_secs(30),
        256,
        4096,
    )?)?;
    let head = head.trim().to_owned();
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
        .args(["status", "--porcelain", "--untracked-files=all"]);
    ensure!(
        crate::cmd::output_timeout_bounded(
            &mut command,
            Duration::from_secs(30),
            1024 * 1024,
            4096
        )?
        .is_empty(),
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

#[cfg(test)]
mod tests;

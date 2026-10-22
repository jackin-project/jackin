// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

mod reports;
mod required;
mod source;
#[cfg(test)]
mod tests;
mod toolchain;

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

use super::anchored_fs::{BoundDirectory, EntryKind, RepositoryDirectory};
use super::{Artifact, LaneResult, TargetDirectory, artifact};
use crate::cmd;
use reports::{parse_junit, verify_xctest};
use source::inventory;
#[cfg(test)]
use source::{source_inventory, sources_inventory};
use toolchain::{path_with_selected_xcodebuild, preflight, verify_task_xcodebuild_path};

const MAX_REPORT_BYTES: u64 = 16 * 1024 * 1024;
const UNIT_GATE_TIMEOUT: Duration = Duration::from_hours(1);
const UI_GATE_TIMEOUT: Duration = Duration::from_hours(2);

type Names = BTreeSet<String>;

#[cfg(test)]
use toolchain::validate_identity;
pub(super) use toolchain::{Toolchain, verify_identity};

pub(super) struct ReportCustody {
    unit_root: BoundDirectory,
    ui_root: Option<BoundDirectory>,
    ui_directory: Option<BoundDirectory>,
}

pub(super) fn verify_report_custody(
    custody: &ReportCustody,
    repository: &RepositoryDirectory,
) -> Result<()> {
    custody.unit_root.verify(repository)?;
    if let Some(root) = &custody.ui_root {
        root.verify(repository)?;
    }
    if let Some(directory) = &custody.ui_directory {
        directory.verify(repository)?;
    }
    Ok(())
}

pub(super) fn run(root: &Path, target: &TargetDirectory, ui: bool) -> Result<LaneResult> {
    let toolchain = preflight()?;
    let repository = target.repository();
    let unit_report_root = repository.open_or_create_directory(Path::new("native/.build"))?;
    let ui_report_root = if ui {
        Some(repository.open_or_create_directory(Path::new("native/.build/test-results"))?)
    } else {
        None
    };
    let unit = inventory(repository, Path::new("native/Tests/JackinUsageBridgeTests"))?;
    let ui_inventory = inventory(repository, Path::new("native/UITests"))?;
    required::verify_source(&unit, &ui_inventory)?;
    ensure!(
        !unit.xctest.is_empty() && !unit.testing.is_empty(),
        "native unit source inventory must contain XCTest and Swift Testing cases"
    );
    ensure!(
        !ui_inventory.xctest.is_empty() && ui_inventory.testing.is_empty(),
        "UI source inventory must contain XCTest cases only"
    );
    let unit_reports = [
        "native/.build/swift-unit-tests.log",
        "native/.build/swift-unit-tests.xml",
        "native/.build/swift-unit-tests-swift-testing.xml",
    ];
    for relative in unit_reports {
        let name = Path::new(relative)
            .file_name()
            .context("native report path has no filename")?;
        ensure!(
            unit_report_root.entry_kind(repository, name)? == EntryKind::Missing,
            "preexisting native unit report rejected: {relative}"
        );
    }
    let prior_ui = ui_report_root
        .as_ref()
        .map(|report_root| ui_directories(repository, report_root))
        .transpose()?
        .unwrap_or_default();
    run_native_task(root, &toolchain, ui)?;
    unit_report_root.verify(repository)?;
    if let Some(report_root) = &ui_report_root {
        report_root.verify(repository)?;
    }

    let xctest_log = read_report(&unit_report_root, repository, "swift-unit-tests.log")?;
    verify_xctest(&xctest_log, &unit.xctest)?;
    let xctest_xml = read_report(&unit_report_root, repository, "swift-unit-tests.xml")?;
    verify_names(
        &parse_junit(&xctest_xml)?,
        &unit.xctest,
        "native XCTest JUnit",
    )?;
    let swift_testing_xml = read_report(
        &unit_report_root,
        repository,
        "swift-unit-tests-swift-testing.xml",
    )?;
    verify_names(
        &parse_junit(&swift_testing_xml)?,
        &unit.testing,
        "native Swift Testing JUnit",
    )?;

    let mut scenarios = vec!["native-swift-tests".to_owned()];
    let mut tests = unit
        .xctest
        .iter()
        .map(|name| format!("xctest:{name}"))
        .chain(
            unit.testing
                .iter()
                .map(|name| format!("swift-testing:{name}")),
        )
        .collect::<Vec<_>>();
    let mut artifacts = unit_reports
        .iter()
        .zip([&xctest_log, &xctest_xml, &swift_testing_xml])
        .map(|(path, contents)| report_artifact(path, contents.as_bytes()))
        .collect::<Vec<_>>();
    let mut ui_report_directory = None;
    for binary in [
        "native/dist/JackinDesktop.app/Contents/MacOS/JackinDesktop",
        "native/dist/JackinDesktop.app/Contents/MacOS/jackin-usage-broker",
    ] {
        artifacts.push(artifact(repository, target, Path::new(binary))?);
    }
    if ui {
        let ui_report_root = ui_report_root
            .as_ref()
            .context("native UI report root was not bound")?;
        let new_ui = ui_directories(repository, ui_report_root)?
            .difference(&prior_ui)
            .cloned()
            .collect::<Vec<_>>();
        ensure!(
            new_ui.len() == 1,
            "expected exactly one fresh UI report directory, found {}",
            new_ui.len()
        );
        let ui_directory = ui_report_root.open_child_directory(repository, &new_ui[0])?;
        let reports = verify_ui_reports(repository, &ui_directory, &ui_inventory.xctest)?;
        ui_report_directory = Some(ui_directory);
        for (path, bytes) in reports {
            artifacts.push(report_artifact(&path.to_string_lossy(), &bytes));
        }
        for binary in [
            "native/DerivedData/Build/Products/Debug/JackinDesktop.app/Contents/MacOS/JackinDesktop",
            "native/DerivedData/Build/Products/Debug/JackinDesktopUITests-Runner.app/Contents/MacOS/JackinDesktopUITests-Runner",
            "native/DerivedData/Build/Products/Debug/JackinDesktopUITests-Runner.app/Contents/PlugIns/JackinDesktopUITests.xctest/Contents/MacOS/JackinDesktopUITests",
        ] {
            artifacts.push(artifact(repository, target, Path::new(binary))?);
        }
        scenarios.push("native-ui-tests".to_owned());
        tests.extend(ui_inventory.xctest.iter().map(|name| format!("ui:{name}")));
    }
    tests.sort();
    unit_report_root.verify(repository)?;
    if let Some(report_root) = &ui_report_root {
        report_root.verify(repository)?;
    }
    let report_custody = ReportCustody {
        unit_root: unit_report_root,
        ui_root: ui_report_root,
        ui_directory: ui_report_directory,
    };
    verify_report_custody(&report_custody, repository)?;
    ensure!(
        preflight()? == toolchain,
        "native toolchain identity changed during acceptance"
    );
    Ok(LaneResult {
        scope: if ui {
            "native unit and per-case real GUI UI-test execution; no live provider claim"
        } else {
            "native unit execution; no live provider claim"
        },
        scenarios,
        enumerated_tests: tests.clone(),
        executed_tests: tests,
        artifacts,
        native_toolchain: Some(toolchain),
        native_report_custody: Some(report_custody),
    })
}

fn run_native_task(root: &Path, toolchain: &Toolchain, ui: bool) -> Result<()> {
    let task_path = path_with_selected_xcodebuild(
        &toolchain.xcodebuild_path,
        &std::env::var_os("PATH").unwrap_or_default(),
    )?;
    verify_task_xcodebuild_path(root, toolchain, &task_path)?;
    let mut command = cmd::command("mise");
    command
        .current_dir(root)
        .env("PATH", &task_path)
        .env("DEVELOPER_DIR", &toolchain.developer)
        .env("SDKROOT", &toolchain.sdk_path)
        .args(["-C", "native", "run", if ui { "merge" } else { "ci" }]);
    let task_result = cmd::run_streaming_timeout(
        &mut command,
        if ui { UI_GATE_TIMEOUT } else { UNIT_GATE_TIMEOUT },
    )
    .context("current native/mise.toml gate failed; UI mode requires a working GUI session and automation permissions");
    let path_result = verify_task_xcodebuild_path(root, toolchain, &task_path);
    task_result?;
    path_result
}

fn read_report(
    report_root: &BoundDirectory,
    repository: &RepositoryDirectory,
    name: &str,
) -> Result<String> {
    let bytes = report_root
        .read_file(
            repository,
            std::ffi::OsStr::new(name),
            MAX_REPORT_BYTES,
            true,
            "native report",
        )
        .with_context(|| format!("opening bounded native report {name}"))?;
    ensure!(!bytes.is_empty(), "native report is empty: {name}");
    String::from_utf8(bytes).context("native report is not UTF-8")
}

fn report_artifact(path: &str, bytes: &[u8]) -> Artifact {
    Artifact {
        path: path.to_owned(),
        sha256: hex::encode(Sha256::digest(bytes)),
    }
}

fn ui_directories(
    repository: &RepositoryDirectory,
    root: &BoundDirectory,
) -> Result<BTreeSet<OsString>> {
    let mut directories = BTreeSet::new();
    for (name, kind) in root.list_children(repository, 1024)? {
        let Some(name_text) = name.to_str() else {
            continue;
        };
        if name_text.starts_with("ui-") {
            ensure!(
                kind == EntryKind::Directory,
                "native UI report entry is not a real directory"
            );
            directories.insert(name);
        }
    }
    Ok(directories)
}

fn verify_ui_reports(
    repository: &RepositoryDirectory,
    directory: &BoundDirectory,
    expected: &Names,
) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let mut report_files = Vec::new();
    for (name, kind) in directory.list_children(repository, 1024)? {
        ensure!(
            kind == EntryKind::File,
            "UI report directory contains an unsafe entry"
        );
        ensure!(
            name.to_str().is_some_and(|name| {
                name.rsplit_once('.')
                    .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("xml"))
            }),
            "UI report filename is not XML"
        );
        report_files.push(name);
    }
    report_files.sort();
    let expected_files: Names = expected
        .iter()
        .map(|name| format!("{}.xml", name.rsplit('.').next().unwrap_or(name)))
        .collect();
    let actual_files: Names = report_files
        .iter()
        .map(|name| Ok(name.to_string_lossy().into_owned()))
        .collect::<Result<_>>()?;
    verify_names(&actual_files, &expected_files, "UI report filenames")?;
    let mut executed = Names::new();
    let mut reports = Vec::new();
    for report_name in report_files {
        let bytes = directory.read_file(
            repository,
            &report_name,
            MAX_REPORT_BYTES,
            true,
            "UI JUnit report",
        )?;
        let text = String::from_utf8(bytes.clone()).context("UI JUnit report is not UTF-8")?;
        let names = parse_junit(&text)?;
        ensure!(
            names.len() == 1,
            "each UI JUnit report must contain one testcase"
        );
        let name = names.iter().next().context("UI report has no testcase")?;
        ensure!(
            Path::new(&report_name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                == name.rsplit('.').next(),
            "UI testcase does not match report filename"
        );
        ensure!(
            executed.insert(name.clone()),
            "duplicate UI testcase {name}"
        );
        reports.push((directory.relative().join(report_name), bytes));
    }
    verify_names(&executed, expected, "UI executed tests")?;
    directory.verify(repository)?;
    Ok(reports)
}

fn verify_names(actual: &Names, expected: &Names, label: &str) -> Result<()> {
    ensure!(
        actual == expected,
        "{label} inventory mismatch: missing {:?}; unexpected {:?}",
        expected.difference(actual).collect::<Vec<_>>(),
        actual.difference(expected).collect::<Vec<_>>()
    );
    Ok(())
}

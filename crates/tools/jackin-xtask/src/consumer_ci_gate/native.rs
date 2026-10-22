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
    path::{Path, PathBuf},
    time::Duration,
};

use super::anchored_fs::read_bounded_file;
use super::{Artifact, LaneResult, TargetDirectory, artifact};
use crate::cmd;
use reports::{parse_junit, verify_xctest};
use source::inventory;
#[cfg(test)]
use source::{source_inventory, sources_inventory};
use toolchain::preflight;

const MAX_REPORT_BYTES: u64 = 16 * 1024 * 1024;
const UNIT_GATE_TIMEOUT: Duration = Duration::from_hours(1);
const UI_GATE_TIMEOUT: Duration = Duration::from_hours(2);

type Names = BTreeSet<String>;

#[cfg(test)]
use toolchain::validate_identity;
pub(super) use toolchain::{Toolchain, verify_identity};

pub(super) fn run(root: &Path, target: &TargetDirectory, ui: bool) -> Result<LaneResult> {
    let toolchain = preflight()?;
    let repository = target.repository();
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
        ensure!(
            repository.entry_kind(Path::new(relative))? == super::anchored_fs::EntryKind::Missing,
            "preexisting native unit report rejected: {relative}"
        );
    }
    let prior_ui = ui_directories(repository)?;
    run_native_task(root, &toolchain, ui)?;

    let xctest_log = read_report(repository, unit_reports[0])?;
    verify_xctest(&xctest_log, &unit.xctest)?;
    let xctest_xml = read_report(repository, unit_reports[1])?;
    verify_names(
        &parse_junit(&xctest_xml)?,
        &unit.xctest,
        "native XCTest JUnit",
    )?;
    let swift_testing_xml = read_report(repository, unit_reports[2])?;
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
    for binary in [
        "native/dist/JackinDesktop.app/Contents/MacOS/JackinDesktop",
        "native/dist/JackinDesktop.app/Contents/MacOS/jackin-usage-broker",
    ] {
        artifacts.push(artifact(repository, target, Path::new(binary))?);
    }
    if ui {
        let new_ui = ui_directories(repository)?
            .difference(&prior_ui)
            .cloned()
            .collect::<Vec<_>>();
        ensure!(
            new_ui.len() == 1,
            "expected exactly one fresh UI report directory, found {}",
            new_ui.len()
        );
        let reports = verify_ui_reports(repository, &new_ui[0], &ui_inventory.xctest)?;
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
    })
}

fn run_native_task(root: &Path, toolchain: &Toolchain, ui: bool) -> Result<()> {
    let mut command = cmd::command("mise");
    command
        .current_dir(root)
        .env("DEVELOPER_DIR", &toolchain.developer)
        .env("SDKROOT", &toolchain.sdk_path)
        .args(["-C", "native", "run", if ui { "merge" } else { "ci" }]);
    cmd::run_streaming_timeout(
        &mut command,
        if ui { UI_GATE_TIMEOUT } else { UNIT_GATE_TIMEOUT },
    )
    .context("current native/mise.toml gate failed; UI mode requires a working GUI session and automation permissions")
}

fn read_report(
    repository: &super::anchored_fs::RepositoryDirectory,
    relative: &str,
) -> Result<String> {
    let mut file = repository
        .open_regular(Path::new(relative), "native report")
        .with_context(|| format!("opening bounded native report {relative}"))?;
    let bytes = read_bounded_file(&mut file, MAX_REPORT_BYTES, true)?;
    ensure!(!bytes.is_empty(), "native report is empty: {relative}");
    String::from_utf8(bytes).context("native report is not UTF-8")
}

fn report_artifact(path: &str, bytes: &[u8]) -> Artifact {
    Artifact {
        path: path.to_owned(),
        sha256: hex::encode(Sha256::digest(bytes)),
    }
}

fn ui_directories(
    repository: &super::anchored_fs::RepositoryDirectory,
) -> Result<BTreeSet<PathBuf>> {
    let root = Path::new("native/.build/test-results");
    match repository.entry_kind(root)? {
        super::anchored_fs::EntryKind::Missing => return Ok(BTreeSet::new()),
        super::anchored_fs::EntryKind::Directory => {}
        _ => anyhow::bail!("native UI report root must be a real directory"),
    }
    let mut directories = BTreeSet::new();
    for (name, kind) in repository.list_children(root, 1024)? {
        let Some(name_text) = name.to_str() else {
            continue;
        };
        if name_text.starts_with("ui-") {
            ensure!(
                kind == super::anchored_fs::EntryKind::Directory,
                "native UI report entry is not a real directory"
            );
            directories.insert(root.join(name));
        }
    }
    Ok(directories)
}

fn verify_ui_reports(
    repository: &super::anchored_fs::RepositoryDirectory,
    directory: &Path,
    expected: &Names,
) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let mut report_files = Vec::new();
    for (name, kind) in repository.list_children(directory, 1024)? {
        ensure!(
            kind == super::anchored_fs::EntryKind::File,
            "UI report directory contains an unsafe entry"
        );
        ensure!(
            name.to_str().is_some_and(|name| {
                name.rsplit_once('.')
                    .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("xml"))
            }),
            "UI report filename is not XML"
        );
        report_files.push(directory.join(name));
    }
    report_files.sort();
    let expected_files: Names = expected
        .iter()
        .map(|name| format!("{}.xml", name.rsplit('.').next().unwrap_or(name)))
        .collect();
    let actual_files: Names = report_files
        .iter()
        .map(|path| {
            path.file_name()
                .context("UI report path has no filename")
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect::<Result<_>>()?;
    verify_names(&actual_files, &expected_files, "UI report filenames")?;
    let mut executed = Names::new();
    let mut reports = Vec::new();
    for path in report_files {
        let mut file = repository.open_regular(&path, "UI JUnit report")?;
        let bytes = read_bounded_file(&mut file, MAX_REPORT_BYTES, true)?;
        let text = String::from_utf8(bytes.clone()).context("UI JUnit report is not UTF-8")?;
        let names = parse_junit(&text)?;
        ensure!(
            names.len() == 1,
            "each UI JUnit report must contain one testcase"
        );
        let name = names.iter().next().context("UI report has no testcase")?;
        ensure!(
            path.file_stem().and_then(|stem| stem.to_str()) == name.rsplit('.').next(),
            "UI testcase does not match report filename"
        );
        ensure!(
            executed.insert(name.clone()),
            "duplicate UI testcase {name}"
        );
        reports.push((path, bytes));
    }
    verify_names(&executed, expected, "UI executed tests")?;
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

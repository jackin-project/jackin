// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};

use crate::cmd;

mod reports;
mod required;
mod source;
use reports::{parse_junit, verify_xctest};
use source::inventory;
#[cfg(test)]
use source::source_inventory;

const MAX_REPORT_BYTES: u64 = 16 * 1024 * 1024;
const UNIT_GATE_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const UI_GATE_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
type Names = BTreeSet<String>;

pub(super) struct Execution {
    pub(super) scenarios: Vec<String>,
    pub(super) tests: Vec<String>,
    pub(super) toolchain: Toolchain,
}

pub(super) fn run(root: &Path, ui: bool) -> Result<Execution> {
    let toolchain = preflight()?;
    let root = root
        .canonicalize()
        .context("canonicalizing native gate root")?;
    let unit = inventory(&root, "native/Tests/JackinUsageBridgeTests")?;
    let ui_inventory = inventory(&root, "native/UITests")?;
    required::verify_source(&unit, &ui_inventory)?;
    ensure!(
        !unit.xctest.is_empty() && !unit.testing.is_empty(),
        "native unit source inventory must contain XCTest and Swift Testing tests"
    );
    ensure!(
        !ui_inventory.xctest.is_empty() && ui_inventory.testing.is_empty(),
        "UI source inventory must contain XCTest tests only"
    );
    for relative in [
        "native/.build/swift-unit-tests.log",
        "native/.build/swift-unit-tests.xml",
        "native/.build/swift-unit-tests-swift-testing.xml",
    ] {
        let path = safe_path(&root, relative)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                ensure!(
                    metadata.is_file(),
                    "stale report is not a file: {}",
                    path.display()
                );
                fs::remove_file(&path)
                    .with_context(|| format!("removing stale report {}", path.display()))?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    if ui {
        clear_ui_reports(&root)?;
    }
    let mut command = cmd::command("mise");
    let compiler_bin = toolchain
        .swift_path
        .parent()
        .context("Swift has no compiler directory")?;
    let mut paths = vec![compiler_bin.to_owned()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").context("missing PATH")?,
    ));
    command
        .current_dir(&root)
        .env("DEVELOPER_DIR", &toolchain.developer)
        .env("SDKROOT", &toolchain.sdk_path)
        .env_remove("TOOLCHAINS")
        .env("PATH", std::env::join_paths(paths)?)
        .args(["run", if ui { "desktop-merge" } else { "desktop-ci" }]);
    cmd::run_streaming_timeout(
        &mut command,
        if ui {
            UI_GATE_TIMEOUT
        } else {
            UNIT_GATE_TIMEOUT
        },
    )
    .context("native desktop gate failed (UI mode requires a working GUI session and granted automation/accessibility permissions)")?;
    verify_xctest(
        &read_bounded(&root, "native/.build/swift-unit-tests.log")?,
        &unit.xctest,
    )?;
    verify_names(
        &parse_junit(&read_bounded(
            &root,
            "native/.build/swift-unit-tests-swift-testing.xml",
        )?)?,
        &unit.testing,
        "Swift Testing",
    )?;
    let mut scenarios = vec!["native-swift-tests".to_owned()];
    let mut tests: Vec<String> = unit
        .xctest
        .iter()
        .map(|name| format!("xctest:{name}"))
        .chain(
            unit.testing
                .iter()
                .map(|name| format!("swift-testing:{name}")),
        )
        .collect();
    if ui {
        verify_ui_reports(&root, &ui_inventory.xctest)?;
        scenarios.push("native-ui-tests".to_owned());
        tests.extend(ui_inventory.xctest.iter().map(|name| format!("ui:{name}")));
    }
    tests.sort();
    ensure!(
        preflight()? == toolchain,
        "native toolchain changed during acceptance"
    );
    Ok(Execution {
        scenarios,
        tests,
        toolchain,
    })
}

fn capture(program: &str, arguments: &[&str]) -> Result<String> {
    let mut command = cmd::command(program);
    command.args(arguments);
    let bytes = cmd::output_timeout(&mut command, Duration::from_secs(30))?;
    Ok(String::from_utf8(bytes)
        .context("native preflight output is not UTF-8")?
        .trim()
        .to_owned())
}

fn version_components(value: &str) -> Result<(u32, u32)> {
    let mut parts = value.split('.');
    let major = parts.next().context("missing version")?.parse::<u32>()?;
    let minor = parts.next().unwrap_or("0").parse::<u32>()?;
    for patch in parts {
        patch.parse::<u32>().context("invalid version component")?;
    }
    Ok((major, minor))
}

// Apple stable release identity: https://developer.apple.com/news/releases/ (2026-09-14).
// SDK/compiler identities are the qualified contents of that exact selected release.
const SHIPPING_XCODE: &str = "Xcode 27.0\nBuild version 27A266a";
const APPLE_XCODE_REQUIREMENT: &str = r#"=identifier "com.apple.dt.Xcode" and anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.9]"#;
const STABLE_RELEASE_SOURCE: &str =
    "https://developer.apple.com/news/releases/ (Xcode 27, 27A266a, 2026-09-14)";
const SHIPPING_SWIFT: &str = "Apple Swift version 6.4 (swiftlang-6.4.0.34.1 clang-2100.3.34.1)\nTarget: arm64-apple-macosx27.0.0";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(super) struct Toolchain {
    host_version: String,
    host_build: String,
    architecture: String,
    developer: PathBuf,
    swift_path: PathBuf,
    sdk_path: PathBuf,
    xcode: String,
    swift: String,
    swift_driver: String,
    sdk_version: String,
    sdk_build: String,
    signature_requirement: String,
    stable_release_source: String,
}

fn validate_toolchain(toolchain: &Toolchain) -> Result<()> {
    ensure!(
        version_components(&toolchain.host_version)? >= (26, 6),
        "Xcode 27 requires macOS 26.6 or later"
    );
    ensure!(
        toolchain.host_version == "27.0",
        "shipping host must be macOS 27.0; supported Xcode hosts are not automatically qualified shipping hosts"
    );
    ensure!(
        !toolchain.host_build.is_empty(),
        "missing macOS build identity"
    );
    ensure!(
        toolchain.architecture == "arm64",
        "native gate requires an arm64 host"
    );
    ensure!(
        toolchain.developer.ends_with("Contents/Developer"),
        "select full Xcode, not Command Line Tools"
    );
    ensure!(
        toolchain.swift_path.starts_with(&toolchain.developer),
        "xcrun Swift must belong to selected Xcode"
    );
    ensure!(
        toolchain.sdk_path.starts_with(&toolchain.developer),
        "macOS SDK must belong to selected Xcode"
    );
    ensure!(
        toolchain.xcode == SHIPPING_XCODE,
        "shipping Xcode must be stable 27.0 build 27A266a"
    );
    ensure!(
        toolchain.swift == SHIPPING_SWIFT
            && toolchain.swift_driver == "swift-driver version: 1.168.6",
        "shipping Swift compiler/driver/clang/target identity mismatch"
    );
    ensure!(
        toolchain.signature_requirement == APPLE_XCODE_REQUIREMENT
            && toolchain.stable_release_source == STABLE_RELEASE_SOURCE,
        "missing approved Apple stable provenance"
    );
    ensure!(
        toolchain.sdk_version == "27.0" && toolchain.sdk_build == "26A425",
        "shipping macOS SDK must be 27.0 build 26A425"
    );
    Ok(())
}

fn global_selection_command() -> std::process::Command {
    let mut command = cmd::command("/usr/bin/xcode-select");
    command.args(["--print-path"]).env_remove("DEVELOPER_DIR");
    command
}

fn compiler_streams(stdout: Vec<u8>, stderr: Vec<u8>) -> Result<(String, String)> {
    Ok((
        String::from_utf8(stdout)?.trim().to_owned(),
        String::from_utf8(stderr)?.trim().to_owned(),
    ))
}

fn preflight() -> Result<Toolchain> {
    ensure!(
        std::env::consts::OS == "macos" && std::env::consts::ARCH == "aarch64",
        "native gate requires a macOS arm64 process"
    );
    let mut selection = global_selection_command();
    let selected_developer = String::from_utf8(cmd::output_timeout(
        &mut selection,
        Duration::from_secs(30),
    )?)?;
    let developer = PathBuf::from(selected_developer.trim()).canonicalize()?;
    ensure!(
        std::env::var_os("TOOLCHAINS").is_none(),
        "native gate refuses TOOLCHAINS override"
    );
    if let Some(value) = std::env::var_os("DEVELOPER_DIR") {
        ensure!(
            PathBuf::from(value).canonicalize()? == developer,
            "DEVELOPER_DIR differs from selected Xcode"
        );
    }
    let bundle = developer
        .parent()
        .and_then(Path::parent)
        .context("invalid Xcode bundle path")?;
    capture(
        "/usr/bin/codesign",
        &[
            "--verify",
            "--deep",
            "--strict",
            "-R",
            APPLE_XCODE_REQUIREMENT,
            bundle.to_str().context("Xcode path not UTF-8")?,
        ],
    )?;
    let selected_swift =
        PathBuf::from(capture("/usr/bin/xcrun", &["--find", "swift"])?).canonicalize()?;
    let path_swift = PathBuf::from(capture("/usr/bin/which", &["swift"])?).canonicalize()?;
    ensure!(
        selected_swift == path_swift || path_swift == Path::new("/usr/bin/swift"),
        "PATH Swift differs from selected Xcode Swift"
    );
    let mut swift_command = cmd::command("/usr/bin/xcrun");
    swift_command.args(["swift", "--version"]);
    let swift_output = cmd::output_raw_timeout(&mut swift_command, Duration::from_secs(30))?;
    ensure!(swift_output.success, "selected Swift identity probe failed");
    let (swift, swift_driver) = compiler_streams(swift_output.stdout, swift_output.stderr)?;
    ensure!(
        capture("swift", &["--version"])? == swift,
        "PATH Swift compiler version differs from selected Xcode"
    );
    let toolchain = Toolchain {
        host_version: capture("/usr/bin/sw_vers", &["-productVersion"])?,
        host_build: capture("/usr/bin/sw_vers", &["-buildVersion"])?,
        architecture: capture("/usr/bin/uname", &["-m"])?,
        developer,
        swift_path: selected_swift,
        sdk_path: PathBuf::from(capture(
            "/usr/bin/xcrun",
            &["--sdk", "macosx", "--show-sdk-path"],
        )?)
        .canonicalize()?,
        xcode: capture("/usr/bin/xcodebuild", &["-version"])?,
        swift,
        swift_driver,
        sdk_version: capture("/usr/bin/xcrun", &["--sdk", "macosx", "--show-sdk-version"])?,
        signature_requirement: APPLE_XCODE_REQUIREMENT.into(),
        stable_release_source: STABLE_RELEASE_SOURCE.into(),
        sdk_build: capture(
            "/usr/bin/xcrun",
            &["--sdk", "macosx", "--show-sdk-build-version"],
        )?,
    };
    if let Some(value) = std::env::var_os("SDKROOT") {
        ensure!(
            PathBuf::from(value).canonicalize()? == toolchain.sdk_path,
            "SDKROOT differs from selected macOS SDK"
        );
    }
    validate_toolchain(&toolchain)?;
    Ok(toolchain)
}

pub(super) fn verify_identity(expected: &Toolchain) -> Result<()> {
    ensure!(
        &preflight()? == expected,
        "native toolchain changed before receipt publication"
    );
    Ok(())
}

fn safe_path(root: &Path, relative: impl AsRef<Path>) -> Result<PathBuf> {
    let mut path = root.to_owned();
    for component in relative.as_ref().components() {
        let Component::Normal(name) = component else {
            bail!("native gate path must stay below repository root");
        };
        path.push(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "native gate refuses symlink {}",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}

#[expect(
    clippy::disallowed_methods,
    reason = "synchronous CLI gate reads bounded test reports outside render/runtime threads"
)]
fn read_bounded(root: &Path, relative: impl AsRef<Path>) -> Result<String> {
    let path = safe_path(root, relative)?;
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("missing native evidence {}", path.display()))?;
    ensure!(
        metadata.is_file() && metadata.len() > 0 && metadata.len() <= MAX_REPORT_BYTES,
        "native evidence must be a nonempty bounded regular file: {}",
        path.display()
    );
    let mut bytes = Vec::new();
    fs::File::open(&path)?
        .take(MAX_REPORT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_REPORT_BYTES,
        "native evidence grew beyond size bound"
    );
    String::from_utf8(bytes).context("native evidence is not UTF-8")
}

fn children(root: &Path, relative: &Path) -> Result<Vec<PathBuf>> {
    let path = safe_path(root, relative)?;
    let mut children = Vec::new();
    for entry in fs::read_dir(path)? {
        let relative = relative.join(entry?.file_name());
        safe_path(root, &relative)?;
        children.push(relative);
        ensure!(
            children.len() <= 1024,
            "native evidence directory exceeds entry bound"
        );
    }
    children.sort();
    Ok(children)
}

fn check_tree(root: &Path, relative: &Path, depth: usize) -> Result<()> {
    ensure!(depth < 32, "native report tree exceeds depth bound");
    for child in children(root, relative)? {
        let metadata = fs::symlink_metadata(root.join(&child))?;
        if metadata.is_dir() {
            check_tree(root, &child, depth + 1)?;
        } else {
            ensure!(
                metadata.is_file(),
                "native report contains nonregular entry"
            );
        }
    }
    Ok(())
}

fn ui_directories(root: &Path) -> Result<Vec<PathBuf>> {
    let relative = Path::new("native/.build/test-results");
    let path = safe_path(root, relative)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut directories = Vec::new();
    for child in children(root, relative)? {
        if child
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("ui-"))
        {
            ensure!(
                root.join(&child).is_dir(),
                "UI report entry must be a directory"
            );
            directories.push(child);
        }
    }
    Ok(directories)
}

fn clear_ui_reports(root: &Path) -> Result<()> {
    for relative in ui_directories(root)? {
        check_tree(root, &relative, 0)?;
        fs::remove_dir_all(safe_path(root, &relative)?)?;
    }
    Ok(())
}

fn verify_ui_reports(root: &Path, expected: &Names) -> Result<()> {
    let directories = ui_directories(root)?;
    ensure!(
        directories.len() == 1,
        "expected exactly one freshly generated UI JUnit directory, got {}",
        directories.len()
    );
    let directory = &directories[0];
    let files = children(root, directory)?;
    let expected_files: Names = expected
        .iter()
        .map(|name| format!("{}.xml", name.rsplit('.').next().unwrap_or(name)))
        .collect();
    let actual_files: Names = files
        .iter()
        .map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
                .context("UI report filename is not UTF-8")
        })
        .collect::<Result<_>>()?;
    verify_names(&actual_files, &expected_files, "UI report filenames")?;
    let mut actual = Names::new();
    for path in files {
        let names = parse_junit(&read_bounded(root, &path)?)?;
        ensure!(
            names.len() == 1,
            "each UI report must contain exactly one testcase"
        );
        let name = names.iter().next().context("UI report has no testcase")?;
        ensure!(
            path.file_stem().and_then(|stem| stem.to_str()) == name.rsplit('.').next(),
            "UI testcase does not match report filename"
        );
        ensure!(actual.insert(name.clone()), "duplicate UI testcase {name}");
    }
    verify_names(&actual, expected, "UI executed tests")
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

#[cfg(test)]
mod tests;

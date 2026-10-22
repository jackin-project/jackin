// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use super::super::anchored_fs::RepositoryDirectory;
use crate::cmd;

const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PROBE_BYTES: usize = 64 * 1024;
const MAX_TOOL_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::consumer_ci_gate) struct Toolchain {
    pub(super) host_version: String,
    pub(super) host_build: String,
    pub(super) architecture: String,
    pub(super) developer: PathBuf,
    pub(super) xcodebuild_path: PathBuf,
    pub(super) xcodebuild_sha256: String,
    pub(super) swift_path: PathBuf,
    pub(super) swift_sha256: String,
    pub(super) swift_format_path: PathBuf,
    pub(super) swift_format_sha256: String,
    pub(super) sdk_path: PathBuf,
    pub(super) xcode_version: String,
    pub(super) swift_version: String,
    pub(super) swift_format_version: String,
    pub(super) sdk_version: String,
    pub(super) sdk_build: String,
    pub(super) toolchains_override: Option<String>,
}

pub(super) fn preflight() -> Result<Toolchain> {
    ensure!(
        std::env::consts::OS == "macos",
        "native gate requires macOS"
    );
    let developer = PathBuf::from(capture("/usr/bin/xcode-select", &["--print-path"], true)?)
        .canonicalize()
        .context("canonicalizing selected Xcode developer directory")?;
    ensure!(
        developer.ends_with("Contents/Developer"),
        "selected developer directory is not full Xcode"
    );
    if let Some(value) = std::env::var_os("DEVELOPER_DIR") {
        ensure!(
            PathBuf::from(value).canonicalize()? == developer,
            "DEVELOPER_DIR differs from globally selected Xcode"
        );
    }
    let developer_text = developer.to_str().context("Xcode path is not UTF-8")?;
    let swift_path = canonical_tool_path(
        &capture_with_developer("/usr/bin/xcrun", &["--find", "swift"], developer_text)?,
        &developer,
        "selected Swift compiler",
    )?;
    let path_swift = PathBuf::from(capture("/usr/bin/which", &["swift"], false)?)
        .canonicalize()
        .context("resolving PATH Swift compiler")?;
    ensure!(
        path_swift == swift_path || path_swift == Path::new("/usr/bin/swift"),
        "PATH Swift differs from selected Xcode Swift"
    );
    let swift_format_path = canonical_tool_path(
        &capture_with_developer(
            "/usr/bin/xcrun",
            &["--find", "swift-format"],
            developer_text,
        )?,
        &developer,
        "selected swift-format",
    )?;
    let sdk_path = PathBuf::from(capture_with_developer(
        "/usr/bin/xcrun",
        &["--sdk", "macosx", "--show-sdk-path"],
        developer_text,
    )?)
    .canonicalize()
    .context("canonicalizing selected macOS SDK")?;
    ensure!(
        sdk_path.starts_with(&developer),
        "selected SDK is outside Xcode"
    );
    if let Some(value) = std::env::var_os("SDKROOT") {
        ensure!(
            PathBuf::from(value).canonicalize()? == sdk_path,
            "SDKROOT differs from selected Xcode SDK"
        );
    }
    let xcodebuild_path = canonical_tool_path(
        &capture_with_developer("/usr/bin/xcrun", &["--find", "xcodebuild"], developer_text)?,
        &developer,
        "selected xcodebuild",
    )?;
    let xcodebuild_program = xcodebuild_path
        .to_str()
        .context("selected xcodebuild path is not UTF-8")?;
    let xcode_version = capture_with_developer(xcodebuild_program, &["-version"], developer_text)?;
    ensure!(
        xcode_version.starts_with("Xcode ") && xcode_version.contains("Build version "),
        "xcodebuild version identity is incomplete"
    );
    let swift_version =
        capture_with_developer("/usr/bin/xcrun", &["swift", "--version"], developer_text)?;
    ensure!(!swift_version.is_empty(), "Swift version identity is empty");
    let swift_format_version = capture_with_developer(
        "/usr/bin/xcrun",
        &["swift-format", "--version"],
        developer_text,
    )?;
    ensure!(
        !swift_format_version.is_empty(),
        "swift-format version identity is empty"
    );
    let tool_root = RepositoryDirectory::open(&developer)?;
    let toolchain = Toolchain {
        host_version: capture("/usr/bin/sw_vers", &["-productVersion"], false)?,
        host_build: capture("/usr/bin/sw_vers", &["-buildVersion"], false)?,
        architecture: capture("/usr/bin/uname", &["-m"], false)?,
        xcodebuild_sha256: digest_tool(&tool_root, &xcodebuild_path)?,
        swift_sha256: digest_tool(&tool_root, &swift_path)?,
        swift_format_sha256: digest_tool(&tool_root, &swift_format_path)?,
        developer: developer.clone(),
        xcodebuild_path,
        swift_path,
        swift_format_path,
        sdk_path,
        xcode_version,
        swift_version,
        swift_format_version,
        sdk_version: capture_with_developer(
            "/usr/bin/xcrun",
            &["--sdk", "macosx", "--show-sdk-version"],
            developer_text,
        )?,
        sdk_build: capture_with_developer(
            "/usr/bin/xcrun",
            &["--sdk", "macosx", "--show-sdk-build-version"],
            developer_text,
        )?,
        toolchains_override: std::env::var("TOOLCHAINS").ok(),
    };
    validate_identity(&toolchain)?;
    Ok(toolchain)
}

pub(in crate::consumer_ci_gate) fn verify_identity(expected: &Toolchain) -> Result<()> {
    ensure!(
        &preflight()? == expected,
        "native Apple toolchain identity changed during acceptance"
    );
    Ok(())
}

pub(in crate::consumer_ci_gate) fn validate_identity(identity: &Toolchain) -> Result<()> {
    ensure!(
        !identity.host_version.is_empty(),
        "macOS version identity is empty"
    );
    ensure!(
        !identity.host_build.is_empty(),
        "macOS build identity is empty"
    );
    ensure!(
        matches!(identity.architecture.as_str(), "arm64" | "x86_64"),
        "unsupported macOS architecture"
    );
    ensure!(
        identity.developer.ends_with("Contents/Developer")
            && identity.swift_path.starts_with(&identity.developer)
            && identity.swift_format_path.starts_with(&identity.developer)
            && identity.sdk_path.starts_with(&identity.developer)
            && identity.xcodebuild_path.starts_with(&identity.developer),
        "Apple tool paths do not belong to selected Xcode"
    );
    ensure!(
        !identity.sdk_version.is_empty() && !identity.sdk_build.is_empty(),
        "macOS SDK version/build identity is incomplete"
    );
    for (label, digest) in [
        ("xcodebuild", &identity.xcodebuild_sha256),
        ("swift", &identity.swift_sha256),
        ("swift-format", &identity.swift_format_sha256),
    ] {
        ensure!(
            digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "{label} executable hash is invalid"
        );
    }
    Ok(())
}

fn canonical_tool_path(path: &str, developer: &Path, label: &str) -> Result<PathBuf> {
    let canonical = PathBuf::from(path)
        .canonicalize()
        .with_context(|| format!("canonicalizing {label} path"))?;
    ensure!(
        canonical.starts_with(developer),
        "{label} is outside selected Xcode"
    );
    Ok(canonical)
}

fn capture(program: &str, arguments: &[&str], remove_developer: bool) -> Result<String> {
    let mut command = cmd::command(program);
    command.args(arguments);
    if remove_developer {
        command.env_remove("DEVELOPER_DIR");
    }
    captured_output(&mut command)
}

fn capture_with_developer(program: &str, arguments: &[&str], developer: &str) -> Result<String> {
    let mut command = cmd::command(program);
    command.args(arguments).env("DEVELOPER_DIR", developer);
    captured_output(&mut command)
}

fn captured_output(command: &mut std::process::Command) -> Result<String> {
    let bytes =
        cmd::output_timeout_bounded(command, PROBE_TIMEOUT, MAX_PROBE_BYTES, MAX_PROBE_BYTES)?;
    let output = String::from_utf8(bytes).context("Apple tool identity output is not UTF-8")?;
    let output = output.trim().to_owned();
    ensure!(!output.is_empty(), "Apple tool identity output is empty");
    Ok(output)
}

fn digest_tool(root: &RepositoryDirectory, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root.root())
        .context("tool path escapes selected Xcode")?;
    let mut file = root.open_regular(relative, "selected Apple tool")?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() <= MAX_TOOL_BYTES,
        "selected tool is not a bounded regular file"
    );
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .context("selected tool size overflow")?;
        ensure!(
            total <= MAX_TOOL_BYTES,
            "selected tool grew beyond size bound"
        );
        hash.update(&buffer[..count]);
    }
    let after = file.metadata()?;
    use std::os::unix::fs::MetadataExt;
    ensure!(
        before.dev() == after.dev()
            && before.ino() == after.ino()
            && before.len() == after.len()
            && before.modified()? == after.modified()?
            && total == after.len(),
        "selected tool changed while hashing"
    );
    Ok(hex::encode(hash.finalize()))
}

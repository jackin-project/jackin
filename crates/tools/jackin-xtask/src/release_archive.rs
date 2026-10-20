// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};

use crate::cmd;

pub(crate) mod layout;
use layout::{ArchiveLayout, ArchiveProfile};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ArchivePackage {
    Jackin,
    JackinCapsule,
}

#[derive(Args, Debug)]
pub(crate) struct ReleaseArchivesArgs {
    #[arg(long, value_enum)]
    package: ArchivePackage,
    #[arg(long)]
    version: String,
    #[arg(long, default_value = "dist")]
    output_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TargetSpec {
    rust: &'static str,
    zigbuild: &'static str,
}

struct ZigCache {
    global: PathBuf,
    local: PathBuf,
}

const JACKIN_TARGETS: [TargetSpec; 4] = [
    TargetSpec {
        rust: "aarch64-apple-darwin",
        zigbuild: "aarch64-apple-darwin",
    },
    TargetSpec {
        rust: "x86_64-apple-darwin",
        zigbuild: "x86_64-apple-darwin",
    },
    TargetSpec {
        rust: "aarch64-unknown-linux-gnu",
        zigbuild: "aarch64-unknown-linux-gnu.2.17",
    },
    TargetSpec {
        rust: "x86_64-unknown-linux-gnu",
        zigbuild: "x86_64-unknown-linux-gnu.2.17",
    },
];

const CAPSULE_TARGETS: [TargetSpec; 2] = [
    TargetSpec {
        rust: "aarch64-unknown-linux-gnu",
        zigbuild: "aarch64-unknown-linux-gnu.2.17",
    },
    TargetSpec {
        rust: "x86_64-unknown-linux-gnu",
        zigbuild: "x86_64-unknown-linux-gnu.2.17",
    },
];

pub(crate) fn build_preview_payload(
    product: &str,
    target: &str,
    version: &str,
    output: &Path,
) -> Result<()> {
    let (package, target) = preview_selection(product, target)?;
    run_targets(
        ReleaseArchivesArgs {
            package,
            version: version.to_owned(),
            output_dir: output.to_owned(),
        },
        std::slice::from_ref(&target),
        ArchiveProfile::Preview,
    )
}

fn preview_selection(product: &str, target: &str) -> Result<(ArchivePackage, TargetSpec)> {
    let package = [ArchivePackage::Jackin, ArchivePackage::JackinCapsule]
        .into_iter()
        .find(|package| package_name(*package) == product)
        .context("unknown preview product")?;
    let target = targets(package)
        .iter()
        .copied()
        .find(|candidate| candidate.rust == target)
        .context("preview target is not declared for this product")?;
    Ok((package, target))
}

pub(crate) fn run(args: ReleaseArchivesArgs) -> Result<()> {
    let selected = targets(args.package);
    run_targets(args, selected, ArchiveProfile::Release)
}

fn run_targets(
    args: ReleaseArchivesArgs,
    targets: &[TargetSpec],
    profile: ArchiveProfile,
) -> Result<()> {
    fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("creating {}", args.output_dir.display()))?;
    let target_dir =
        env::var_os("CARGO_TARGET_DIR").map_or_else(|| PathBuf::from("target"), PathBuf::from);
    let build_root = target_dir.join("release-archives");
    let zig_cache = zig_cache()?;
    let macos_sdk = targets
        .iter()
        .any(|target| target.rust.contains("apple"))
        .then(prepare_macos_sdk)
        .transpose()?;

    let jobs_per_target = jobs_per_target(1);
    let layouts = targets
        .iter()
        .map(|target| {
            ArchiveLayout::new(&build_root.join(target.rust), target.rust, profile)
                .map(|layout| (*target, layout))
        })
        .collect::<Result<Vec<_>>>()?;
    for (target, layout) in &layouts {
        build(
            args.package,
            *target,
            &args.version,
            jobs_per_target,
            layout,
            &zig_cache,
            macos_sdk.as_deref(),
        )?;
    }

    for (_, layout) in &layouts {
        let archive = package(args.package, layout, &args.output_dir)?;
        crate::release_sbom::generate(
            &archive,
            &sidecar(&archive, "sbom.json"),
            &crate::release_sbom::BuildEvidence {
                version: &args.version,
                layout,
            },
        )?;
    }
    Ok(())
}

fn jobs_per_target(target_count: usize) -> usize {
    jobs_per_target_for(
        env::var("CARGO_BUILD_JOBS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or_else(|| thread::available_parallelism().map_or(1, usize::from)),
        target_count,
    )
}

fn jobs_per_target_for(available: usize, target_count: usize) -> usize {
    available.checked_div(target_count).unwrap_or(1).max(1)
}

fn targets(package: ArchivePackage) -> &'static [TargetSpec] {
    match package {
        ArchivePackage::Jackin => &JACKIN_TARGETS,
        ArchivePackage::JackinCapsule => &CAPSULE_TARGETS,
    }
}

fn binaries(package: ArchivePackage) -> &'static [&'static str] {
    match package {
        ArchivePackage::Jackin => &["jackin", "jackin-role", "jackin-usage-broker"],
        ArchivePackage::JackinCapsule => &["jackin-capsule"],
    }
}

fn package_name(package: ArchivePackage) -> &'static str {
    match package {
        ArchivePackage::Jackin => "jackin",
        ArchivePackage::JackinCapsule => "jackin-capsule",
    }
}

fn zig_cache() -> Result<ZigCache> {
    let runner_temp = env::var_os("RUNNER_TEMP").unwrap_or_else(|| "/tmp".into());
    let root = PathBuf::from(runner_temp);
    let cache = ZigCache {
        global: env::var_os("ZIG_GLOBAL_CACHE_DIR")
            .map_or_else(|| root.join("zig-global-cache"), PathBuf::from),
        local: env::var_os("ZIG_LOCAL_CACHE_DIR")
            .map_or_else(|| root.join("zig-local-cache"), PathBuf::from),
    };
    for path in [&cache.global, &cache.local] {
        fs::create_dir_all(path).with_context(|| format!("creating {}", path.display()))?;
    }
    Ok(cache)
}

fn prepare_macos_sdk() -> Result<PathBuf> {
    if !cfg!(target_os = "macos") {
        bail!("Apple release archives require a qualified macOS Xcode host");
    }
    let version = cmd::output(cmd::command("/usr/bin/xcrun").args([
        "--sdk",
        "macosx",
        "--show-sdk-version",
    ]))?;
    if String::from_utf8(version)?.trim() != "27.0" {
        bail!("Apple release archives require the qualified macOS 27.0 SDK");
    }
    let output =
        cmd::output(cmd::command("/usr/bin/xcrun").args(["--sdk", "macosx", "--show-sdk-path"]))?;
    let sdk = PathBuf::from(String::from_utf8(output)?.trim());
    if !sdk.is_absolute() || !sdk.is_dir() {
        bail!("Xcode did not return an installed absolute macOS SDK path");
    }
    Ok(sdk)
}

fn build(
    package: ArchivePackage,
    target: TargetSpec,
    version: &str,
    jobs: usize,
    layout: &ArchiveLayout,
    zig_cache: &ZigCache,
    macos_sdk: Option<&Path>,
) -> Result<()> {
    let mut command = cmd::command("cargo");
    command.args(build_arguments(package, target, layout.profile()));
    // Neither stable nor preview may inherit the retired whole-graph CGU override.
    command.env_remove("CARGO_PROFILE_RELEASE_CODEGEN_UNITS");
    command.env_remove("CARGO_PROFILE_PREVIEW_CODEGEN_UNITS");
    command.env("CARGO_BUILD_JOBS", jobs.to_string());
    command.env("CARGO_TARGET_DIR", layout.root());
    command.env("JACKIN_VERSION_OVERRIDE", version);
    command.env("ZIG_GLOBAL_CACHE_DIR", &zig_cache.global);
    command.env("ZIG_LOCAL_CACHE_DIR", &zig_cache.local);
    if target.rust.contains("apple") {
        let sdk = macos_sdk.context("Apple target requires the prepared macOS SDK")?;
        command.env("SDKROOT", sdk);
        command.env("MACOSX_DEPLOYMENT_TARGET", "12.0");
    }
    fs::create_dir_all(layout.root())?;
    cmd::run_archive_stdout_file(&mut command, &layout.artifacts())
        .with_context(|| format!("building {} for {}", package_name(package), target.rust))
}

fn build_arguments(
    package: ArchivePackage,
    target: TargetSpec,
    profile: ArchiveProfile,
) -> Vec<&'static str> {
    let mut args = vec![
        if target.rust.contains("apple") {
            "build"
        } else {
            "zigbuild"
        },
        "--timings",
        "--locked",
        "-p",
        package_name(package),
    ];
    args.splice(2..2, profile.arguments().iter().copied());
    for binary in binaries(package) {
        args.extend(["--bin", *binary]);
    }
    args.extend([
        "--target",
        target.zigbuild,
        "--message-format=json-render-diagnostics",
    ]);
    args
}

#[expect(
    clippy::disallowed_methods,
    reason = "release packaging is synchronous host-side xtask work, outside runtime/render threads"
)]
pub(crate) fn package(
    package: ArchivePackage,
    layout: &ArchiveLayout,
    output_dir: &Path,
) -> Result<PathBuf> {
    let archive = output_dir.join(format!(
        "{}-{}.tar.gz",
        package_name(package),
        layout.target()
    ));
    let release_dir = layout.target_output();
    for binary in binaries(package) {
        layout.validate_path(&release_dir.join(binary))?;
        if !release_dir.join(binary).is_file() {
            bail!(
                "release binary is missing: {}",
                release_dir.join(binary).display()
            );
        }
    }

    let file = fs::File::create(&archive)?;
    let gzip = flate2::GzBuilder::new()
        .mtime(0)
        .write(file, flate2::Compression::default());
    let mut tar = tar::Builder::new(gzip);
    for binary in binaries(package) {
        let mut file = fs::File::open(release_dir.join(binary))?;
        let mut header = tar::Header::new_gnu();
        header.set_size(file.metadata()?.len());
        header.set_mode(0o755);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        tar.append_data(&mut header, binary, &mut file)?;
    }
    tar.into_inner()?.finish()?.sync_all()?;
    Ok(archive)
}

pub(crate) struct PayloadIdentity {
    pub(crate) package: &'static str,
    pub(crate) target: &'static str,
    pub(crate) binaries: &'static [&'static str],
}

pub(crate) fn preview_payloads() -> Vec<PayloadIdentity> {
    [ArchivePackage::Jackin, ArchivePackage::JackinCapsule]
        .into_iter()
        .flat_map(|package| {
            targets(package).iter().map(move |target| PayloadIdentity {
                package: package_name(package),
                target: target.rust,
                binaries: binaries(package),
            })
        })
        .collect()
}

pub(crate) fn payload_identity(archive: &Path) -> Result<PayloadIdentity> {
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .context("archive filename must be UTF-8")?;
    for package in [ArchivePackage::Jackin, ArchivePackage::JackinCapsule] {
        for target in targets(package) {
            if name == format!("{}-{}.tar.gz", package_name(package), target.rust) {
                return Ok(PayloadIdentity {
                    package: package_name(package),
                    target: target.rust,
                    binaries: binaries(package),
                });
            }
        }
    }
    bail!("archive is not one of the six declared release payloads: {name}")
}

fn sidecar(archive: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}.{}", archive.display(), suffix))
}

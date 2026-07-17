//! Complete multi-target release archive production for one crate.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};
use sha2::{Digest, Sha256};

use crate::cmd;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ArchivePackage {
    Jackin,
    JackinCapsule,
}

#[derive(Args, Debug)]
pub(crate) struct ReleaseArchivesArgs {
    /// Crate whose complete release target set will be produced.
    #[arg(long, value_enum)]
    package: ArchivePackage,
    /// Version embedded in the release binaries.
    #[arg(long)]
    version: String,
    /// Optional version segment included in each archive filename.
    #[arg(long)]
    archive_version: Option<String>,
    /// Directory receiving archives and their sidecars.
    #[arg(long, default_value = "dist")]
    output_dir: PathBuf,
    /// Optional file receiving one complete sccache report for the crate job.
    #[arg(long)]
    sccache_stats: Option<PathBuf>,
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

pub(crate) fn run(args: ReleaseArchivesArgs) -> Result<()> {
    fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("creating {}", args.output_dir.display()))?;
    let target_dir =
        env::var_os("CARGO_TARGET_DIR").map_or_else(|| PathBuf::from("target"), PathBuf::from);
    let zig_cache = zig_cache()?;
    let macos_sdk = (args.package == ArchivePackage::Jackin)
        .then(prepare_macos_sdk)
        .transpose()?;

    for target in targets(args.package) {
        prepare_target(target.rust)?;
        build(
            args.package,
            *target,
            &args.version,
            &zig_cache,
            macos_sdk.as_deref(),
        )?;
        let archive = package(
            args.package,
            target.rust,
            args.archive_version.as_deref(),
            &target_dir,
            &args.output_dir,
        )?;
        write_checksum(&archive)?;
        sign(&archive)?;
        write_sbom(&archive)?;
    }
    preserve_cargo_timings(&target_dir, &args.output_dir)?;
    if let Some(path) = args.sccache_stats.as_deref() {
        write_sccache_stats(path)?;
    }
    Ok(())
}

fn targets(package: ArchivePackage) -> &'static [TargetSpec] {
    match package {
        ArchivePackage::Jackin => &JACKIN_TARGETS,
        ArchivePackage::JackinCapsule => &CAPSULE_TARGETS,
    }
}

fn binaries(package: ArchivePackage) -> &'static [&'static str] {
    match package {
        ArchivePackage::Jackin => &["jackin", "jackin-role"],
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

fn prepare_target(target: &str) -> Result<()> {
    cmd::run_streaming(cmd::command("rustup").args(["target", "add", target]))
        .with_context(|| format!("preparing Rust target {target}"))
}

fn prepare_macos_sdk() -> Result<PathBuf> {
    let runner_temp = env::var_os("RUNNER_TEMP").context("RUNNER_TEMP is required in CI")?;
    let root = PathBuf::from(runner_temp).join("macos-sdk");
    let sdk = root.join("MacOSX26.1.sdk");
    if sdk.is_dir() {
        return Ok(sdk);
    }
    fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
    let archive = root.join("MacOSX26.1.sdk.tar.xz");
    cmd::run_streaming(cmd::command("curl").args([
        OsStr::new("-fsSL"),
        OsStr::new("-o"),
        archive.as_os_str(),
        OsStr::new(
            "https://github.com/joseluisq/macosx-sdks/releases/download/26.1/MacOSX26.1.sdk.tar.xz",
        ),
    ]))
    .context("downloading macOS 26.1 SDK")?;
    cmd::run_streaming(cmd::command("tar").args([
        OsStr::new("-xJf"),
        archive.as_os_str(),
        OsStr::new("-C"),
        root.as_os_str(),
    ]))
    .context("extracting macOS 26.1 SDK")?;
    fs::remove_file(&archive).with_context(|| format!("removing {}", archive.display()))?;
    if !sdk.is_dir() {
        bail!("macOS SDK extraction did not create {}", sdk.display());
    }
    Ok(sdk)
}

fn build(
    package: ArchivePackage,
    target: TargetSpec,
    version: &str,
    zig_cache: &ZigCache,
    macos_sdk: Option<&Path>,
) -> Result<()> {
    let mut command = cmd::command("cargo");
    command.args(["zigbuild", "--timings", "--release", "--locked"]);
    if package == ArchivePackage::JackinCapsule {
        command.args(["-p", "jackin-capsule"]);
    }
    command.args(["--target", target.zigbuild]);
    command.env("JACKIN_VERSION_OVERRIDE", version);
    if package == ArchivePackage::Jackin {
        // The host binary's dependency graph can otherwise feed Zig's Apple
        // linker more than 1,500 objects and exceed a container's descriptor
        // quota. A single release codegen unit also produces the intended
        // fully optimized distribution binary.
        command.env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1");
    }
    command.env("ZIG_GLOBAL_CACHE_DIR", &zig_cache.global);
    command.env("ZIG_LOCAL_CACHE_DIR", &zig_cache.local);
    if target.rust.contains("apple") {
        let sdk = macos_sdk.context("Apple target requires the prepared macOS SDK")?;
        command.env("SDKROOT", sdk);
        command.env("MACOSX_DEPLOYMENT_TARGET", "12.0");
    }
    cmd::run_streaming(&mut command)
        .with_context(|| format!("building {} for {}", package_name(package), target.rust))
}

fn package(
    package: ArchivePackage,
    target: &str,
    archive_version: Option<&str>,
    target_dir: &Path,
    output_dir: &Path,
) -> Result<PathBuf> {
    let basename = archive_version.map_or_else(
        || format!("{}-{target}", package_name(package)),
        |version| format!("{}-{version}-{target}", package_name(package)),
    );
    let archive = output_dir.join(format!("{basename}.tar.gz"));
    let release_dir = target_dir.join(target).join("release");
    for binary in binaries(package) {
        if !release_dir.join(binary).is_file() {
            bail!(
                "release binary is missing: {}",
                release_dir.join(binary).display()
            );
        }
    }

    let mut tar = cmd::command("tar");
    tar.args([
        OsStr::new("--sort=name"),
        OsStr::new("--mtime=@0"),
        OsStr::new("--owner=0"),
        OsStr::new("--group=0"),
        OsStr::new("--numeric-owner"),
        OsStr::new("-cf"),
        OsStr::new("-"),
        OsStr::new("-C"),
        release_dir.as_os_str(),
    ]);
    tar.args(binaries(package));
    let mut gzip = cmd::command("gzip");
    gzip.arg("-n");
    cmd::run_pipeline_to_file(&mut tar, &mut gzip, &archive)?;
    Ok(archive)
}

fn write_checksum(archive: &Path) -> Result<()> {
    let bytes = fs::read(archive).with_context(|| format!("reading {}", archive.display()))?;
    let checksum = format!("{}\n", hex::encode(Sha256::digest(bytes)));
    fs::write(sidecar(archive, "sha256"), checksum)
        .with_context(|| format!("writing checksum for {}", archive.display()))
}

fn sign(archive: &Path) -> Result<()> {
    let bundle = sidecar(archive, "bundle");
    cmd::run_streaming(cmd::command("cosign").args([
        OsStr::new("sign-blob"),
        OsStr::new("--bundle"),
        bundle.as_os_str(),
        OsStr::new("--yes"),
        archive.as_os_str(),
    ]))
    .with_context(|| format!("signing {}", archive.display()))
}

fn write_sbom(archive: &Path) -> Result<()> {
    let output = sidecar(archive, "sbom.json");
    cmd::run_stdout_file(
        cmd::command("syft").args([
            OsStr::new("scan"),
            archive.as_os_str(),
            OsStr::new("-o"),
            OsStr::new("cyclonedx-json"),
        ]),
        &output,
    )
    .with_context(|| format!("generating SBOM for {}", archive.display()))
}

fn sidecar(archive: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}.{}", archive.display(), suffix))
}

fn preserve_cargo_timings(target_dir: &Path, output_dir: &Path) -> Result<()> {
    let source = target_dir.join("cargo-timings");
    if !source.is_dir() {
        return Ok(());
    }
    let destination = output_dir.join("cargo-timings");
    fs::create_dir_all(&destination)
        .with_context(|| format!("creating {}", destination.display()))?;
    for entry in crate::fs_util::read_dir_sorted(&source)? {
        let path = entry.path();
        if path.is_file() {
            fs::copy(&path, destination.join(entry.file_name()))
                .with_context(|| format!("copying cargo timing {}", path.display()))?;
        }
    }
    Ok(())
}

fn write_sccache_stats(path: &Path) -> Result<()> {
    let stats = cmd::output(cmd::command("sccache").arg("--show-stats"))?;
    fs::write(path, &stats).with_context(|| format!("writing {}", path.display()))?;
    io::stdout()
        .lock()
        .write_all(&stats)
        .context("writing sccache stats to stdout")
}

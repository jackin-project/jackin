//! Builds `jackin-container` for Linux via cargo-zigbuild and caches the result.
//!
//! Usage:
//!   cargo run --bin build-jackin-container [-- [--arch arm64|amd64] [--export]]
//!
//! Flags:
//!   --arch arm64|amd64   Target architecture (default: matches current host container arch)
//!   --export             Print `export JACKIN_CONTAINER_BIN=<path>` suitable for eval
//!
//! Requires: zig and cargo-zigbuild installed (`mise install zig cargo:cargo-zigbuild`)
//!
//! After running, `jackin load` will find the binary in the standard cache path
//! automatically. Or use --export to set `JACKIN_CONTAINER_BIN` explicitly:
//!
//!   eval "$(cargo run --bin build-jackin-container -- --export)"
//!   cargo run --bin jackin -- load the-architect . --debug

use std::path::{Path, PathBuf};
use std::process;

use anyhow::{Context, Result};
use jackin::container_binary::{
    REQUIRED_VERSION, cached_binary_path, chmod_executable, container_arch,
};
use jackin::paths::JackinPaths;

// Compile-time workspace root: reliable even when cwd differs.
const WORKSPACE_ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn main() -> Result<()> {
    let Args { arch, export } = parse_args();
    let arch = arch.unwrap_or_else(|| container_arch().to_string());

    let paths = JackinPaths::detect()?;
    let cached = cached_binary_path(&paths.cache_dir, REQUIRED_VERSION, &arch);

    if let Some(parent) = cached.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache dir {}", parent.display()))?;
    }

    let workspace = PathBuf::from(WORKSPACE_ROOT);
    build_via_zigbuild(&workspace, &arch, &cached)?;

    if export {
        // Print only the export line — intended for `eval "$(...)"`
        println!("export JACKIN_CONTAINER_BIN={}", cached.display());
    } else {
        eprintln!(
            "[build] cached at: {}\n\
             [build] to use:    export JACKIN_CONTAINER_BIN={}",
            cached.display(),
            cached.display()
        );
    }
    Ok(())
}

struct Args {
    arch: Option<String>,
    export: bool,
}

fn parse_args() -> Args {
    let mut arch = None;
    let mut export = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--export" => export = true,
            "--arch" => arch = args.next(),
            s if s.starts_with("--arch=") => {
                arch = Some(s.trim_start_matches("--arch=").to_string());
            }
            _ => {}
        }
    }
    Args { arch, export }
}

fn zigbuild_target(arch: &str) -> &'static str {
    match arch {
        "arm64" => "aarch64-unknown-linux-gnu.2.17",
        _ => "x86_64-unknown-linux-gnu.2.17",
    }
}

// The target directory uses the base triple without the glibc version suffix.
fn target_triple(arch: &str) -> &'static str {
    match arch {
        "arm64" => "aarch64-unknown-linux-gnu",
        _ => "x86_64-unknown-linux-gnu",
    }
}

fn check_zigbuild_installed() -> Result<()> {
    // `cargo --list` prints one subcommand per line; zigbuild appears as "    zigbuild".
    let out = process::Command::new("cargo")
        .arg("--list")
        .output()
        .with_context(|| "failed to run `cargo --list`")?;
    let found = String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.trim() == "zigbuild");
    anyhow::ensure!(
        found,
        "cargo-zigbuild is not installed.\n\
         Install it with:\n\
           mise install zig cargo:cargo-zigbuild\n\
         or, without mise:\n\
           cargo install cargo-zigbuild\n\
           brew install zig  (or equivalent)"
    );
    Ok(())
}

fn build_via_zigbuild(workspace: &Path, arch: &str, dest: &Path) -> Result<()> {
    check_zigbuild_installed()?;

    let target = zigbuild_target(arch);
    eprintln!(
        "[build] cargo zigbuild -p jackin-container --target {target} ({REQUIRED_VERSION})\n\
         [build] first build ~2-3 min; subsequent builds incremental via cargo cache"
    );

    let status = process::Command::new("cargo")
        .args([
            "zigbuild",
            "--release",
            "-p",
            "jackin-container",
            "--target",
            target,
        ])
        .current_dir(workspace)
        .status()
        .with_context(|| "failed to spawn `cargo zigbuild`")?;

    anyhow::ensure!(
        status.success(),
        "cargo zigbuild failed for target {target}"
    );

    let built = workspace
        .join("target")
        .join(target_triple(arch))
        .join("release")
        .join("jackin-container");

    anyhow::ensure!(
        built.exists(),
        "build succeeded but binary not found at {}",
        built.display()
    );

    std::fs::copy(&built, dest)
        .with_context(|| format!("failed to copy {} to {}", built.display(), dest.display()))?;
    chmod_executable(dest);
    Ok(())
}

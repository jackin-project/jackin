//! Builds `jackin-container` for Linux via Docker and stores it in the local cache.
//!
//! Usage: `cargo run --bin build-jackin-container [-- --arch arm64|amd64]`
//!
//! After running this, `jackin load` will find the binary in cache and skip
//! the GitHub Releases download, enabling fully-offline local verification.

use std::path::{Path, PathBuf};
use std::process;

use anyhow::{Context, Result};
use jackin::container_binary::{
    REQUIRED_VERSION, cached_binary_path, chmod_executable, container_arch,
};
use jackin::paths::JackinPaths;

const RUST_IMAGE: &str = "rust:1.95.0";

fn main() -> Result<()> {
    let arch = parse_arch_arg().unwrap_or_else(|| container_arch().to_string());

    let paths = JackinPaths::detect()?;
    let cached = cached_binary_path(&paths.cache_dir, REQUIRED_VERSION, &arch);

    if let Some(parent) = cached.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache dir {}", parent.display()))?;
    }

    let workspace = find_workspace_root().ok_or_else(|| {
        anyhow::anyhow!(
            "cannot find workspace root (directory containing crates/jackin-container/).\n\
             Run this command from within the jackin source checkout."
        )
    })?;

    build_via_docker(&workspace, &arch, &cached)?;

    println!(
        "jackin-container {REQUIRED_VERSION} linux/{arch} cached at {}",
        cached.display()
    );
    println!("Run `jackin load` (or `cargo run --bin jackin -- load`) to verify.");
    Ok(())
}

fn parse_arch_arg() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--arch" {
            return args.next();
        }
        if let Some(arch) = arg.strip_prefix("--arch=") {
            return Some(arch.to_string());
        }
    }
    None
}

/// Walk up from cwd to find the repo root (contains `crates/jackin-container/`).
fn find_workspace_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let mut dir = cwd.as_path();
    for _ in 0..10 {
        if dir.join("crates").join("jackin-container").is_dir() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
    None
}

fn build_via_docker(workspace: &Path, arch: &str, dest: &Path) -> Result<()> {
    let platform = linux_platform(arch);
    let out_dir = dest.parent().expect("dest has parent");

    eprintln!(
        "[build] building jackin-container {REQUIRED_VERSION} for linux/{arch} via Docker \
         (first build ~2-3 min, subsequent builds incremental)..."
    );

    let workspace_str = workspace.display().to_string();
    let out_str = out_dir.display().to_string();
    let workspace_mount = format!("{workspace_str}:/workspace:ro");
    let out_mount = format!("{out_str}:/out");
    let build_cmd = "cd /workspace \
                     && cargo build --release -p jackin-container 2>&1 \
                     && cp /workspace/target/release/jackin-container /out/jackin-container";

    let status = process::Command::new("docker")
        .args([
            "run",
            "--rm",
            "--platform",
            platform,
            "-v",
            &workspace_mount,
            "-v",
            &out_mount,
            "-v",
            "jackin-container-build-cache:/root/.cargo/registry",
            RUST_IMAGE,
            "sh",
            "-c",
            build_cmd,
        ])
        .status()
        .with_context(|| "failed to run docker — is Docker running?")?;

    anyhow::ensure!(
        status.success(),
        "docker build of jackin-container failed (exit {status}).\n\
         Ensure Docker is running and {workspace} is accessible.",
        workspace = workspace.display()
    );

    chmod_executable(dest);
    eprintln!("[build] jackin-container written to {}", dest.display());
    Ok(())
}

fn linux_platform(arch: &str) -> &'static str {
    match arch {
        "arm64" => "linux/arm64",
        _ => "linux/amd64",
    }
}

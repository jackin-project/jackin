//! Builds `jackin-capsule` for Linux via cargo-zigbuild and caches the result.
//!
//! Usage:
//!   cargo run --bin build-jackin-capsule [-- [--arch arm64|amd64] [--export]]
//!
//! Flags:
//!   --arch arm64|amd64   Target architecture (default: matches current host container arch)
//!   --export             Print `export JACKIN_CAPSULE_BIN=<path>` suitable for eval
//!
//! Requires: zig and cargo-zigbuild installed (`mise install zig cargo:cargo-zigbuild`)
//!
//! After running, `jackin load` will find the binary in the standard cache path
//! automatically. Or use --export to set `JACKIN_CAPSULE_BIN` explicitly:
//!
//!   eval "$(cargo run --bin build-jackin-capsule -- --export)"
//!   cargo run --bin jackin -- load the-architect . --debug

use std::path::{Path, PathBuf};
use std::process;

use anyhow::{Context, Result};
use jackin::binary_artifact::{chmod_executable, container_arch};
use jackin::capsule_binary::{REQUIRED_VERSION, cached_binary_path};
use jackin::paths::JackinPaths;

// Compile-time workspace root: reliable even when cwd differs.
const WORKSPACE_ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn main() -> Result<()> {
    let Args { arch, export } = parse_args()?;
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
        // POSIX single-quote wrap so `eval "$(...)"` survives paths with
        // spaces or shell metacharacters. Embedded `'` is closed,
        // backslash-escaped, then re-opened.
        let escaped = cached.display().to_string().replace('\'', "'\\''");
        println!("export JACKIN_CAPSULE_BIN='{escaped}'");
    } else {
        eprintln!(
            "[build] cached at: {}\n\
             [build] to use:    export JACKIN_CAPSULE_BIN={}",
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

fn parse_args() -> Result<Args> {
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
            // Reject unknown tokens loudly. A silent catch-all here
            // lets a typo like `--arc amd64` fall back to the host arch
            // with no warning; the operator scp's the wrong-arch binary
            // and only finds out later when `docker run` greets them
            // with `exec format error`.
            other => anyhow::bail!(
                "unknown argument {other:?}; recognized flags: --arch <arm64|amd64>, --export"
            ),
        }
    }
    Ok(Args { arch, export })
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
    // The `cargo zigbuild` subcommand has dropped `--version` / `-V` in
    // recent cargo-zigbuild releases (only `-h/--help` and build flags
    // survive), so probing the subcommand exits non-zero even when the
    // binary is reachable. Probe the parent `cargo-zigbuild` binary
    // instead — its `--version` flag is part of the cargo-plugin
    // contract.
    const INSTALL_HINT: &str = "Install the pinned toolchain from mise.toml with:\n  \
                                mise install zig cargo:cargo-zigbuild";
    match process::Command::new("cargo-zigbuild")
        .arg("--version")
        .output()
    {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => anyhow::bail!(
            "cargo-zigbuild rejected `--version` (exit {}): {}\n{INSTALL_HINT}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(e) => {
            anyhow::bail!("cargo-zigbuild not reachable on PATH: {e}\n{INSTALL_HINT}")
        }
    }
}

fn ensure_rustup_target(triple: &str) -> Result<()> {
    let out = process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .with_context(|| "failed to run `rustup target list --installed`")?;
    let installed = String::from_utf8_lossy(&out.stdout);
    if installed.lines().any(|l| l.trim() == triple) {
        return Ok(());
    }
    eprintln!("[build] installing rustup target {triple}...");
    let status = process::Command::new("rustup")
        .args(["target", "add", triple])
        .status()
        .with_context(|| format!("failed to run `rustup target add {triple}`"))?;
    anyhow::ensure!(status.success(), "rustup target add {triple} failed");
    Ok(())
}

fn build_via_zigbuild(workspace: &Path, arch: &str, dest: &Path) -> Result<()> {
    check_zigbuild_installed()?;
    ensure_rustup_target(target_triple(arch))?;

    let target = zigbuild_target(arch);
    eprintln!(
        "[build] cargo zigbuild -p jackin-capsule --target {target} ({REQUIRED_VERSION})\n\
         [build] first build ~2-3 min; subsequent builds incremental via cargo cache"
    );

    let status = process::Command::new("cargo")
        .args([
            "zigbuild",
            "--release",
            "-p",
            "jackin-capsule",
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
        .join("jackin-capsule");

    anyhow::ensure!(
        built.exists(),
        "build succeeded but binary not found at {}",
        built.display()
    );

    std::fs::copy(&built, dest)
        .with_context(|| format!("failed to copy {} to {}", built.display(), dest.display()))?;
    chmod_executable(dest)
        .with_context(|| format!("setting +x on built binary {}", dest.display()))?;
    Ok(())
}

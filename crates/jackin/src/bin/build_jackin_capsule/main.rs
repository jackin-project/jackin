//! Builds `jackin-capsule` for Linux via cargo-zigbuild and caches the result.
//!
//! Directory-based bin layout keeps its unit tests at `tests.rs` without `#[path]`.
//!
//! Usage:
//!   cargo run --bin build-jackin-capsule [-- [--arch arm64|amd64] [--profile debug] [--features dhat-heap] [--export]]
//!
//! Flags:
//!   --arch arm64|amd64   Target architecture (default: matches current host container arch)
//!   --profile debug      Build with the `capsule-debug` profile: symbols retained, line tables
//!                        included. Backtraces resolve to function + file + line. Output binary
//!                        is named `jackin-capsule-debug`; the lean release binary is unchanged.
//!                        Also accepted as `--debug` for convenience.
//!   --export             Print `export JACKIN_CAPSULE_BIN=<path>` suitable for eval
//!   --features <list>    Pass a comma-separated feature list to the capsule build.
//!                        Used for opt-in performance telemetry builds.
//!
//! Requires: zig and cargo-zigbuild installed (`mise install zig cargo:cargo-zigbuild`)
//!
//! After running, `jackin load` will find the binary in the standard cache path
//! automatically. Or use --export to set `JACKIN_CAPSULE_BIN` explicitly:
//!
//!   eval "$(cargo run --bin build-jackin-capsule -- --export)"
//!   cargo run --bin jackin -- load the-architect . --debug
//!
//! Debug build for triage (backtraces resolve to function names):
//!
//!   eval "$(cargo run --bin build-jackin-capsule -- --profile debug --export)"
//!   cargo run --bin jackin -- load the-architect . --debug
//!
//! When using the debug capsule, set `RUST_BACKTRACE=full` for the richest traces:
//!
//! ```text
//! RUST_BACKTRACE=full cargo run --bin jackin -- load the-architect . --debug
//! ```

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::expect_used,
    reason = "developer build helper emits shell snippets/progress and fails fast on workspace discovery invariants"
)]

use std::path::{Path, PathBuf};
use std::process;

use anyhow::{Context, Result};
use jackin_core::paths::JackinPaths;
use jackin_image::binary_artifact::{chmod_executable, container_arch};
use jackin_image::capsule_binary::REQUIRED_VERSION;

// Compile-time crate manifest dir. Now that the binary lives in crates/jackin/,
// this points to crates/jackin/ — not the workspace root. See workspace_root().
const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

fn main() -> Result<()> {
    jackin::install_default_tls_provider();

    let Args {
        arch,
        export,
        features,
        profile,
    } = parse_args()?;
    let arch = arch.unwrap_or_else(|| container_arch().to_owned());

    let paths = JackinPaths::detect()?;
    let cached = binary_cache_path(
        &paths.cache_dir,
        REQUIRED_VERSION,
        &arch,
        profile,
        &features,
    );

    if let Some(parent) = cached.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache dir {}", parent.display()))?;
    }

    let workspace = workspace_root();
    build_via_zigbuild(&workspace, &arch, profile, &features, &cached)?;

    if export {
        // POSIX single-quote wrap so `eval "$(...)"` survives paths with
        // spaces or shell metacharacters. Embedded `'` is closed,
        // backslash-escaped, then re-opened.
        let escaped = cached.display().to_string().replace('\'', "'\\''");
        println!("export JACKIN_CAPSULE_BIN='{escaped}'");
    } else {
        let profile_note = if profile == BuildProfile::Debug {
            "\n[build] note: debug binary — set RUST_BACKTRACE=full for richest traces"
        } else {
            ""
        };
        let feature_note = if features.is_empty() {
            String::new()
        } else {
            format!("\n[build] note: features enabled: {}", features.join(","))
        };
        eprintln!(
            "[build] cached at: {}\n\
             [build] to use:    export JACKIN_CAPSULE_BIN={}{profile_note}{feature_note}",
            cached.display(),
            cached.display()
        );
    }
    Ok(())
}

/// Which Cargo profile to use for the capsule build.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BuildProfile {
    /// `--release` + `strip = "symbols"` — the lean default for shipping.
    Release,
    /// `[profile.capsule-debug]` — retains symbols + line tables; ~10× larger.
    /// Backtraces from `Backtrace::force_capture()` resolve to function + file + line number.
    Debug,
}

impl BuildProfile {
    const fn cargo_profile_arg(self) -> &'static str {
        match self {
            Self::Release => "release",
            // Named profile in workspace Cargo.toml.
            Self::Debug => "capsule-debug",
        }
    }

    /// Subdirectory the Cargo build puts the binary in (matches profile name).
    const fn target_subdir(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Debug => "capsule-debug",
        }
    }

    /// Filename suffix so debug and release can coexist in the same cache dir.
    const fn binary_suffix(self) -> &'static str {
        match self {
            Self::Release => "",
            Self::Debug => "-debug",
        }
    }
}

struct Args {
    arch: Option<String>,
    export: bool,
    features: Vec<String>,
    profile: BuildProfile,
}

/// Return the cache path for the built binary.
///
/// Release:  `<cache>/jackin-capsule/<version>/linux-<arch>/jackin-capsule`
/// Debug:    `<cache>/jackin-capsule/<version>/linux-<arch>/jackin-capsule-debug`
fn binary_cache_path(
    cache_dir: &Path,
    version: &str,
    arch: &str,
    profile: BuildProfile,
    features: &[String],
) -> PathBuf {
    let safe_version = version.replace('+', "_");
    let feature_suffix = feature_suffix(features);
    cache_dir
        .join("jackin-capsule")
        .join(safe_version)
        .join(format!("linux-{arch}"))
        .join(format!(
            "jackin-capsule{}{}",
            profile.binary_suffix(),
            feature_suffix
        ))
}

fn feature_suffix(features: &[String]) -> String {
    if features.is_empty() {
        return String::new();
    }
    let joined = features
        .iter()
        .map(|feature| sanitize_feature_for_filename(feature))
        .collect::<Vec<_>>()
        .join("-");
    format!("-features-{joined}")
}

fn sanitize_feature_for_filename(feature: &str) -> String {
    feature
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '-',
        })
        .collect()
}

/// Walk up from the crate manifest dir to find the Cargo workspace root.
///
/// Before Phase 8 the binary lived at the workspace root, so `CARGO_MANIFEST_DIR`
/// pointed there directly. Now it lives at `crates/jackin/`; the workspace root is
/// the ancestor whose Cargo.toml contains `[workspace]`.
fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(MANIFEST_DIR);
    loop {
        let toml = dir.join("Cargo.toml");
        if toml.exists() {
            let contents = std::fs::read_to_string(&toml).unwrap_or_default();
            if contents.contains("[workspace]") {
                return dir;
            }
        }
        dir = dir
            .parent()
            .expect("hit filesystem root without finding workspace Cargo.toml")
            .to_path_buf();
    }
}

fn parse_args() -> Result<Args> {
    let mut arch = None;
    let mut export = false;
    let mut features = Vec::new();
    let mut profile = BuildProfile::Release;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--export" => export = true,
            "--arch" => arch = args.next(),
            s if s.starts_with("--arch=") => {
                arch = Some(s.trim_start_matches("--arch=").to_owned());
            }
            // --debug is a convenience alias for --profile debug.
            "--debug" => profile = BuildProfile::Debug,
            "--profile" => {
                let name = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--profile requires a value (release|debug)"))?;
                profile = match name.as_str() {
                    "release" => BuildProfile::Release,
                    "debug" => BuildProfile::Debug,
                    other => anyhow::bail!(
                        "unknown profile {other:?}; recognized values: release, debug"
                    ),
                };
            }
            s if s.starts_with("--profile=") => {
                let name = s.trim_start_matches("--profile=");
                profile = match name {
                    "release" => BuildProfile::Release,
                    "debug" => BuildProfile::Debug,
                    other => anyhow::bail!(
                        "unknown profile {other:?}; recognized values: release, debug"
                    ),
                };
            }
            "--features" => {
                let value = args.next().ok_or_else(|| {
                    anyhow::anyhow!("--features requires a comma-separated value")
                })?;
                append_features(&mut features, &value);
            }
            s if s.starts_with("--features=") => {
                append_features(&mut features, s.trim_start_matches("--features="));
            }
            // Reject unknown tokens loudly. A silent catch-all here
            // lets a typo like `--arc amd64` fall back to the host arch
            // with no warning; the operator scp's the wrong-arch binary
            // and only finds out later when `docker run` greets them
            // with `exec format error`.
            other => anyhow::bail!(
                "unknown argument {other:?}; recognized flags: --arch <arm64|amd64>, --profile <release|debug>, --debug, --features <list>, --export"
            ),
        }
    }
    Ok(Args {
        arch,
        export,
        features,
        profile,
    })
}

fn append_features(features: &mut Vec<String>, raw: &str) {
    features.extend(
        raw.split(',')
            .map(str::trim)
            .filter(|feature| !feature.is_empty())
            .map(ToOwned::to_owned),
    );
    features.sort();
    features.dedup();
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
    let mut command = process::Command::new("cargo-zigbuild");
    command.arg("--version");
    #[expect(
        clippy::disallowed_methods,
        reason = "capsule build helper is a standalone build utility, not a render/runtime thread"
    )]
    match command.output() {
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
    let mut command = process::Command::new("rustup");
    command.args(["target", "list", "--installed"]);
    #[expect(
        clippy::disallowed_methods,
        reason = "capsule build helper is a standalone build utility, not a render/runtime thread"
    )]
    let out = command
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

fn build_via_zigbuild(
    workspace: &Path,
    arch: &str,
    profile: BuildProfile,
    features: &[String],
    dest: &Path,
) -> Result<()> {
    check_zigbuild_installed()?;
    ensure_rustup_target(target_triple(arch))?;

    let target = zigbuild_target(arch);
    let cargo_profile = profile.cargo_profile_arg();
    eprintln!(
        "[build] cargo zigbuild --profile {cargo_profile} -p jackin-capsule --target {target} ({REQUIRED_VERSION})\n\
         [build] first build ~2-3 min; subsequent builds incremental via cargo cache"
    );

    let cargo_args = [
        "zigbuild",
        "--profile",
        cargo_profile,
        "-p",
        "jackin-capsule",
        "--target",
        target,
    ];
    let mut command = cargo_command_with_fd_limit(&cargo_args);
    if !features.is_empty() {
        command.arg("--features").arg(features.join(","));
    }

    let status = command
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
        .join(profile.target_subdir())
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

fn cargo_command_with_fd_limit(args: &[&str]) -> process::Command {
    #[cfg(unix)]
    {
        let mut command = process::Command::new("sh");
        command
            .arg("-c")
            .arg("ulimit -n 20480 2>/dev/null || true; exec cargo \"$@\"")
            .arg("cargo")
            .args(args);
        command
    }
    #[cfg(not(unix))]
    {
        let mut command = process::Command::new("cargo");
        command.args(args);
        command
    }
}

#[cfg(test)]
mod tests;

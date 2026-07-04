// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Build-script helpers shared by jackin crates.
//!
//! Each workspace binary crate derives a runtime version string of the
//! form `<cargo-version>` or `<cargo-version>+<short-sha>` and arranges
//! to re-run the build script when the git HEAD moves.
use std::process::Command;

const WORKSPACE_GIT_DIR_FROM_CRATE: &str = "../../.git";

/// Derive the runtime version for a binary crate under `crates/<name>/`.
///
/// Returns the version string the caller should emit via
/// `println!("cargo:rustc-env=<NAME>={version}")`. The name is left
/// to the caller because each crate uses a distinct rustc-env name
/// (`JACKIN_VERSION`, `JACKIN_CAPSULE_VERSION`, ...).
#[must_use]
pub fn derive_workspace_crate_version() -> String {
    derive_version(WORKSPACE_GIT_DIR_FROM_CRATE)
}

/// `git_dir_relative` is the path to the workspace `.git/` directory,
/// relative to the crate that owns the build script. Used to emit the
/// `cargo:rerun-if-changed` hooks so a new HEAD or ref triggers a rebuild.
#[must_use]
#[expect(
    clippy::print_stdout,
    reason = "build-script helper must emit Cargo directives on stdout"
)]
fn derive_version(git_dir_relative: &str) -> String {
    println!("cargo:rerun-if-env-changed=JACKIN_VERSION_OVERRIDE");
    println!("cargo:rerun-if-changed={git_dir_relative}/HEAD");
    println!("cargo:rerun-if-changed={git_dir_relative}/refs");
    // `git gc` / `git pack-refs` consolidates loose refs into
    // .git/packed-refs; after that, branch-tip moves (fast-forwards,
    // fetches) update only packed-refs and never touch .git/refs/. Watch
    // it explicitly so the embedded version SHA stays in sync with the
    // working checkout regardless of which storage shape the local
    // repository uses.
    println!("cargo:rerun-if-changed={git_dir_relative}/packed-refs");

    if let Ok(override_version) = std::env::var("JACKIN_VERSION_OVERRIDE") {
        return override_version;
    }

    let cargo_version =
        std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.6.0-dev".to_owned());
    #[expect(
        clippy::disallowed_methods,
        reason = "build metadata runs in Cargo build-script context, not on a render/runtime thread"
    )]
    let short_sha = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned());

    short_sha.map_or_else(
        || cargo_version.clone(),
        |sha| format!("{cargo_version}+{sha}"),
    )
}

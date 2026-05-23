//! Build-script helpers shared by jackin crates.
//!
//! Both `jackin` and `jackin-capsule` derive a runtime version string
//! of the form `<cargo-version>` or `<cargo-version>+<short-sha>` and
//! arrange to re-run the build script when the git HEAD moves. The
//! only difference between the two call sites is the relative path
//! to the workspace `.git/` directory.

use std::process::Command;

/// Re-export the version-derivation contract.
///
/// `git_dir_relative` is the path to the workspace `.git/` directory,
/// relative to the crate that owns the build script (e.g. `".git"`
/// from the workspace root, `"../../.git"` from a `crates/<name>/`
/// subdirectory). Used to emit the `cargo:rerun-if-changed` hooks so
/// a new HEAD or ref triggers a rebuild.
///
/// Returns the version string the caller should emit via
/// `println!("cargo:rustc-env=<NAME>={version}")`. The name is left
/// to the caller because each crate uses a distinct rustc-env name
/// (`JACKIN_VERSION`, `JACKIN_CAPSULE_VERSION`, ...).
#[must_use]
pub fn derive_version(git_dir_relative: &str) -> String {
    println!("cargo:rerun-if-env-changed=JACKIN_VERSION_OVERRIDE");
    println!("cargo:rerun-if-changed={git_dir_relative}/HEAD");
    println!("cargo:rerun-if-changed={git_dir_relative}/refs");

    if let Ok(override_version) = std::env::var("JACKIN_VERSION_OVERRIDE") {
        return override_version;
    }

    let cargo_version =
        std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION set by cargo for build script");
    let short_sha = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());

    short_sha.map_or_else(
        || cargo_version.clone(),
        |sha| format!("{cargo_version}+{sha}"),
    )
}

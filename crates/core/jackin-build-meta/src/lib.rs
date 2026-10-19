use jackin_process::ExecRequest;
use std::path::{Path, PathBuf};

#[must_use]
pub fn derive_workspace_crate_version() -> String {
    derive_version()
}

#[must_use]
#[expect(
    clippy::print_stdout,
    reason = "build-script helper must emit Cargo directives on stdout"
)]
fn derive_version() -> String {
    println!("cargo:rerun-if-env-changed=JACKIN_VERSION_OVERRIDE");
    println!("cargo:rerun-if-env-changed=CI");
    let cargo_version =
        std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.6.0-dev".to_owned());
    if let Some(version) = local_version_override(
        std::env::var("JACKIN_VERSION_OVERRIDE").ok(),
        std::env::var_os("CI").is_some(),
        &cargo_version,
    ) {
        return version;
    }

    // Git owns administrative-path discovery: linked worktrees have a private
    // HEAD and shared refs, and packed-refs is legitimately absent in fresh clones.
    // Cargo treats a missing watched input as dirty on every subsequent build.
    if let Ok(directory) = std::env::current_dir() {
        for path in git_watch_paths(&directory) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    let short_sha =
        jackin_process::exec_sync(&ExecRequest::new("git", ["rev-parse", "--short=7", "HEAD"]))
            .ok()
            .filter(|o| o.success)
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned());

    short_sha.map_or_else(
        || cargo_version.clone(),
        |sha| format!("{cargo_version}+{sha}"),
    )
}

fn local_version_override(
    explicit_override: Option<String>,
    is_ci: bool,
    cargo_version: &str,
) -> Option<String> {
    if let Some(version) = explicit_override {
        return Some(version);
    }
    if is_ci {
        return None;
    }
    Some(cargo_version.to_owned())
}

#[cfg(test)]
mod tests;

/// Watch only existing Git inputs, independently of the caller crate's depth.
fn git_watch_paths(directory: &Path) -> Vec<PathBuf> {
    let request = ExecRequest::new(
        "git",
        [
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "HEAD",
            "--git-path",
            "refs",
            "--git-path",
            "packed-refs",
        ],
    )
    .cwd(directory);
    let Some(output) = jackin_process::exec_sync(&request)
        .ok()
        .filter(|output| output.success)
        .and_then(|output| String::from_utf8(output.stdout).ok())
    else {
        return Vec::new();
    };
    let paths = output.lines().map(PathBuf::from).collect::<Vec<_>>();
    if paths.len() != 3 || paths.iter().any(|path| !path.is_absolute()) {
        return Vec::new();
    }
    paths.into_iter().filter(|path| path.exists()).collect()
}

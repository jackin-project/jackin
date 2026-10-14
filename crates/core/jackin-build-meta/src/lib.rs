use jackin_process::ExecRequest;

const WORKSPACE_GIT_DIR_FROM_CRATE: &str = "../../.git";

#[must_use]
pub fn derive_workspace_crate_version() -> String {
    derive_version(WORKSPACE_GIT_DIR_FROM_CRATE)
}

#[must_use]
#[expect(
    clippy::print_stdout,
    reason = "build-script helper must emit Cargo directives on stdout"
)]
fn derive_version(git_dir_relative: &str) -> String {
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

    println!("cargo:rerun-if-changed={git_dir_relative}/HEAD");
    println!("cargo:rerun-if-changed={git_dir_relative}/refs");

    println!("cargo:rerun-if-changed={git_dir_relative}/packed-refs");

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

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=JACKIN_VERSION_OVERRIDE");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");

    let version = std::env::var("JACKIN_VERSION_OVERRIDE")
        .ok()
        .unwrap_or_else(|| {
            let cargo_version = env!("CARGO_PKG_VERSION");
            let short_sha = Command::new("git")
                .args(["rev-parse", "--short=7", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string());

            short_sha.map_or_else(
                || cargo_version.to_string(),
                |sha| format!("{cargo_version}+{sha}"),
            )
        });

    println!("cargo:rustc-env=JACKIN_VERSION={version}");
}

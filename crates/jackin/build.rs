fn main() {
    let version = jackin_build_meta::derive_workspace_crate_version();
    println!("cargo:rustc-env=JACKIN_VERSION={version}");
}

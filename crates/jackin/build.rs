fn main() {
    let version = jackin_build_meta::derive_version(".git");
    println!("cargo:rustc-env=JACKIN_VERSION={version}");
}

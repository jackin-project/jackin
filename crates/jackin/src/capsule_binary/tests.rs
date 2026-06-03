//! Tests for `capsule_binary`.
use super::*;

#[test]
fn download_url_dev_uses_preview_tag() {
    let url = download_url("0.6.0-dev+bf7df07", "amd64");
    assert!(url.contains("/releases/download/preview/"), "{url}");
    assert!(
        url.ends_with("/jackin-capsule-x86_64-unknown-linux-gnu.tar.gz"),
        "{url}"
    );
}

#[test]
fn download_url_stable_uses_version_tag() {
    let url = download_url("0.6.0", "amd64");
    assert!(url.contains("/releases/download/v0.6.0/"), "{url}");
    assert!(
        url.ends_with("/jackin-capsule-0.6.0-x86_64-unknown-linux-gnu.tar.gz"),
        "{url}"
    );
}

#[test]
fn download_url_arm64_uses_aarch64_target() {
    let url = download_url("0.6.0-dev+bf7df07", "arm64");
    assert!(
        url.ends_with("/jackin-capsule-aarch64-unknown-linux-gnu.tar.gz"),
        "{url}"
    );
}

#[test]
fn download_url_preview_uses_preview_tag() {
    let url = download_url("0.6.0-preview.411+bf7df07", "amd64");
    assert!(url.contains("/releases/download/preview/"), "{url}");
    assert!(
        url.ends_with("/jackin-capsule-x86_64-unknown-linux-gnu.tar.gz"),
        "{url}"
    );
}

#[test]
fn cached_path_replaces_plus_in_version() {
    let path = cached_binary_path(Path::new("/cache"), "0.6.0-dev+bf7df07", "amd64");
    let s = path.to_string_lossy();
    assert!(s.contains("0.6.0-dev_bf7df07"), "{s}");
    assert!(!s.contains('+'), "{s}");
}

#[test]
fn packaged_binary_path_for_keg_uses_libexec_arch_dir() {
    let path = packaged_binary_path_for_keg(
        Path::new("/opt/homebrew/Cellar/jackin-preview/0.6.0-preview.1"),
        "arm64",
    );
    assert_eq!(
        path,
        Path::new(
            "/opt/homebrew/Cellar/jackin-preview/0.6.0-preview.1/libexec/jackin-capsule/linux-arm64/jackin-capsule"
        )
    );
}

#[test]
fn linux_target_maps_arch() {
    assert_eq!(linux_target("arm64"), "aarch64-unknown-linux-gnu");
    assert_eq!(linux_target("amd64"), "x86_64-unknown-linux-gnu");
    assert_eq!(linux_target("x86_64"), "x86_64-unknown-linux-gnu");
}

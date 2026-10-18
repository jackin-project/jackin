// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn capsule_resolution_future_stays_within_launch_stack_budget() {
    let paths = JackinPaths::for_tests(Path::new("/not-executed"));
    let future = ensure_available(&paths);
    let bytes = size_of_val(&future);
    assert!(
        bytes <= 8192,
        "capsule future is {bytes} bytes; budget is 8192"
    );
}

#[test]
fn preview_url_uses_immutable_tag_and_canonical_target() {
    let version = "0.6.0-preview.411+bf7df07";
    for (arch, target) in [
        ("amd64", "x86_64-unknown-linux-gnu"),
        ("arm64", "aarch64-unknown-linux-gnu"),
    ] {
        assert_eq!(
            download_url(version, arch, ReleaseChannel::Preview),
            format!(
                "https://github.com/jackin-project/jackin/releases/download/v{version}/jackin-capsule-{target}.tar.gz"
            )
        );
    }
}

#[test]
fn only_canonical_preview_versions_authorize_remote_downloads() {
    assert_eq!(
        release_channel("0.6.0-preview.411+bf7df07"),
        ReleaseChannel::Preview
    );
    for version in [
        "0.6.0",
        "0.6.0-dev+bf7df07",
        "preview",
        "0.6.0-preview.0+bf7df07",
        "0.6.0-preview.01+bf7df07",
        "0.6.0-preview.+1+bf7df07",
        "0.6.0-preview.411+BF7DF07",
        "0.6.0-preview.411+bf7df0",
        "00.6.0-preview.411+bf7df07",
    ] {
        assert_eq!(
            release_channel(version),
            ReleaseChannel::LocalOnly,
            "{version}"
        );
    }
    assert!(local_capsule_only_message("0.6.0").contains("No release download was attempted"));
}

#[test]
fn cache_paths_bind_exact_immutable_version() {
    assert_eq!(
        cache_key_version("0.6.0-preview.411+bf7df07"),
        "0.6.0-preview.411_bf7df07"
    );
    assert_ne!(
        cached_binary_path(Path::new("/cache"), "0.6.0-preview.411+bf7df07", "amd64"),
        cached_binary_path(Path::new("/cache"), "0.6.0-preview.412+1111111", "amd64")
    );
    assert_eq!(
        resolved_cache_path(Path::new("/cache")),
        cached_binary_path(Path::new("/cache"), REQUIRED_VERSION, container_arch())
    );
}

#[tokio::test]
async fn explicit_test_stub_is_accepted_by_the_versioned_cache_resolver() {
    let dir = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(dir.path());
    let cached = resolved_cache_path(&paths.cache_dir);

    install_test_stub(&paths).unwrap();

    assert!(cached.is_file());
    assert!(!paths.cache_dir.join("jackin-capsule-test-stub").exists());
    assert_eq!(resolve_cached_or_fetch(&paths).await.unwrap(), cached);
}

#[test]
fn packaged_binary_path_uses_libexec_arch() {
    assert_eq!(
        packaged_binary_path_for_keg(
            Path::new("/opt/homebrew/Cellar/jackin-preview/0.6.0-preview.1"),
            "arm64"
        ),
        Path::new(
            "/opt/homebrew/Cellar/jackin-preview/0.6.0-preview.1/libexec/jackin-capsule/linux-arm64/jackin-capsule"
        )
    );
}

#[test]
fn cleanup_and_failure_messages_name_actionable_local_recovery() {
    assert_eq!(
        cleanup_warning_for(
            Path::new("/tmp/missing"),
            &std::io::Error::from(std::io::ErrorKind::NotFound)
        ),
        None
    );
    assert!(
        cleanup_warning_for(
            Path::new("/tmp/stuck"),
            &std::io::Error::from(std::io::ErrorKind::PermissionDenied)
        )
        .unwrap()
        .contains("manual cleanup")
    );
    let version = "0.6.0-preview.411+bf7df07";
    let message = download_failure_message(
        version,
        &download_url(version, "amd64", ReleaseChannel::Preview),
    );
    assert!(message.contains(&format!("releases/tag/v{version}")));
    assert!(message.contains("JACKIN_CAPSULE_BIN"));
}

#[cfg(not(target_os = "linux"))]
#[test]
fn embedded_version_requires_exact_token() {
    assert!(contains_exact_version(
        b"\0jackin-capsule\x000.6.4\0",
        b"0.6.4"
    ));
    for bytes in [
        b"0.6.40".as_slice(),
        b"0.6.4-preview.1+abcdef0",
        b"10.6.4",
        b"0.6.4-dev",
    ] {
        assert!(!contains_exact_version(bytes, b"0.6.4"));
    }
}

#[test]
fn archive_rejects_noncanonical_members_and_requires_one_executable() {
    for (name, mode, repeat, accepted) in [
        ("jackin-capsule", 0o755, 1, true),
        ("jackin-capsule", 0o644, 1, false),
        ("nested/jackin-capsule", 0o755, 1, false),
        ("jackin-capsule", 0o755, 2, false),
        ("wrong", 0o755, 1, false),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive.tar.gz");
        let file = std::fs::File::create(&path).unwrap();
        let gzip = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut tar = tar::Builder::new(gzip);
        for _ in 0..repeat {
            let mut header = tar::Header::new_gnu();
            header.set_size(4);
            header.set_mode(mode);
            header.set_cksum();
            tar.append_data(&mut header, name, b"test".as_slice())
                .unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
        assert_eq!(
            archive::extract_capsule(&path, &dir.path().join("capsule")).is_ok(),
            accepted,
            "{name} {mode:o} {repeat}"
        );
    }
}

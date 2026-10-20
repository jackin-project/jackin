// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::super::origin::{cache_path, parse};
use super::*;

#[test]
fn native_origin_requires_coherent_channel_version_and_full_source() {
    let source = "abcdef0".to_owned() + &"a".repeat(33);
    for (version, channel) in [("0.6.4", "stable"), ("0.6.4-preview.1+abcdef0", "preview")] {
        let origin = parse(version, Some(channel), Some(&source)).unwrap();
        assert_eq!(origin.source, source);
        assert_eq!(origin.channel, version_channel(version));
        for (wrong_channel, wrong_source) in [
            (None, Some(source.as_str())),
            (Some(channel), None),
            (Some("local"), Some(source.as_str())),
            (Some(channel), Some("abcdef0")),
        ] {
            assert!(parse(version, wrong_channel, wrong_source).is_none());
        }
    }
    for (version, channel, source) in [
        ("0.6.4", "preview", source.as_str()),
        ("0.6.4-preview.1+abcdef0", "stable", source.as_str()),
        ("0.6.4-preview.1+fffffff", "preview", source.as_str()),
        ("dev", "stable", source.as_str()),
    ] {
        assert!(parse(version, Some(channel), Some(source)).is_none());
    }
}

#[test]
fn native_cache_never_reuses_bare_local_or_other_source_cache() {
    let temporary = tempfile::tempdir().unwrap();
    let bare = cached_binary_path(temporary.path(), "0.6.4", "amd64");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    std::fs::write(
        &bare,
        b"local helper must never be executed by a native release",
    )
    .unwrap();
    let source = "a".repeat(40);
    let native = parse("0.6.4", Some("stable"), Some(&source));
    let actual = cache_path(temporary.path(), "0.6.4", "amd64", native);
    assert_ne!(actual, bare);
    assert!(!actual.exists());
    assert_eq!(cache_path(temporary.path(), "0.6.4", "amd64", None), bare);
    let version = "0.6.4-preview.1+abcdef0";
    let first = "abcdef0".to_owned() + &"a".repeat(33);
    let second = "abcdef0".to_owned() + &"b".repeat(33);
    assert_ne!(
        cache_path(
            temporary.path(),
            version,
            "amd64",
            parse(version, Some("preview"), Some(&first))
        ),
        cache_path(
            temporary.path(),
            version,
            "amd64",
            parse(version, Some("preview"), Some(&second))
        )
    );
}

#[tokio::test]
async fn native_lookup_ignores_executable_local_cache_and_accepts_only_its_own_cache() {
    use std::os::unix::fs::PermissionsExt;
    let temporary = tempfile::tempdir().unwrap();
    let marker = temporary.path().join("invoked");
    let version = "0.6.4";
    let source = "a".repeat(40);
    let native = parse(version, Some("stable"), Some(&source));
    let local = cached_binary_path(temporary.path(), version, "amd64");
    let own = cache_path(temporary.path(), version, "amd64", native);
    let stub = format!(
        "#!/bin/sh\ntouch '{}'\nprintf '%s\\n' 'jackin-capsule {version}'\n",
        marker.display()
    );
    std::fs::create_dir_all(local.parent().unwrap()).unwrap();
    std::fs::write(&local, &stub).unwrap();
    std::fs::set_permissions(&local, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        cache::lookup(temporary.path(), version, "amd64", native)
            .await
            .unwrap(),
        None
    );
    assert!(!marker.exists(), "native lookup executed a local helper");
    std::fs::create_dir_all(own.parent().unwrap()).unwrap();
    std::fs::copy(&local, &own).unwrap();
    assert_eq!(
        cache::lookup(temporary.path(), version, "amd64", native)
            .await
            .unwrap(),
        Some(own)
    );
    // Linux checks the helper by execution. Other hosts inspect its exact embedded version.
    assert_eq!(marker.exists(), cfg!(target_os = "linux"));
}

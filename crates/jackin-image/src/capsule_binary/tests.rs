// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
fn cache_key_version_collapses_dev_shas_to_preview_channel() {
    assert_eq!(cache_key_version("0.6.0-dev+bf7df07"), "0.6.0-dev+preview");
    assert_eq!(
        cache_key_version("0.6.0-preview.411+bf7df07"),
        "0.6.0-preview.411+preview"
    );
    assert_eq!(cache_key_version("0.6.0"), "0.6.0");
}

#[test]
fn normalize_sigstore_v03_bundle_maps_current_cosign_fields() {
    let raw = serde_json::json!({
        "mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json",
        "verificationMaterial": {
            "certificate": {
                "rawBytes": BASE64.encode([1, 2, 3, 4])
            },
            "tlogEntries": [{
                "logIndex": "42",
                "logId": { "keyId": BASE64.encode(b"rekor-key") },
                "integratedTime": "1234",
                "inclusionPromise": {
                    "signedEntryTimestamp": "set"
                },
                "canonicalizedBody": "body"
            }]
        },
        "messageSignature": {
            "signature": "sig"
        }
    })
    .to_string();

    let normalized = normalize_sigstore_v03_bundle(&raw).unwrap();
    let json: serde_json::Value = serde_json::from_str(&normalized).unwrap();

    assert_eq!(json["base64Signature"], "sig");
    assert_eq!(json["rekorBundle"]["SignedEntryTimestamp"], "set");
    assert_eq!(json["rekorBundle"]["Payload"]["body"], "body");
    assert_eq!(json["rekorBundle"]["Payload"]["integratedTime"], 1234);
    assert_eq!(json["rekorBundle"]["Payload"]["logIndex"], 42);
    assert_eq!(
        json["rekorBundle"]["Payload"]["logID"],
        "72656b6f722d6b6579"
    );

    let cert = BASE64.decode(json["cert"].as_str().unwrap()).unwrap();
    let cert = String::from_utf8(cert).unwrap();
    assert!(cert.starts_with("-----BEGIN CERTIFICATE-----\n"));
    assert!(cert.ends_with("-----END CERTIFICATE-----\n"));
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

#[test]
fn base_download_url_dev_uses_preview_tag() {
    let url = base_download_url("0.6.0-dev+bf7df07");
    assert_eq!(
        url,
        "https://github.com/jackin-project/jackin/releases/download/preview"
    );
}

#[test]
fn base_download_url_preview_uses_preview_tag() {
    let url = base_download_url("0.6.0-preview.411+bf7df07");
    assert_eq!(
        url,
        "https://github.com/jackin-project/jackin/releases/download/preview"
    );
}

#[test]
fn base_download_url_stable_uses_version_tag() {
    let url = base_download_url("0.6.0");
    assert_eq!(
        url,
        "https://github.com/jackin-project/jackin/releases/download/v0.6.0"
    );
}

#[test]
fn rekor_keys_decode_and_contain_expected_id() {
    let keys = rekor_verification_keys();
    assert_eq!(keys.len(), 2, "expected base64 and hex Rekor key IDs");
    assert!(
        keys.contains_key(SIGSTORE_REKOR_KEY_ID),
        "expected key ID {SIGSTORE_REKOR_KEY_ID} not found in decoded map"
    );
    assert!(
        keys.contains_key("c0d23d6ad406973f9559f3ba2d1ca01f84147d8ffc5b8445c224f98b9591801d"),
        "expected hex Rekor key ID not found in decoded map"
    );
    // Confirm the key variant is ECDSA P-256, matching Sigstore production Rekor.
    let key = keys.get(SIGSTORE_REKOR_KEY_ID).unwrap();
    assert!(
        matches!(
            key,
            sigstore::crypto::CosignVerificationKey::ECDSA_P256_SHA256_ASN1(_)
        ),
        "expected Rekor key to be ECDSA_P256_SHA256_ASN1 variant, got: {key:?}"
    );
}

#[test]
fn is_preview_version_matches_dev_and_preview_suffixes() {
    assert!(is_preview_version("0.6.0-dev+bf7df07"));
    assert!(is_preview_version("0.6.0-preview.411+bf7df07"));
    // Any string containing "-dev" is preview (substring match by design).
    assert!(is_preview_version("0.6.0-developer"));
    assert!(!is_preview_version("0.6.0"));
    // "-preview1" lacks the required trailing dot — not a preview channel version.
    assert!(!is_preview_version("0.6.0-preview1"));
}

#[test]
fn is_allowed_signer_san_accepts_release_and_preview_workflows() {
    // Accepted: release.yml with tag ref.
    assert!(is_allowed_signer_san(
        "https://github.com/jackin-project/jackin/.github/workflows/release.yml@refs/tags/v0.6.0"
    ));
    // Accepted: preview.yml with branch ref.
    assert!(is_allowed_signer_san(
        "https://github.com/jackin-project/jackin/.github/workflows/preview.yml@refs/heads/main"
    ));
    // Rejected: different workflow file in the same repo.
    assert!(!is_allowed_signer_san(
        "https://github.com/jackin-project/jackin/.github/workflows/evil.yml@refs/heads/main"
    ));
    // Rejected: correct workflow but in a different repository.
    assert!(!is_allowed_signer_san(
        "https://github.com/attacker/jackin/.github/workflows/release.yml@refs/tags/v0.6.0"
    ));
    // Rejected: partial path match (no trailing @ref).
    assert!(!is_allowed_signer_san(
        "https://github.com/jackin-project/jackin/.github/workflows/release.yml"
    ));
    // Rejected: empty string.
    assert!(!is_allowed_signer_san(""));
}

#[test]
fn resolved_cache_path_uses_container_arch_not_host_arch() {
    let path = resolved_cache_path(Path::new("/cache"));
    let s = path.to_string_lossy();
    assert!(s.contains(&format!("linux-{}", container_arch())), "{s}");
    assert!(!s.contains(std::env::consts::ARCH), "{s}");
}

#[test]
fn cleanup_warning_ignores_not_found() {
    let err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
    assert_eq!(
        cleanup_warning_for(Path::new("/tmp/jackin-capsule-never-created.tmp"), &err),
        None,
        "nothing to clean is success, not a warning",
    );
}

#[test]
fn cleanup_warning_names_path_on_real_failure() {
    let err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    let line = cleanup_warning_for(Path::new("/tmp/jackin-capsule-stuck.tmp"), &err)
        .expect("a real removal failure must warn");
    assert!(
        line.starts_with("[jackin❯] warning: temporary download file cleanup failed"),
        "{line}",
    );
    assert!(line.contains("/tmp/jackin-capsule-stuck.tmp"), "{line}");
    assert!(line.contains("manual cleanup may be needed"), "{line}");
}

#[test]
fn remove_with_debug_log_missing_file_is_silent_success() {
    let missing = std::env::temp_dir().join(format!(
        "jackin-capsule-test-missing-{}.tmp",
        std::process::id()
    ));
    drop(std::fs::remove_file(&missing));
    remove_with_debug_log(&missing);
    assert!(!missing.exists());
}

#[test]
fn download_failure_message_stable_names_versioned_asset_tag_and_url() {
    let url = download_url("0.6.4", "arm64");
    let msg = download_failure_message("0.6.4", &url, false);
    assert!(msg.contains("jackin-capsule 0.6.4"), "{msg}");
    assert!(
        msg.contains("releases/tag/v0.6.4"),
        "stable runbook must link the versioned tag: {msg}",
    );
    assert!(msg.contains("JACKIN_CAPSULE_BIN"), "{msg}");
    assert!(msg.contains("build-jackin-capsule"), "{msg}");
    assert!(
        msg.contains(&url),
        "attempted URL must always be shown: {msg}"
    );
    assert!(
        !msg.contains("releases/tag/preview"),
        "stable runbook must not blame the preview build: {msg}",
    );
}

#[test]
fn download_failure_message_preview_keeps_preview_runbook_and_url() {
    let url = download_url("0.6.5-dev+abc1234", "amd64");
    let msg = download_failure_message("0.6.5-dev+abc1234", &url, true);
    assert!(msg.contains("releases/tag/preview"), "{msg}");
    assert!(msg.contains("build-jackin-capsule"), "{msg}");
    assert!(
        msg.contains(&url),
        "attempted URL must always be shown: {msg}"
    );
    assert!(
        !msg.contains("releases/tag/v0.6.5"),
        "preview runbook must not link a versioned tag: {msg}",
    );
}

#[test]
fn resolved_cache_path_matches_the_path_the_resolver_reads() {
    let expected = cached_binary_path(
        Path::new("/cache"),
        &cache_key_version(REQUIRED_VERSION),
        container_arch(),
    );
    assert_eq!(resolved_cache_path(Path::new("/cache")), expected);
}

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::ImageError;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use jackin_core::JackinPaths;

use crate::binary_artifact::{
    chmod_executable, container_arch, extract_tar_gz_member, hash_file_sha256, is_executable_file,
    parse_sha256_hex, repair_executable_file, sha256_hex,
};

pub const REQUIRED_VERSION: &str = env!("JACKIN_VERSION");

const ASSET_PREFIX: &str = "jackin-capsule";

pub async fn ensure_available(paths: &JackinPaths) -> Result<PathBuf> {
    if let Some(bin_os) = std::env::var_os("JACKIN_CAPSULE_BIN") {
        let path = PathBuf::from(bin_os);
        if !is_executable_file(&path) {
            return Err(ImageError::msg(format!(
                "JACKIN_CAPSULE_BIN={} does not exist or is not executable",
                path.display()
            ))
            .into());
        }

        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::CapsuleBinary,
            jackin_telemetry::schema::enums::CacheResult::Bypass,
        );
        return Ok(path);
    }

    resolve_cached_or_fetch(paths).await
}

async fn resolve_cached_or_fetch(paths: &JackinPaths) -> Result<PathBuf> {
    let arch = container_arch();
    let cache_version = cache_key_version(REQUIRED_VERSION);
    let cached = resolved_cache_path(&paths.cache_dir);

    if is_executable_file(&cached) {
        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::CapsuleBinary,
            jackin_telemetry::schema::enums::CacheResult::Hit,
        );
        return Ok(cached);
    }
    if repair_executable_file(&cached)? {
        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::CapsuleBinary,
            jackin_telemetry::schema::enums::CacheResult::Reuse,
        );
        record(
            "capsule_binary_cache_repaired",
            &format!(
                "{REQUIRED_VERSION} linux/{arch} cache key {cache_version} at {}",
                cached.display()
            ),
        );
        return Ok(cached);
    }

    let stub_path = paths.cache_dir.join("jackin-capsule-test-stub");
    if cfg!(test) {
        install_test_stub(paths).context("installing in-process test stub")?;
        return Ok(stub_path);
    }
    if stub_path.exists() && is_executable_file(&stub_path) {
        return Ok(stub_path);
    }

    if let Some(packaged) = packaged_binary_path(REQUIRED_VERSION, arch).await {
        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::CapsuleBinary,
            jackin_telemetry::schema::enums::CacheResult::Reuse,
        );
        return Ok(packaged);
    }

    if let Some(parent) = cached.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache dir {}", parent.display()))?;
    }

    crate::telemetry_boundary::cache_decision(
        jackin_telemetry::schema::enums::CacheName::CapsuleBinary,
        jackin_telemetry::schema::enums::CacheResult::Miss,
    );
    download_and_cache(REQUIRED_VERSION, arch, &cached).await?;
    Ok(cached)
}

pub fn cached_binary_path(cache_dir: &Path, version: &str, arch: &str) -> PathBuf {
    let safe_version = version.replace('+', "_");
    cache_dir
        .join("jackin-capsule")
        .join(safe_version)
        .join(format!("linux-{arch}"))
        .join("jackin-capsule")
}

#[must_use]
pub fn resolved_cache_path(cache_dir: &Path) -> PathBuf {
    cached_binary_path(
        cache_dir,
        &cache_key_version(REQUIRED_VERSION),
        container_arch(),
    )
}

fn cache_key_version(version: &str) -> String {
    if is_preview_version(version) {
        let cargo_version = version
            .split_once('+')
            .map_or(version, |(prefix, _)| prefix);
        format!("{cargo_version}+preview")
    } else {
        version.to_owned()
    }
}

fn record(kind: &str, message: &str) {
    let _ = message;
    if let Some(run) = jackin_diagnostics::active_run() {
        run.compact(kind, kind);
    }
}

async fn packaged_binary_path(version: &str, arch: &str) -> Option<PathBuf> {
    let is_preview = is_preview_version(version);
    for candidate in packaged_binary_candidates(arch) {
        if !is_executable_file(&candidate) {
            continue;
        }
        if verify_version(&candidate, version, is_preview)
            .await
            .is_ok()
        {
            return Some(candidate);
        }
    }
    None
}

fn packaged_binary_candidates(arch: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        push_packaged_candidate(&mut candidates, &exe, arch);
        if let Ok(canonical) = exe.canonicalize() {
            push_packaged_candidate(&mut candidates, &canonical, arch);
        }
    }
    candidates
}

fn push_packaged_candidate(candidates: &mut Vec<PathBuf>, exe: &Path, arch: &str) {
    let Some(bin_dir) = exe.parent() else {
        return;
    };
    let Some(keg_root) = bin_dir.parent() else {
        return;
    };
    let candidate = packaged_binary_path_for_keg(keg_root, arch);
    if !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}

fn packaged_binary_path_for_keg(keg_root: &Path, arch: &str) -> PathBuf {
    keg_root
        .join("libexec")
        .join("jackin-capsule")
        .join(format!("linux-{arch}"))
        .join("jackin-capsule")
}

fn cleanup_warning_for(path: &Path, err: &std::io::Error) -> Option<String> {
    if err.kind() == std::io::ErrorKind::NotFound {
        return None;
    }
    Some(format!(
        "[jackin❯] warning: temporary download file cleanup failed for {}; manual cleanup may be needed",
        path.display()
    ))
}

fn remove_with_debug_log(path: &Path) {
    if let Err(e) = std::fs::remove_file(path)
        && let Some(line) = cleanup_warning_for(path, &e)
    {
        jackin_diagnostics::emit_compact_line("warning", &line);
    }
}

fn download_failure_message(version: &str, url: &str, is_preview: bool) -> String {
    if is_preview {
        format!(
            "jackin-capsule {version} download failed.\n\
             \n\
             Attempted URL: {url}\n\
             \n\
             Developing locally? Build and cache it first:\n\
               cargo run --bin build-jackin-capsule\n\
             Then retry `jackin load`.\n\
             \n\
             Using an installed jackin? The CI preview build may not\n\
             have completed yet. Wait a few minutes and retry, or check:\n\
               https://github.com/jackin-project/jackin/releases/tag/preview"
        )
    } else {
        format!(
            "jackin-capsule {version} download failed.\n\
             \n\
             Attempted URL: {url}\n\
             \n\
             The versioned release asset for jackin-capsule {version} may not\n\
             be published yet. Check the release tag:\n\
               https://github.com/jackin-project/jackin/releases/tag/v{version}\n\
             \n\
             Developing locally? Build and cache it first:\n\
               cargo run --bin build-jackin-capsule\n\
             or point JACKIN_CAPSULE_BIN at a local binary.\n\
             Then retry `jackin load`."
        )
    }
}

fn capsule_manifest_failure_message(version: &str, base_url: &str, is_preview: bool) -> String {
    let manifest_url = format!("{base_url}/capsule-manifest.json");
    let bundle_url = format!("{base_url}/capsule-manifest.json.bundle");
    if is_preview {
        format!(
            "fetching or verifying the signed jackin-capsule manifest failed for {version} \
             (preview channel).\n\
             \n\
             Attempted manifest: {manifest_url}\n\
             Attempted signature bundle: {bundle_url}\n\
             \n\
             The signed manifest is required; refusing to use an unverified capsule binary.\n\
             Preview builds use the rolling `preview` release. Wait for the preview build to\n\
             complete and retry, or build and cache it locally:\n\
               cargo run --bin build-jackin-capsule\n\
             Then retry `jackin load`."
        )
    } else {
        format!(
            "fetching or verifying the signed jackin-capsule manifest failed for {version} \
             (stable channel).\n\
             \n\
             Attempted manifest: {manifest_url}\n\
             Attempted signature bundle: {bundle_url}\n\
             \n\
             The signed manifest is required; refusing to use an unverified capsule binary.\n\
             Check the versioned release tag `v{version}`, or build and cache it locally:\n\
               cargo run --bin build-jackin-capsule\n\
             or point JACKIN_CAPSULE_BIN at a local binary.\n\
             Then retry `jackin load`."
        )
    }
}

async fn download_and_cache(version: &str, arch: &str, dest: &Path) -> Result<()> {
    let url = download_url(version, arch);
    let base_url = base_download_url(version);
    let tmp_archive = dest.with_extension("tar.gz.tmp");
    let tmp = dest.with_extension("tmp");

    let is_preview = is_preview_version(version);
    let (expected_sha_result, download_result) = tokio::join!(
        fetch_and_verify_manifest(version, &base_url, arch, is_preview),
        crate::telemetry_boundary::download_request(
            crate::telemetry_boundary::DownloadRoute::CapsuleArtifact,
            &url,
            jackin_docker::net::download_parallel(&url, &tmp_archive),
        ),
    );

    if let Err(e) = download_result {
        remove_with_debug_log(&tmp_archive);
        return Err(e).context(download_failure_message(version, &url, is_preview));
    }
    let expected_sha = match expected_sha_result {
        Ok(sha) => sha,
        Err(e) => {
            remove_with_debug_log(&tmp_archive);
            return Err(e)
                .with_context(|| capsule_manifest_failure_message(version, &base_url, is_preview));
        }
    };

    let archive_for_hash = tmp_archive.clone();
    let hash_result =
        jackin_telemetry::spawn::joined_blocking(move || hash_file_sha256(&archive_for_hash)).await;
    let actual_sha = match hash_result {
        Err(e) => {
            remove_with_debug_log(&tmp_archive);
            return Err(e).with_context(|| {
                format!(
                    "SHA-256 hash worker panicked or was cancelled for {}; \
                     delete the partial archive and retry",
                    tmp_archive.display()
                )
            });
        }
        Ok(Err(e)) => {
            remove_with_debug_log(&tmp_archive);
            return Err(e).with_context(|| {
                format!(
                    "hashing downloaded jackin-capsule archive at {}",
                    tmp_archive.display()
                )
            });
        }
        Ok(Ok(sha)) => sha,
    };
    if !actual_sha.eq_ignore_ascii_case(&expected_sha) {
        remove_with_debug_log(&tmp_archive);
        return Err(ImageError::msg(format!(
            "jackin-capsule SHA-256 mismatch for {url}\n  expected {expected_sha}\n  actual   {actual_sha}\n\
             refusing to cache the binary; investigate network tampering and retry."
        ))
        .into());
    }

    if let Err(e) = extract_tar_gz_member(&tmp_archive, "jackin-capsule", &tmp) {
        remove_with_debug_log(&tmp_archive);
        return Err(e).with_context(|| {
            format!(
                "extracting jackin-capsule from {} (archive passed SHA-256 check — \
                 if retrying fails, the release asset may be malformed; \
                 check https://github.com/jackin-project/jackin/releases)",
                tmp_archive.display()
            )
        });
    }
    remove_with_debug_log(&tmp_archive);

    if let Err(e) = chmod_executable(&tmp) {
        remove_with_debug_log(&tmp);
        return Err(e).with_context(|| {
            format!(
                "setting executable bit on cached jackin-capsule at {}; \
                 ensure {} is not mounted noexec",
                tmp.display(),
                tmp.parent().unwrap_or(&tmp).display()
            )
        });
    }

    if let Err(e) = verify_version(&tmp, version, is_preview).await {
        remove_with_debug_log(&tmp);
        return Err(e).with_context(|| {
            format!(
                "version verification failed for downloaded jackin-capsule {version} at {}",
                tmp.display()
            )
        });
    }
    if let Err(e) = std::fs::rename(&tmp, dest) {
        remove_with_debug_log(&tmp);
        return Err(e).with_context(|| {
            format!(
                "failed to move jackin-capsule to {}; \
                 if {} and {} are on different filesystems, \
                 ensure the cache directory is on the same volume as the download temp directory",
                dest.display(),
                tmp.display(),
                dest.display()
            )
        });
    }

    Ok(())
}

fn is_preview_version(version: &str) -> bool {
    version.contains("-dev") || version.contains("-preview.")
}

fn download_url(version: &str, arch: &str) -> String {
    let target = linux_target(arch);
    if is_preview_version(version) {
        let asset = format!("{ASSET_PREFIX}-{target}.tar.gz");
        format!("https://github.com/jackin-project/jackin/releases/download/preview/{asset}")
    } else {
        let asset = format!("{ASSET_PREFIX}-{version}-{target}.tar.gz");
        format!("https://github.com/jackin-project/jackin/releases/download/v{version}/{asset}")
    }
}

fn base_download_url(version: &str) -> String {
    if is_preview_version(version) {
        "https://github.com/jackin-project/jackin/releases/download/preview".to_owned()
    } else {
        format!("https://github.com/jackin-project/jackin/releases/download/v{version}")
    }
}

const SIGSTORE_REKOR_PUB_KEY_B64: &str = "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE2G2Y+2tabdTV5BcGiBIx0a9fAFwrkBbm\
     LSGtks4L3qX6yYY0zufBnhC8Ur/iy55GhWP/9A/bY2LhC30M9+RYtw==";
const SIGSTORE_REKOR_KEY_ID: &str = "wNI9atQGlz+VWfO6LRygH4QUfY/8W4RFwiT5i5WRgB0=";

#[expect(
    clippy::expect_used,
    reason = "compile-time Sigstore root constant must fail fast if malformed"
)]
fn rekor_verification_keys()
-> &'static std::collections::BTreeMap<String, sigstore::crypto::CosignVerificationKey> {
    use sigstore::crypto::CosignVerificationKey;

    static KEYS: std::sync::OnceLock<std::collections::BTreeMap<String, CosignVerificationKey>> =
        std::sync::OnceLock::new();
    KEYS.get_or_init(|| {
        let der = BASE64.decode(SIGSTORE_REKOR_PUB_KEY_B64).expect(
            "SIGSTORE_REKOR_PUB_KEY_B64 is malformed base64; \
                 update the constant from trust_root/prod/trusted_root.json in sigstore-rs",
        );
        let key = CosignVerificationKey::try_from_der(&der).expect(
            "SIGSTORE_REKOR_PUB_KEY_B64 decoded to invalid SPKI DER; \
             verify the key matches logId wNI9atQG... in trusted_root.json",
        );
        let key_id_hex =
            hex::encode(BASE64.decode(SIGSTORE_REKOR_KEY_ID).expect(
                "SIGSTORE_REKOR_KEY_ID is malformed base64; update it with the Rekor log ID",
            ));
        std::collections::BTreeMap::from([
            (SIGSTORE_REKOR_KEY_ID.to_owned(), key.clone()),
            (key_id_hex, key),
        ])
    })
}

#[derive(serde::Deserialize)]
struct CapsuleManifest {
    version: String,
    targets: std::collections::HashMap<String, String>,
}

#[derive(serde::Deserialize)]
struct RekorBody {
    kind: String,
    spec: RekorSpec,
}
#[derive(serde::Deserialize)]
struct RekorSpec {
    data: RekorData,
    signature: RekorSig,
}
#[derive(serde::Deserialize)]
struct RekorData {
    hash: RekorHash,
}
#[derive(serde::Deserialize)]
struct RekorHash {
    algorithm: String,
    value: String,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RekorSig {
    content: String,

    public_key: RekorPublicKey,
}

#[derive(serde::Deserialize)]
struct RekorPublicKey {
    content: String,
}

fn verify_rekor_body_binds_bundle(
    bundle: &sigstore::cosign::bundle::SignedArtifactBundle,
    manifest_bytes: &[u8],
) -> Result<()> {
    use sha2::{Digest, Sha256};

    let body_json = BASE64
        .decode(&bundle.rekor_bundle.payload.body)
        .context("base64-decoding Rekor payload body")?;

    let body: RekorBody =
        serde_json::from_slice(&body_json).context("parsing Rekor payload body as hashedrekord")?;

    if body.kind != "hashedrekord" {
        return Err(ImageError::msg(format!(
            "Rekor entry kind is {:?}; expected \"hashedrekord\" — the signing workflow \
             may have been reconfigured to use a different entry type",
            body.kind
        ))
        .into());
    }

    if body.spec.data.hash.algorithm != "sha256" {
        return Err(ImageError::msg(format!(
            "Rekor entry uses unexpected hash algorithm {:?}; expected sha256",
            body.spec.data.hash.algorithm
        ))
        .into());
    }
    let manifest_sha256 = sha256_hex(Sha256::digest(manifest_bytes));
    if !body
        .spec
        .data
        .hash
        .value
        .eq_ignore_ascii_case(&manifest_sha256)
    {
        return Err(ImageError::msg(format!(
            "Rekor entry covers a different artifact (body hash {}, actual manifest hash {}); \
             the bundle may have been transplanted from another signing event",
            body.spec.data.hash.value, manifest_sha256
        ))
        .into());
    }

    if body.spec.signature.public_key.content != bundle.cert {
        return Err(ImageError::msg(
            "Rekor log entry covers a different certificate than the one in bundle.cert; \
             bundle.cert may have been substituted after the log entry was created",
        )
        .into());
    }

    if body.spec.signature.content != bundle.base64_signature {
        return Err(ImageError::msg(
            "Rekor log entry covers a different signature than bundle.base64_signature; \
             bundle.base64_signature may have been substituted after the log entry was created",
        )
        .into());
    }

    Ok(())
}

fn verified_signed_artifact_bundle(
    raw: &str,
) -> Result<sigstore::cosign::bundle::SignedArtifactBundle> {
    use sigstore::cosign::bundle::SignedArtifactBundle;

    match SignedArtifactBundle::new_verified(raw, rekor_verification_keys()) {
        Ok(bundle) => Ok(bundle),
        Err(legacy_err) => {
            let normalized = normalize_sigstore_v03_bundle(raw).with_context(|| {
                format!("legacy cosign bundle parse failed first: {legacy_err}")
            })?;
            SignedArtifactBundle::new_verified(&normalized, rekor_verification_keys())
                .context("verifying normalized Sigstore bundle v0.3 as legacy cosign bundle")
        }
    }
}

fn normalize_sigstore_v03_bundle(raw: &str) -> Result<String> {
    let json: serde_json::Value =
        serde_json::from_str(raw).context("parsing Sigstore bundle v0.3 JSON")?;
    let media_type = json_pointer_string(&json, "/mediaType")?;
    if media_type != "application/vnd.dev.sigstore.bundle.v0.3+json" {
        return Err(ImageError::msg(format!(
            "unsupported capsule manifest bundle mediaType {media_type:?}"
        ))
        .into());
    }

    let cert_der_b64 = json_pointer_string(&json, "/verificationMaterial/certificate/rawBytes")?;
    let cert_der = BASE64
        .decode(cert_der_b64)
        .context("base64-decoding Sigstore bundle v0.3 certificate rawBytes")?;
    let cert_pem = der_cert_to_pem(&cert_der);
    let cert = BASE64.encode(cert_pem.as_bytes());

    let signature = json_pointer_string(&json, "/messageSignature/signature")?;
    let entry = json
        .pointer("/verificationMaterial/tlogEntries/0")
        .context("Sigstore bundle v0.3 missing verificationMaterial.tlogEntries[0]")?;
    let signed_entry_timestamp =
        json_pointer_string(entry, "/inclusionPromise/signedEntryTimestamp")?;
    let body = json_pointer_string(entry, "/canonicalizedBody")?;
    let integrated_time = json_pointer_i64(entry, "/integratedTime")?;
    let log_index = json_pointer_i64(entry, "/logIndex")?;
    let log_id = hex::encode(
        BASE64
            .decode(json_pointer_string(entry, "/logId/keyId")?)
            .context("base64-decoding Sigstore bundle v0.3 logId.keyId")?,
    );

    Ok(serde_json::json!({
        "base64Signature": signature,
        "cert": cert,
        "rekorBundle": {
            "SignedEntryTimestamp": signed_entry_timestamp,
            "Payload": {
                "body": body,
                "integratedTime": integrated_time,
                "logIndex": log_index,
                "logID": log_id,
            },
        },
    })
    .to_string())
}

fn json_pointer_string(json: &serde_json::Value, pointer: &str) -> Result<String> {
    json.pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow::Error::from(ImageError::MissingJsonString {
                pointer: pointer.to_owned(),
            })
        })
}

fn json_pointer_i64(json: &serde_json::Value, pointer: &str) -> Result<i64> {
    let value = json.pointer(pointer).ok_or_else(|| {
        anyhow::Error::from(ImageError::MissingJsonInteger {
            pointer: pointer.to_owned(),
        })
    })?;
    if let Some(number) = value.as_i64() {
        return Ok(number);
    }
    value
        .as_str()
        .ok_or_else(|| {
            anyhow::Error::from(ImageError::JsonIntegerString {
                pointer: pointer.to_owned(),
            })
        })?
        .parse()
        .with_context(|| format!("parsing integer field {pointer}"))
}

fn der_cert_to_pem(der: &[u8]) -> String {
    let encoded = BASE64.encode(der);
    let mut pem = String::from("-----BEGIN CERTIFICATE-----\n");
    for chunk in encoded.as_bytes().chunks(64) {
        pem.push_str(&String::from_utf8_lossy(chunk));
        pem.push('\n');
    }
    pem.push_str("-----END CERTIFICATE-----\n");
    pem
}

async fn fetch_and_verify_manifest(
    version: &str,
    base_url: &str,
    arch: &str,
    is_preview: bool,
) -> Result<String> {
    use sigstore::cosign::{Client, CosignCapabilities};

    let manifest_url = format!("{base_url}/capsule-manifest.json");
    let bundle_url = format!("{base_url}/capsule-manifest.json.bundle");

    let (manifest_result, bundle_result) = tokio::join!(
        crate::telemetry_boundary::download_request(
            crate::telemetry_boundary::DownloadRoute::CapsuleManifest,
            &manifest_url,
            jackin_docker::net::fetch_text(&manifest_url),
        ),
        crate::telemetry_boundary::download_request(
            crate::telemetry_boundary::DownloadRoute::CapsuleManifestBundle,
            &bundle_url,
            jackin_docker::net::fetch_text(&bundle_url),
        ),
    );

    let manifest_text =
        manifest_result.with_context(|| format!("fetching capsule manifest at {manifest_url}"))?;
    let bundle_text = bundle_result
        .with_context(|| format!("fetching capsule manifest bundle at {bundle_url}"))?;

    let manifest_bytes = manifest_text.as_bytes();

    let bundle = verified_signed_artifact_bundle(&bundle_text)
        .context("verifying Rekor log entry in capsule manifest bundle")?;

    verify_rekor_body_binds_bundle(&bundle, manifest_bytes).context(
        "Rekor log entry does not cover the certificate or manifest hash in this bundle",
    )?;

    let cert_pem = String::from_utf8(
        BASE64
            .decode(&bundle.cert)
            .context("base64-decoding certificate from capsule manifest bundle")?,
    )
    .context("certificate in bundle is not valid UTF-8")?;

    Client::verify_blob(&cert_pem, &bundle.base64_signature, manifest_bytes)
        .with_context(|| {
            format!(
                "jackin-capsule manifest signature verification failed for {manifest_url}\n\
                 expected signer: ^https://github.com/jackin-project/jackin/\n\
                 refusing to use unverified capsule binary; investigate release tampering and retry.\n\
                 If you built the binary locally, set JACKIN_CAPSULE_BIN=/path/to/jackin-capsule instead."
            )
        })?;

    let san = extract_cert_san_url(&cert_pem)?;
    if !is_allowed_signer_san(&san) {
        return Err(ImageError::msg(format!(
            "capsule manifest signed by unexpected signer {san:?}; \
             expected release.yml or preview.yml workflow in jackin-project/jackin"
        ))
        .into());
    }

    let manifest: CapsuleManifest =
        serde_json::from_str(&manifest_text).context("parsing verified capsule-manifest.json")?;

    if !is_preview && manifest.version != version {
        return Err(ImageError::msg(format!(
            "signed capsule manifest carries version {:?} but expected {version:?}; \
             the release asset may have been replaced or the manifest is stale",
            manifest.version
        ))
        .into());
    }

    let target = linux_target(arch);
    let sha = manifest
        .targets
        .get(target)
        .with_context(|| format!("capsule manifest missing target {target:?} (arch {arch:?})"))?;
    parse_sha256_hex(sha).with_context(|| format!("invalid sha256 in manifest for {target}"))
}

fn extract_cert_san_url(cert_pem: &str) -> Result<String> {
    use x509_cert::Certificate;
    use x509_cert::der::DecodePem;
    use x509_cert::ext::pkix::SubjectAltName;
    use x509_cert::ext::pkix::name::GeneralName;

    let cert = Certificate::from_pem(cert_pem.as_bytes()).context("parsing PEM certificate")?;

    for result in cert.tbs_certificate().filter_extensions::<SubjectAltName>() {
        let (_, san) = result.context("parsing SubjectAltName extension")?;
        for name in &san.0 {
            if let GeneralName::UniformResourceIdentifier(uri) = name {
                return Ok(uri.to_string());
            }
        }
    }
    Err(ImageError::NoUriSan.into())
}

fn is_allowed_signer_san(san: &str) -> bool {
    san.starts_with("https://github.com/jackin-project/jackin/.github/workflows/release.yml@")
        || san
            .starts_with("https://github.com/jackin-project/jackin/.github/workflows/preview.yml@")
}

fn linux_target(arch: &str) -> &'static str {
    match arch {
        "arm64" => "aarch64-unknown-linux-gnu",
        _ => "x86_64-unknown-linux-gnu",
    }
}

pub fn install_test_stub(paths: &JackinPaths) -> Result<()> {
    let stub = paths.cache_dir.join("jackin-capsule-test-stub");
    if let Some(parent) = stub.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache dir {}", parent.display()))?;
    }
    if !stub.exists() {
        std::fs::write(&stub, b"#!/bin/sh\necho jackin-capsule test stub\n")
            .with_context(|| format!("writing test stub at {}", stub.display()))?;
    }
    chmod_executable(&stub)
        .with_context(|| format!("setting +x on test stub {}", stub.display()))?;
    Ok(())
}

#[cfg_attr(
    not(target_os = "linux"),
    expect(
        clippy::unused_async,
        reason = "async retained for caller symmetry with the Linux exec path"
    )
)]
async fn verify_version(binary: &Path, expected: &str, is_preview: bool) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let request = jackin_process::ExecRequest::new(binary, ["--version"])
            .timeout(std::time::Duration::from_secs(15));
        let output = crate::process_telemetry::exec_async(
            &request,
            jackin_telemetry::schema::enums::ProcessExecutableName::JackinCapsule,
        )
        .await
        .context("running downloaded capsule verification process")?;
        if !output.success {
            return Err(ImageError::msg("downloaded capsule verification failed").into());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if is_preview {
            if !stdout.contains(ASSET_PREFIX) {
                return Err(
                    ImageError::msg("downloaded capsule identity verification failed").into(),
                );
            }
            return Ok(());
        }
        if !stdout.contains(expected) {
            return Err(ImageError::msg("downloaded capsule version verification failed").into());
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let bytes = std::fs::read(binary)
            .with_context(|| format!("reading {} for verification", binary.display()))?;
        if !contains_subslice(&bytes, ASSET_PREFIX.as_bytes()) {
            return Err(ImageError::msg(format!(
                "downloaded binary at {} does not contain the {ASSET_PREFIX} identity marker",
                binary.display()
            ))
            .into());
        }
        if !is_preview && !contains_subslice(&bytes, expected.as_bytes()) {
            return Err(ImageError::msg(format!(
                "downloaded binary at {} does not contain expected version {expected}.\n\
                 Stable release ↔ asset mapping appears to have drifted.\n\
                 Delete and retry: rm -f {}",
                binary.display(),
                binary.display()
            ))
            .into());
        }
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests;

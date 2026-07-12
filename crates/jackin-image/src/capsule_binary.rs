//! Locate or bundle the `jackin-capsule` binary embedded into derived role images.
//!
//! Acquisition strategy (priority order): `JACKIN_CAPSULE_BIN` env override →
//! local cache hit → Homebrew formula `libexec/` → GitHub Release download.
//!
//! Not responsible for: building the binary, injecting it into a Docker image,
//! or verifying runtime compatibility between capsule and host versions.

/// Download, cache, and verify the `jackin-capsule` binary.
///
/// Acquisition strategy — in priority order:
///
/// **`JACKIN_CAPSULE_BIN=/path`** (env var set):
///   Use that binary directly. No cache, no download. Intended for local
///   development and PR verification when the binary was built with
///   `cargo run --bin build-jackin-capsule`.
///
/// **Cache hit** (`~/.jackin/cache/jackin-capsule/<version>/linux-<arch>/`):
///   Use the already-cached binary.
///
/// **Packaged binary** (Homebrew formula installed with a Capsule resource):
///   Use the binary installed under the formula's `libexec/` tree.
///
/// **Dev or preview version** (`-dev` or `-preview.` suffix, cache miss):
///   Download the `.tar.gz` archive from the rolling `preview`
///   GitHub Release tag, verify it, and extract the binary.
///
/// **Stable release** (no `-dev`, no `-preview`, cache miss):
///   Download the `.tar.gz` archive from the versioned `v<version>`
///   GitHub Release tag, verify it, and extract the binary.
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

/// Ensure the `jackin-capsule` binary is available and return its path.
pub async fn ensure_available(paths: &JackinPaths) -> Result<PathBuf> {
    // Explicit override: operator built the binary themselves and told us where it is.
    if let Some(bin_os) = std::env::var_os("JACKIN_CAPSULE_BIN") {
        let path = PathBuf::from(bin_os);
        if !is_executable_file(&path) {
            return Err(ImageError::msg(format!(
                "JACKIN_CAPSULE_BIN={} does not exist or is not executable",
                path.display()
            ))
            .into());
        }
        // Operator-trust note: this override path bypasses the
        // SHA-256 verification that the cache-miss download path
        // applies. The operator pointing at a local file is
        // explicitly opting in (typically a `cargo run --bin
        // build-jackin-capsule` artifact, the path that test
        // suites and dev iteration use). Production hosts that
        // never set this env var still get the strong checksum
        // gate from `download_and_cache`. The note is debug-only so
        // it never streams over the launch progress surface; the
        // rich launch screen owns the terminal during this call.
        jackin_diagnostics::debug_log!(
            "capsule_binary",
            "JACKIN_CAPSULE_BIN override at {} (skipping SHA-256 verification)",
            path.display()
        );
        return Ok(path);
    }

    resolve_cached_or_fetch(paths).await
}

/// Resolve the binary from cache (repairing a stale executable bit),
/// packaged Homebrew tree, or download — everything *after* the
/// `JACKIN_CAPSULE_BIN` override. Split out so tests can exercise the
/// cache-repair path without unsetting the process-global env var
/// (`unsafe` env mutation is forbidden workspace-wide, and CI exports
/// `JACKIN_CAPSULE_BIN` for the whole nextest run).
async fn resolve_cached_or_fetch(paths: &JackinPaths) -> Result<PathBuf> {
    let arch = container_arch();
    let cache_version = cache_key_version(REQUIRED_VERSION);
    let cached = cached_binary_path(&paths.cache_dir, &cache_version, arch);

    if is_executable_file(&cached) {
        jackin_diagnostics::debug_log!(
            "capsule_binary",
            "cache hit for jackin-capsule {REQUIRED_VERSION} linux/{arch} (cache key {cache_version})"
        );
        return Ok(cached);
    }
    if repair_executable_file(&cached)? {
        jackin_diagnostics::debug_log!(
            "capsule_binary",
            "repaired executable bit for cached jackin-capsule {REQUIRED_VERSION} linux/{arch} (cache key {cache_version}) at {}",
            cached.display()
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

    // Tests stub the binary by writing a placeholder file at this
    // well-known location (see `install_test_stub`). Used by both lib
    // tests via `cfg!(test)` and integration tests via the helper
    // call; production hosts never have this file because the cache
    // dir lives under `~/.jackin/cache/` and gets the real binary on
    // first run. `cfg!(test)` short-circuits the stub write for lib
    // tests so they don't need any per-test setup, after cached-binary
    // behavior has had a chance to run.
    let stub_path = paths.cache_dir.join("jackin-capsule-test-stub");
    if cfg!(test) {
        install_test_stub(paths).context("installing in-process test stub")?;
        return Ok(stub_path);
    }
    if stub_path.exists() && is_executable_file(&stub_path) {
        return Ok(stub_path);
    }

    if let Some(packaged) = packaged_binary_path(REQUIRED_VERSION, arch).await {
        jackin_diagnostics::debug_log!(
            "capsule_binary",
            "using packaged jackin-capsule {REQUIRED_VERSION} linux/{arch} at {}",
            packaged.display()
        );
        return Ok(packaged);
    }

    if let Some(parent) = cached.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache dir {}", parent.display()))?;
    }

    download_and_cache(REQUIRED_VERSION, arch, &cached).await?;
    Ok(cached)
}

/// Path in the local cache for a given version + arch.
pub fn cached_binary_path(cache_dir: &Path, version: &str, arch: &str) -> PathBuf {
    let safe_version = version.replace('+', "_");
    cache_dir
        .join("jackin-capsule")
        .join(safe_version)
        .join(format!("linux-{arch}"))
        .join("jackin-capsule")
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
    if let Some(run) = jackin_diagnostics::active_run() {
        run.compact(kind, message);
    } else {
        jackin_diagnostics::debug_log!("capsule_binary", "{kind}: {message}");
    }
}

async fn packaged_binary_path(version: &str, arch: &str) -> Option<PathBuf> {
    let is_preview = is_preview_version(version);
    for candidate in packaged_binary_candidates(arch) {
        if !is_executable_file(&candidate) {
            continue;
        }
        match verify_version(&candidate, version, is_preview).await {
            Ok(()) => return Some(candidate),
            Err(err) => jackin_diagnostics::debug_log!(
                "capsule_binary",
                "ignoring packaged jackin-capsule at {}: {err}",
                candidate.display()
            ),
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

/// Remove a file, emitting a compact always-visible warning if the removal fails.
/// Used at every cleanup site in `download_and_cache` — both error paths and the
/// success-path archive removal after extraction — so a failed cleanup is always
/// observable regardless of `--debug`, and the operator can manually remove the
/// stale file to recover disk space.
fn remove_with_debug_log(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        jackin_diagnostics::emit_compact_line(
            "warning",
            &format!(
                "[jackin] warning: failed to remove temp file at {}: {e} \
                 (manual cleanup may be needed)",
                path.display()
            ),
        );
    }
}

async fn download_and_cache(version: &str, arch: &str, dest: &Path) -> Result<()> {
    let url = download_url(version, arch);
    let base_url = base_download_url(version);
    jackin_diagnostics::debug_log!(
        "capsule_binary",
        "downloading jackin-capsule {version} for linux/{arch}"
    );
    let tmp_archive = dest.with_extension("tar.gz.tmp");
    let tmp = dest.with_extension("tmp");

    // Fetch the signed capsule manifest (verifies cosign bundle + identity) and
    // download the archive concurrently. The manifest returns the expected SHA-256
    // for this arch.
    // Resolve once; both the manifest verification and the post-download
    // version check use the same channel determination.
    let is_preview = is_preview_version(version);
    let (expected_sha_result, download_result) = tokio::join!(
        fetch_and_verify_manifest(version, &base_url, arch, is_preview),
        jackin_docker::net::download_parallel(&url, &tmp_archive),
    );

    // Either failure must remove the partial archive so a retry starts clean —
    // the manifest fetch and the download run concurrently, so a manifest error can land
    // with the archive already fully written.
    if let Err(e) = download_result {
        remove_with_debug_log(&tmp_archive);
        return Err(e).context(format!(
            "jackin-capsule {version} download failed.\n\
             \n\
             Developing locally? Build and cache it first:\n\
               cargo run --bin build-jackin-capsule\n\
             Then retry `jackin load`.\n\
             \n\
             Using an installed jackin? The CI preview build may not\n\
             have completed yet. Wait a few minutes and retry, or check:\n\
               https://github.com/jackin-project/jackin/releases/tag/preview"
        ));
    }
    let expected_sha = match expected_sha_result {
        Ok(sha) => sha,
        Err(e) => {
            remove_with_debug_log(&tmp_archive);
            return Err(e).with_context(|| {
                format!(
                    "fetching or verifying signed capsule manifest for jackin-capsule {version}"
                )
            });
        }
    };

    // Verify the attested SHA-256 from the signed manifest against the downloaded archive.
    // A mismatch means the archive does not match what CI signed.
    // Hashing a multi-MB archive parks the tokio worker; run it on the
    // blocking pool so concurrent launch / TUI tasks keep progressing.
    let archive_for_hash = tmp_archive.clone();
    let hash_result =
        tokio::task::spawn_blocking(move || hash_file_sha256(&archive_for_hash)).await;
    let actual_sha = match hash_result {
        Err(e) => {
            // JoinError: worker panicked or runtime shut down. Clean up before propagating —
            // the error message tells the operator to delete the partial archive, so we do it.
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

    // Verify BEFORE rename so a verification failure leaves nothing in
    // the final cache path. Promoting the tmp file to `dest` first and
    // then bailing on verify failure would leave an executable-bit-set
    // file at the cache location — the next `ensure_available` would
    // see `is_executable_file == true` and reuse the wrong-version
    // binary forever.
    //
    // Verification shape depends on host OS and version channel:
    //   * Linux + stable: exec --version, require exact match.
    //   * Linux + dev/preview: exec --version, require only the
    //     ASSET_PREFIX identity marker (preview rolls forward
    //     independently of the operator's HEAD; an exact match would
    //     permanently reject every dev clone).
    //   * Non-Linux: cannot exec a Linux ELF. Scan the file's bytes
    //     for the same identity marker (and the version string, when
    //     stable) since both are baked in via env! and appear as
    //     contiguous ASCII runs.
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

    jackin_diagnostics::debug_log!(
        "capsule_binary",
        "jackin-capsule {version} cached at {} (sha256 {})",
        dest.display(),
        &actual_sha[..16.min(actual_sha.len())]
    );
    Ok(())
}

/// A `-dev` or `-preview.` version downloads from the rolling `preview` release
/// tag; any other version downloads from the matching `v<version>` tag.
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

// Sigstore production Rekor transparency log public key (ECDSA P-256, SPKI DER, base64-encoded).
// Source: trust_root/prod/trusted_root.json in sigstore-rs v0.14, logId wNI9atQG...
// Used to verify Signed Entry Timestamps in cosign bundles without a TUF network call.
// Update when Sigstore rotates the Rekor key (announced at blog.sigstore.dev).
const SIGSTORE_REKOR_PUB_KEY_B64: &str = "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE2G2Y+2tabdTV5BcGiBIx0a9fAFwrkBbm\
     LSGtks4L3qX6yYY0zufBnhC8Ur/iy55GhWP/9A/bY2LhC30M9+RYtw==";
const SIGSTORE_REKOR_KEY_ID: &str = "wNI9atQGlz+VWfO6LRygH4QUfY/8W4RFwiT5i5WRgB0=";

/// The embedded Rekor key decoded into a verification key, keyed by its log ID.
///
/// Decoded once and cached: the key is a compile-time constant we own, so a
/// decode failure is a programming error — hence `expect` rather than a
/// propagated error. The `rekor_keys_decode_and_contain_expected_id` unit test
/// is the regression guard; a malformed constant panics there at test time
/// rather than silently at first production download.
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

/// The verified `capsule-manifest.json` payload: version string and per-Linux-target SHA-256 digests.
#[derive(serde::Deserialize)]
struct CapsuleManifest {
    version: String,
    targets: std::collections::HashMap<String, String>,
}

/// Fetch the signed `capsule-manifest.json`, verify its cosign bundle, and return
/// the attested SHA-256 hex for the given arch target.
///
/// Verification chain:
/// 1. Fetch manifest JSON + cosign bundle concurrently.
/// 2. Verify the Rekor Signed Entry Timestamp against the embedded production
///    Rekor key via `SignedArtifactBundle::new_verified` (no TUF network call).
/// 3. Cross-check the Rekor payload body covers these exact manifest bytes, this
///    cert, and this signature — closing the field-swap window where an attacker
///    keeps a real Rekor SET but substitutes a self-signed cert (`verify_rekor_body_binds_bundle`).
/// 4. Verify the blob signature via `Client::verify_blob`.
/// 5. Require the certificate SAN to be the `release.yml` or `preview.yml`
///    signing workflow in `jackin-project/jackin`.
/// 6. Validate the manifest `version` field: assert equality for stable releases;
///    log for preview (host and capsule build versions legitimately differ on the
///    rolling preview channel).
/// 7. Parse the verified manifest JSON and extract the SHA256 for `arch`.
///
// Hashedrekord body field subset for the Rekor body binding check.
// Unknown keys are ignored for forward-compatibility with Rekor spec changes.
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
    /// Base64-encoded PEM certificate (`publicKey.content` in hashedrekord).
    public_key: RekorPublicKey,
}
/// Wrapper so serde can deserialise `{"content": "..."}` into a plain `String`.
#[derive(serde::Deserialize)]
struct RekorPublicKey {
    content: String,
}

/// Decode the Rekor `payload.body` (base64 hashedrekord JSON) and assert:
/// 1. `spec.data.hash.value` == `sha256(manifest_bytes)` — the SET covers these bytes.
/// 2. `spec.signature.publicKey.content` == `bundle.cert` — the SET covers this cert.
/// 3. `spec.signature.content` == `bundle.base64_signature` — the SET covers this sig.
///
/// `SignedArtifactBundle::new_verified` only verifies the Signed Entry Timestamp over
/// the canonical Payload JSON, treating `payload.body` as an opaque string. Without this
/// check, the `cert` and `base64_signature` fields in the bundle are fully attacker-
/// controlled — keeping any real Rekor SET while substituting a self-signed cert passes
/// all downstream `verify_blob` and SAN checks.
fn verify_rekor_body_binds_bundle(
    bundle: &sigstore::cosign::bundle::SignedArtifactBundle,
    manifest_bytes: &[u8],
) -> Result<()> {
    use sha2::{Digest, Sha256};

    // Decode the opaque body field from the Payload.
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

    // 1. Verify the body covers these exact manifest bytes.
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

    // 2. Verify the body covers the same certificate presented in bundle.cert.
    // The body is authenticated by the Rekor SET; bundle.cert is attacker-supplied.
    if body.spec.signature.public_key.content != bundle.cert {
        return Err(ImageError::msg(
            "Rekor log entry covers a different certificate than the one in bundle.cert; \
             bundle.cert may have been substituted after the log entry was created",
        )
        .into());
    }

    // 3. Verify the body covers the same signature presented in bundle.base64_signature.
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

/// Failure is a hard abort — no warn-and-continue fallback.
async fn fetch_and_verify_manifest(
    version: &str,
    base_url: &str,
    arch: &str,
    is_preview: bool,
) -> Result<String> {
    // CosignCapabilities is the trait that defines verify_blob; must be in scope.
    use sigstore::cosign::{Client, CosignCapabilities};

    let manifest_url = format!("{base_url}/capsule-manifest.json");
    let bundle_url = format!("{base_url}/capsule-manifest.json.bundle");

    jackin_diagnostics::debug_log!(
        "capsule_binary",
        "fetching signed capsule manifest for jackin-capsule {version} linux/{arch}"
    );

    let (manifest_result, bundle_result) = tokio::join!(
        jackin_docker::net::fetch_text(&manifest_url),
        jackin_docker::net::fetch_text(&bundle_url),
    );

    let manifest_text =
        manifest_result.with_context(|| format!("fetching capsule manifest at {manifest_url}"))?;
    let bundle_text = bundle_result
        .with_context(|| format!("fetching capsule manifest bundle at {bundle_url}"))?;

    // Verify the Rekor Signed Entry Timestamp — proves the bundle was logged at signing time.
    let manifest_bytes = manifest_text.as_bytes();

    let bundle = verified_signed_artifact_bundle(&bundle_text)
        .context("verifying Rekor log entry in capsule manifest bundle")?;

    // Cross-check: the Rekor payload body must cover these exact manifest bytes and this
    // exact certificate. `new_verified` only validates the SET signature over the Payload
    // struct — it treats `payload.body` as an opaque string and never compares it to the
    // top-level `cert` / `base64_signature` fields. Without this check, an attacker who
    // can replace the .bundle file could keep any legitimate Rekor SET, substitute their
    // own self-signed cert (which verify_blob accepts without CA validation), and sign the
    // malicious manifest with it — passing all three subsequent checks.
    verify_rekor_body_binds_bundle(&bundle, manifest_bytes).context(
        "Rekor log entry does not cover the certificate or manifest hash in this bundle",
    )?;

    // The cert field in the bundle is base64-encoded PEM.
    let cert_pem = String::from_utf8(
        BASE64
            .decode(&bundle.cert)
            .context("base64-decoding certificate from capsule manifest bundle")?,
    )
    .context("certificate in bundle is not valid UTF-8")?;

    // Verify the blob signature against the certificate's public key.
    // `manifest_bytes` must be the exact bytes the CI step signed — do NOT
    // parse-then-re-serialize `manifest_text` before this call.
    Client::verify_blob(&cert_pem, &bundle.base64_signature, manifest_bytes)
        .with_context(|| {
            format!(
                "jackin-capsule manifest signature verification failed for {manifest_url}\n\
                 expected signer: ^https://github.com/jackin-project/jackin/\n\
                 refusing to use unverified capsule binary; investigate release tampering and retry.\n\
                 If you built the binary locally, set JACKIN_CAPSULE_BIN=/path/to/jackin-capsule instead."
            )
        })?;

    // Confirm the certificate SAN is exactly one of the two signing workflows
    // (release.yml for tagged releases, preview.yml for rolling main builds).
    // The Rekor SET above proves the bundle was logged; verify_blob above proves
    // the cert's key signed these exact bytes; this SAN check pins the signer
    // identity to the specific workflow files, preventing any other workflow in
    // the repo from producing a valid manifest bundle.
    let san = extract_cert_san_url(&cert_pem)?;
    if !is_allowed_signer_san(&san) {
        return Err(ImageError::msg(format!(
            "capsule manifest signed by unexpected signer {san:?}; \
             expected release.yml or preview.yml workflow in jackin-project/jackin"
        ))
        .into());
    }

    jackin_diagnostics::debug_log!(
        "capsule_binary",
        "capsule manifest signature verified for {version} linux/{arch}: signer = {san}"
    );

    // Parse the verified manifest, bind its version, and return the SHA256 for this arch.
    let manifest: CapsuleManifest =
        serde_json::from_str(&manifest_text).context("parsing verified capsule-manifest.json")?;

    // For stable releases the manifest URL already embeds the version, but validating
    // the signed field closes a downgrade window on the preview channel (one rolling
    // tag, manifest always overwritten). For preview the host version is a -dev or
    // -preview. build and may legitimately differ from the capsule build version, so
    // we log it rather than assert equality.
    if is_preview {
        jackin_diagnostics::debug_log!(
            "capsule_binary",
            "signed capsule manifest version: {} (host version: {version})",
            manifest.version
        );
    } else if manifest.version != version {
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

/// Extract the first URI Subject Alternative Name from a PEM-encoded X.509 certificate.
///
/// Fulcio issues certificates with the OIDC subject as a URI SAN for keyless signing.
/// Uses `x509-cert` to parse the `SubjectAltName` extension by OID rather than scanning
/// DER bytes — prevents bypass via URL-like strings in other certificate fields.
fn extract_cert_san_url(cert_pem: &str) -> Result<String> {
    use x509_cert::Certificate;
    use x509_cert::der::DecodePem;
    use x509_cert::ext::pkix::SubjectAltName;
    use x509_cert::ext::pkix::name::GeneralName;

    let cert = Certificate::from_pem(cert_pem.as_bytes()).context("parsing PEM certificate")?;

    for result in cert.tbs_certificate.filter::<SubjectAltName>() {
        let (_, san) = result.context("parsing SubjectAltName extension")?;
        for name in &san.0 {
            if let GeneralName::UniformResourceIdentifier(uri) = name {
                return Ok(uri.to_string());
            }
        }
    }
    Err(ImageError::NoUriSan.into())
}

/// Return true if `san` is the OIDC identity of one of the two permitted signing
/// workflows (`release.yml` for tagged releases, `preview.yml` for rolling main
/// builds). The `@<ref>` suffix is required by GitHub Actions OIDC; the
/// `starts_with` check accepts any ref so the check survives branch/tag renames.
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

/// Write a placeholder file at `<cache_dir>/jackin-capsule-test-stub`
/// with the executable bit set.
///
/// The `ensure_available` lookup honours this path when present,
/// short-circuiting the network download for integration tests that
/// use `FakeDockerClient` and never actually `docker run` the produced
/// image. Lib-tests (`cfg!(test)`) call this implicitly; integration
/// tests in `tests/` opt in via
/// `tests/common::install_capsule_binary_stub`.
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

/// Format stdout/stderr streams from a process exit for an error message.
/// Returns a human-readable detail block; falls back to a signal-crash hint when
/// both streams are empty.
// Only called from the Linux `verify_version` exec path; on macOS/Windows the
// sole non-test call site is compiled out while unit tests still exercise it.
#[cfg_attr(
    all(not(target_os = "linux"), not(test)),
    expect(dead_code, reason = "Linux-only verify_version exec detail formatter")
)]
fn format_exit_detail(stdout: &str, stderr: &str) -> String {
    let streams: Vec<String> = [("stdout", stdout.trim()), ("stderr", stderr.trim())]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    if streams.is_empty() {
        "(no output — possible signal/crash)".to_owned()
    } else {
        streams.join("\n")
    }
}

/// Verify the downloaded binary is a jackin-capsule of the expected
/// version. Strict matching is only meaningful for stable releases —
/// dev/preview builds share a single rolling `preview` tag whose SHA
/// drifts independently of any individual operator's HEAD, so a
/// SHA-derived exact-version check would reject every dev clone.
///
/// Strategy:
///   * Linux: exec `--version`, then either substring-match the
///     full version (stable) or accept any output that names
///     `jackin-capsule` (preview).
///   * Non-Linux: scan the binary's bytes for the same markers since
///     the Linux ELF cannot be executed on macOS/Windows.
// Non-Linux has no `.await` (the exec path is Linux-only); the signature stays
// async for caller symmetry and the real Linux build.
#[cfg_attr(not(target_os = "linux"), allow(clippy::unused_async))]
async fn verify_version(binary: &Path, expected: &str, is_preview: bool) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let output = tokio::process::Command::new(binary)
            .arg("--version")
            .output()
            .await
            .context("failed to run jackin-capsule --version")?;
        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = format_exit_detail(&stdout, &stderr);
            return Err(ImageError::msg(format!(
                "jackin-capsule --version at {} exited with {}\n{detail}\n\
                 If the binary is corrupted, delete it and retry: rm -f {}",
                binary.display(),
                output.status,
                binary.display()
            ))
            .into());
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if is_preview {
            if !stdout.contains(ASSET_PREFIX) {
                return Err(ImageError::msg(format!(
                    "downloaded binary does not identify as {ASSET_PREFIX} (got {stdout:?})"
                ))
                .into());
            }
            return Ok(());
        }
        if !stdout.contains(expected) {
            return Err(ImageError::msg(format!(
                "downloaded jackin-capsule reports {:?} but expected {expected}.\n\
                 Stable release ↔ asset mapping appears to have drifted.\n\
                 Delete and retry: rm -f {}",
                stdout.trim(),
                binary.display()
            ))
            .into());
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Cannot exec a Linux ELF on macOS / Windows. Scan the file
        // contents for the same identity markers the Linux exec check
        // verifies; the version + asset prefix strings are baked in
        // via env! and appear as contiguous ASCII runs.
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

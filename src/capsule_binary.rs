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
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::binary_artifact::{
    chmod_executable, container_arch, extract_tar_gz_member, hash_file_sha256, is_executable_file,
    parse_sha256_hex,
};
use crate::paths::JackinPaths;

pub const REQUIRED_VERSION: &str = env!("JACKIN_VERSION");

const ASSET_PREFIX: &str = "jackin-capsule";

/// Ensure the `jackin-capsule` binary is available and return its path.
pub async fn ensure_available(paths: &JackinPaths) -> Result<PathBuf> {
    // Explicit override: operator built the binary themselves and told us where it is.
    if let Some(bin_os) = std::env::var_os("JACKIN_CAPSULE_BIN") {
        let path = PathBuf::from(bin_os);
        anyhow::ensure!(
            is_executable_file(&path),
            "JACKIN_CAPSULE_BIN={} does not exist or is not executable",
            path.display()
        );
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
        crate::debug_log!(
            "capsule_binary",
            "JACKIN_CAPSULE_BIN override at {} (skipping SHA-256 verification)",
            path.display()
        );
        return Ok(path);
    }

    // Tests stub the binary by writing a placeholder file at this
    // well-known location (see `install_test_stub`). Used by both lib
    // tests via `cfg!(test)` and integration tests via the helper
    // call; production hosts never have this file because the cache
    // dir lives under `~/.jackin/cache/` and gets the real binary on
    // first run. `cfg!(test)` short-circuits the stub write for lib
    // tests so they don't need any per-test setup.
    let stub_path = paths.cache_dir.join("jackin-capsule-test-stub");
    if cfg!(test) {
        install_test_stub(paths).context("installing in-process test stub")?;
        return Ok(stub_path);
    }
    if stub_path.exists() && is_executable_file(&stub_path) {
        return Ok(stub_path);
    }

    let arch = container_arch();
    let cached = cached_binary_path(&paths.cache_dir, REQUIRED_VERSION, arch);

    if is_executable_file(&cached) {
        crate::debug_log!(
            "capsule_binary",
            "cache hit for jackin-capsule {REQUIRED_VERSION} linux/{arch}"
        );
        return Ok(cached);
    }

    if let Some(packaged) = packaged_binary_path(REQUIRED_VERSION, arch).await {
        crate::debug_log!(
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

async fn packaged_binary_path(version: &str, arch: &str) -> Option<PathBuf> {
    let is_preview = is_preview_version(version);
    for candidate in packaged_binary_candidates(arch) {
        if !is_executable_file(&candidate) {
            continue;
        }
        match verify_version(&candidate, version, is_preview).await {
            Ok(()) => return Some(candidate),
            Err(err) => crate::debug_log!(
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

async fn download_and_cache(version: &str, arch: &str, dest: &Path) -> Result<()> {
    let url = download_url(version, arch);
    let base_url = base_download_url(version);
    crate::debug_log!(
        "capsule_binary",
        "downloading jackin-capsule {version} for linux/{arch}"
    );
    let tmp_archive = dest.with_extension("tar.gz.tmp");
    let tmp = dest.with_extension("tmp");

    // Fetch the signed capsule manifest (verifies cosign bundle + identity) and
    // download the archive concurrently. The manifest returns the expected SHA-256
    // for this arch, replacing the bare .sha256 file fetch.
    let (expected_sha_result, download_result) = tokio::join!(
        fetch_and_verify_manifest(version, &base_url, arch),
        crate::net::download_parallel(&url, &tmp_archive),
    );

    // Either failure must remove the partial archive so a retry starts clean —
    // the manifest fetch and the download run concurrently, so a manifest error can land
    // with the archive already fully written.
    if let Err(e) = download_result {
        let _ = std::fs::remove_file(&tmp_archive);
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
            let _ = std::fs::remove_file(&tmp_archive);
            return Err(e);
        }
    };

    // Verify the published SHA-256. The CI pipeline emits `<asset>.sha256`
    // alongside every archive; a mismatch means a tampered or partial release.
    // Hashing a multi-MB archive parks the tokio worker; run it on the
    // blocking pool so concurrent launch / TUI tasks keep progressing.
    let archive_for_hash = tmp_archive.clone();
    let actual_sha = tokio::task::spawn_blocking(move || hash_file_sha256(&archive_for_hash))
        .await
        .context("hash worker join")?
        .with_context(|| {
            format!(
                "hashing downloaded jackin-capsule archive at {}",
                tmp_archive.display()
            )
        })?;
    if !actual_sha.eq_ignore_ascii_case(&expected_sha) {
        let _ = std::fs::remove_file(&tmp_archive);
        anyhow::bail!(
            "jackin-capsule SHA-256 mismatch for {url}\n  expected {expected_sha}\n  actual   {actual_sha}\n\
             refusing to cache the binary; investigate network tampering and retry."
        );
    }

    extract_tar_gz_member(&tmp_archive, "jackin-capsule", &tmp)
        .with_context(|| format!("extracting jackin-capsule from {}", tmp_archive.display()))?;
    let _ = std::fs::remove_file(&tmp_archive);

    chmod_executable(&tmp).with_context(|| {
        format!(
            "setting executable bit on cached jackin-capsule at {}",
            tmp.display()
        )
    })?;

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
    let is_preview = is_preview_version(version);
    if let Err(e) = verify_version(&tmp, version, is_preview).await {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, dest)
        .with_context(|| format!("failed to move jackin-capsule to {}", dest.display()))?;

    crate::debug_log!(
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
        "https://github.com/jackin-project/jackin/releases/download/preview".to_string()
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
/// Decoded once and cached: the key is a compile-time constant, so a decode
/// failure is a build-time mistake, not a runtime condition — hence `expect`
/// rather than a propagated error.
fn rekor_verification_keys()
-> &'static std::collections::BTreeMap<String, sigstore::crypto::CosignVerificationKey> {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use sigstore::crypto::CosignVerificationKey;

    static KEYS: std::sync::OnceLock<std::collections::BTreeMap<String, CosignVerificationKey>> =
        std::sync::OnceLock::new();
    KEYS.get_or_init(|| {
        let der = BASE64
            .decode(SIGSTORE_REKOR_PUB_KEY_B64)
            .expect("embedded Sigstore Rekor key is valid base64");
        let key = CosignVerificationKey::try_from_der(&der)
            .expect("embedded Sigstore Rekor key is a valid SPKI DER key");
        std::collections::BTreeMap::from([(SIGSTORE_REKOR_KEY_ID.to_string(), key)])
    })
}

/// The verified `capsule-manifest.json` payload: per-Linux-target SHA-256 digests.
#[derive(serde::Deserialize)]
struct CapsuleManifest {
    targets: std::collections::HashMap<String, String>,
}

/// Fetch the signed `capsule-manifest.json`, verify its cosign bundle, and return
/// the attested SHA-256 hex for the given arch target.
///
/// Verification chain:
/// 1. Fetch manifest JSON + cosign bundle concurrently.
/// 2. Verify the Rekor Signed Entry Timestamp against the embedded production
///    Rekor key via `SignedArtifactBundle::new_verified` (no TUF network call).
/// 3. Verify the blob signature via `Client::verify_blob`.
/// 4. Require the certificate SAN to be the `release.yml` or `preview.yml`
///    signing workflow in `jackin-project/jackin`.
/// 5. Parse the verified manifest JSON and extract the SHA256 for `arch`.
///
/// Failure is a hard abort — no warn-and-continue fallback.
async fn fetch_and_verify_manifest(version: &str, base_url: &str, arch: &str) -> Result<String> {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use sigstore::cosign::bundle::SignedArtifactBundle;
    use sigstore::cosign::{Client, CosignCapabilities};

    let manifest_url = format!("{base_url}/capsule-manifest.json");
    let bundle_url = format!("{base_url}/capsule-manifest.json.bundle");

    crate::debug_log!(
        "capsule_binary",
        "fetching signed capsule manifest for jackin-capsule {version} linux/{arch}"
    );

    let (manifest_result, bundle_result) = tokio::join!(
        crate::net::fetch_text(&manifest_url),
        crate::net::fetch_text(&bundle_url),
    );

    let manifest_text =
        manifest_result.with_context(|| format!("fetching capsule manifest at {manifest_url}"))?;
    let bundle_text = bundle_result
        .with_context(|| format!("fetching capsule manifest bundle at {bundle_url}"))?;

    // Verify Rekor Signed Entry Timestamp against the embedded production Rekor key.
    // `manifest_text.as_bytes()` is the exact byte sequence the CI step signed — do
    // NOT parse-then-re-serialize before this call.
    let manifest_bytes = manifest_text.as_bytes();

    let bundle = SignedArtifactBundle::new_verified(&bundle_text, rekor_verification_keys())
        .context("verifying Rekor log entry in capsule manifest bundle")?;

    // The cert field in the bundle is base64-encoded PEM.
    let cert_pem = String::from_utf8(
        BASE64
            .decode(&bundle.cert)
            .context("base64-decoding certificate from capsule manifest bundle")?,
    )
    .context("certificate in bundle is not valid UTF-8")?;

    // Verify blob signature against the certificate's public key.
    Client::verify_blob(&cert_pem, &bundle.base64_signature, manifest_bytes)
        .with_context(|| {
            format!(
                "jackin-capsule manifest signature verification failed for {manifest_url}\n\
                 expected signer: ^https://github.com/jackin-project/jackin/\n\
                 OIDC issuer: https://token.actions.githubusercontent.com\n\
                 refusing to use unverified capsule binary; investigate release tampering and retry.\n\
                 If you built the binary locally, set JACKIN_CAPSULE_BIN=/path/to/jackin-capsule instead."
            )
        })?;

    // Confirm the certificate SAN is exactly one of the two signing workflows
    // (release.yml for tagged releases, preview.yml for rolling main builds).
    // The Rekor SET verification above guarantees the cert was logged at signing time;
    // Rekor enforces Fulcio-issued certs, so the SAN is the OIDC identity Fulcio issued
    // to. Tightening to the specific workflow files prevents any other workflow in the
    // repo from producing a valid manifest bundle.
    let san = extract_cert_san_url(&cert_pem)?;
    anyhow::ensure!(
        san.starts_with("https://github.com/jackin-project/jackin/.github/workflows/release.yml@")
            || san.starts_with(
                "https://github.com/jackin-project/jackin/.github/workflows/preview.yml@"
            ),
        "capsule manifest signed by unexpected signer {san:?}; \
         expected release.yml or preview.yml workflow in jackin-project/jackin"
    );

    crate::debug_log!(
        "capsule_binary",
        "capsule manifest signature verified for {version} linux/{arch}: signer = {san}"
    );

    // Parse the verified manifest and return the SHA256 for this arch.
    let manifest: CapsuleManifest =
        serde_json::from_str(&manifest_text).context("parsing verified capsule-manifest.json")?;

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
    anyhow::bail!("no URI SAN found in Fulcio certificate")
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
async fn verify_version(binary: &Path, expected: &str, is_preview: bool) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let output = tokio::process::Command::new(binary)
            .arg("--version")
            .output()
            .await
            .context("failed to run jackin-capsule --version")?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if is_preview {
            if !stdout.contains(ASSET_PREFIX) {
                anyhow::bail!(
                    "downloaded binary does not identify as {ASSET_PREFIX} (got {stdout:?})"
                );
            }
            return Ok(());
        }
        if !stdout.contains(expected) {
            anyhow::bail!(
                "downloaded jackin-capsule reports {:?} but expected {expected}.\n\
                 Stable release ↔ asset mapping appears to have drifted.\n\
                 Delete and retry: rm -f {}",
                stdout.trim(),
                binary.display()
            );
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
            anyhow::bail!(
                "downloaded binary at {} does not contain the {ASSET_PREFIX} identity marker",
                binary.display()
            );
        }
        if !is_preview && !contains_subslice(&bytes, expected.as_bytes()) {
            anyhow::bail!(
                "downloaded binary at {} does not contain expected version {expected}.\n\
                 Stable release ↔ asset mapping appears to have drifted.\n\
                 Delete and retry: rm -f {}",
                binary.display(),
                binary.display()
            );
        }
        let _ = expected;
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
mod tests {
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
}

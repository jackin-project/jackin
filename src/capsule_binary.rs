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
/// **Dev or preview version** (`-dev` or `-preview.` suffix, cache miss):
///   Download from the rolling `preview` GitHub Release tag.
///
/// **Stable release** (no `-dev`, no `-preview`, cache miss):
///   Download from the versioned `v<version>` GitHub Release tag.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::paths::JackinPaths;

pub const REQUIRED_VERSION: &str = env!("JACKIN_VERSION");

const ASSET_PREFIX: &str = "jackin-capsule";

/// Ensure the `jackin-capsule` binary is available and return its path.
pub async fn ensure_available(paths: &JackinPaths) -> Result<PathBuf> {
    // Explicit override: operator built the binary themselves and told us where it is.
    if let Some(bin_os) = std::env::var_os("JACKIN_CAPSULE_BIN") {
        let path = PathBuf::from(bin_os);
        anyhow::ensure!(
            is_valid_cached_binary(&path),
            "JACKIN_CAPSULE_BIN={} does not exist or is not executable",
            path.display()
        );
        crate::debug_log!(
            "capsule_binary",
            "JACKIN_CAPSULE_BIN override: {}",
            path.display()
        );
        // Operator-trust note: this override path bypasses the
        // SHA-256 verification that the cache-miss download path
        // applies. The operator pointing at a local file is
        // explicitly opting in (typically a `cargo run --bin
        // build-jackin-capsule` artifact, the path that test
        // suites and dev iteration use). Production hosts that
        // never set this env var still get the strong checksum
        // gate from `download_and_cache`.
        eprintln!(
            "[jackin] using JACKIN_CAPSULE_BIN override at {} (skipping SHA-256 verification)",
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
    if stub_path.exists() && is_valid_cached_binary(&stub_path) {
        return Ok(stub_path);
    }

    let arch = container_arch();
    let cached = cached_binary_path(&paths.cache_dir, REQUIRED_VERSION, arch);

    if is_valid_cached_binary(&cached) {
        crate::debug_log!(
            "capsule_binary",
            "cache hit for jackin-capsule {REQUIRED_VERSION} linux/{arch}"
        );
        return Ok(cached);
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

/// Linux arch for the container target, derived from the host machine arch.
pub const fn container_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    }
}

async fn download_and_cache(version: &str, arch: &str, dest: &Path) -> Result<()> {
    let url = download_url(version, arch);
    let sha_url = format!("{url}.sha256");
    eprintln!("[jackin] downloading jackin-capsule {version} for linux/{arch}...");

    let tmp = dest.with_extension("tmp");
    let tmp_path_str = tmp.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "cache temp path {} contains non-UTF-8 bytes; cannot pass to curl",
            tmp.display()
        )
    })?;
    let status = tokio::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--output",
            tmp_path_str,
            &url,
        ])
        .status()
        .await
        .context("failed to run curl to download jackin-capsule")?;

    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        anyhow::bail!(
            "jackin-capsule {version} not found in GitHub Releases.\n\
             \n\
             Developing locally? Build and cache it first:\n\
               cargo run --bin build-jackin-capsule\n\
             Then retry `jackin load`.\n\
             \n\
             Using an installed (Homebrew) jackin? The CI preview build may not\n\
             have completed yet. Wait a few minutes and retry, or check:\n\
               https://github.com/jackin-project/jackin/releases/tag/preview"
        );
    }

    // Fetch and verify the published SHA-256. The preview/release CI
    // pipeline emits `<asset>.sha256` alongside every binary asset; if
    // that companion file is missing or doesn't match the downloaded
    // bytes the binary may have come from a tampered or partial
    // release and we must refuse to cache it.
    let expected_sha = fetch_remote_sha256(&sha_url)
        .await
        .with_context(|| format!("fetching jackin-capsule SHA-256 manifest at {sha_url}"))?;
    // Hashing a multi-MB binary parks the tokio worker; run it on the
    // blocking pool so concurrent launch / TUI tasks keep progressing.
    let tmp_for_hash = tmp.clone();
    let actual_sha = tokio::task::spawn_blocking(move || hash_file_sha256(&tmp_for_hash))
        .await
        .context("hash worker join")?
        .with_context(|| format!("hashing downloaded jackin-capsule at {}", tmp.display()))?;
    if !actual_sha.eq_ignore_ascii_case(&expected_sha) {
        let _ = std::fs::remove_file(&tmp);
        anyhow::bail!(
            "jackin-capsule SHA-256 mismatch for {url}\n  expected {expected_sha}\n  actual   {actual_sha}\n\
             refusing to cache the binary; investigate network tampering and retry."
        );
    }

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
    // see `is_valid_cached_binary == true` and reuse the wrong-version
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
    let is_preview = version.contains("-dev") || version.contains("-preview.");
    if let Err(e) = verify_version(&tmp, version, is_preview).await {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, dest)
        .with_context(|| format!("failed to move jackin-capsule to {}", dest.display()))?;

    eprintln!(
        "[jackin] jackin-capsule {version} cached at {} (sha256 {})",
        dest.display(),
        &actual_sha[..16.min(actual_sha.len())]
    );
    Ok(())
}

/// Fetch the published SHA-256 hex string for a release asset. The CI
/// workflow emits the file as one line of lowercase hex (no filename
/// suffix) so trim + lowercase is enough.
async fn fetch_remote_sha256(url: &str) -> Result<String> {
    let output = tokio::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "30",
            url,
        ])
        .output()
        .await
        .context("failed to run curl for sha256 manifest")?;
    if !output.status.success() {
        anyhow::bail!(
            "{url} download failed (status={}, stderr={})",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8(output.stdout).context("sha256 manifest body is not UTF-8")?;
    let hex = text.split_whitespace().next().unwrap_or("").to_lowercase();
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!(
            "{url} did not return a 64-char hex sha256 (got {:?})",
            hex.chars().take(80).collect::<String>()
        );
    }
    Ok(hex)
}

/// SHA-256 of a file, returned as lowercase hex.
fn hash_file_sha256(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    use std::io::Read as _;
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("opening {} for hashing", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("reading {} for hashing", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for byte in &digest {
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

fn download_url(version: &str, arch: &str) -> String {
    let target = linux_target(arch);
    let asset = format!("{ASSET_PREFIX}-{target}");
    if version.contains("-dev") || version.contains("-preview.") {
        format!("https://github.com/jackin-project/jackin/releases/download/preview/{asset}")
    } else {
        format!("https://github.com/jackin-project/jackin/releases/download/v{version}/{asset}")
    }
}

fn linux_target(arch: &str) -> &'static str {
    match arch {
        "arm64" => "aarch64-unknown-linux-gnu",
        _ => "x86_64-unknown-linux-gnu",
    }
}

pub fn is_valid_cached_binary(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.is_file()
        && path
            .metadata()
            .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
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

pub fn chmod_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let meta =
        std::fs::metadata(path).with_context(|| format!("stating {} for chmod", path.display()))?;
    let mut perms = meta.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("chmod 0755 on {}", path.display()))
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
        assert!(url.contains("x86_64-unknown-linux-gnu"), "{url}");
    }

    #[test]
    fn download_url_stable_uses_version_tag() {
        let url = download_url("0.6.0", "amd64");
        assert!(url.contains("/releases/download/v0.6.0/"), "{url}");
        assert!(url.contains("x86_64-unknown-linux-gnu"), "{url}");
    }

    #[test]
    fn download_url_arm64_uses_aarch64_target() {
        let url = download_url("0.6.0-dev+bf7df07", "arm64");
        assert!(url.contains("aarch64-unknown-linux-gnu"), "{url}");
    }

    #[test]
    fn download_url_preview_uses_preview_tag() {
        let url = download_url("0.6.0-preview.411+bf7df07", "amd64");
        assert!(url.contains("/releases/download/preview/"), "{url}");
    }

    #[test]
    fn cached_path_replaces_plus_in_version() {
        let path = cached_binary_path(Path::new("/cache"), "0.6.0-dev+bf7df07", "amd64");
        let s = path.to_string_lossy();
        assert!(s.contains("0.6.0-dev_bf7df07"), "{s}");
        assert!(!s.contains('+'), "{s}");
    }

    #[test]
    fn linux_target_maps_arch() {
        assert_eq!(linux_target("arm64"), "aarch64-unknown-linux-gnu");
        assert_eq!(linux_target("amd64"), "x86_64-unknown-linux-gnu");
        assert_eq!(linux_target("x86_64"), "x86_64-unknown-linux-gnu");
    }

    #[test]
    fn hash_file_sha256_matches_known_vector() {
        // SHA-256 of the empty string is the well-known
        // e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let digest = hash_file_sha256(tmp.path()).unwrap();
        assert_eq!(
            digest,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hash_file_sha256_matches_for_known_bytes() {
        // SHA-256 of the ASCII string "abc" is
        // ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"abc").unwrap();
        let digest = hash_file_sha256(tmp.path()).unwrap();
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}

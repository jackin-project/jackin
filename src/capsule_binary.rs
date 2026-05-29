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
    let is_preview = version.contains("-dev") || version.contains("-preview.");
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
    let sha_url = format!("{url}.sha256");
    crate::debug_log!(
        "capsule_binary",
        "downloading jackin-capsule {version} for linux/{arch}"
    );
    let tmp_archive = dest.with_extension("tar.gz.tmp");
    let tmp = dest.with_extension("tmp");

    // Fetch the expected SHA-256 and download the archive concurrently.
    let (expected_sha_result, download_result) = tokio::join!(
        fetch_remote_sha256(&sha_url),
        crate::net::download_parallel(&url, &tmp_archive),
    );

    // Either failure must remove the partial archive so a retry starts clean —
    // the SHA fetch and the download run concurrently, so a SHA error can land
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
            return Err(e)
                .with_context(|| format!("fetching jackin-capsule SHA-256 manifest at {sha_url}"));
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
    let is_preview = version.contains("-dev") || version.contains("-preview.");
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

/// Fetch the published SHA-256 hex string for a release asset. The CI workflow
/// emits one line of hex (optionally `<hex>  <filename>`).
async fn fetch_remote_sha256(url: &str) -> Result<String> {
    let text = crate::net::fetch_text(url).await?;
    parse_sha256_hex(&text).with_context(|| format!("{url} did not return a valid sha256"))
}

fn download_url(version: &str, arch: &str) -> String {
    let target = linux_target(arch);
    if version.contains("-dev") || version.contains("-preview.") {
        let asset = format!("{ASSET_PREFIX}-{target}.tar.gz");
        format!("https://github.com/jackin-project/jackin/releases/download/preview/{asset}")
    } else {
        let asset = format!("{ASSET_PREFIX}-{version}-{target}.tar.gz");
        format!("https://github.com/jackin-project/jackin/releases/download/v{version}/{asset}")
    }
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

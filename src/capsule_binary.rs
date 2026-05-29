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
use fast_down::{
    Event, Proxy,
    fast_puller::{FastDownPuller, FastDownPullerOptions, build_client as build_http_client},
    file::MmapFilePusher,
    http::Prefetch,
    multi::{self, download_multi},
};
use reqwest::header::HeaderMap;
use std::sync::Arc;
use std::time::Duration;

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
        if !is_valid_cached_binary(&candidate) {
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
    crate::debug_log!(
        "capsule_binary",
        "downloading jackin-capsule {version} for linux/{arch}"
    );
    let tmp_archive = dest.with_extension("tar.gz.tmp");
    let tmp = dest.with_extension("tmp");

    // Fetch the expected SHA-256 and download the archive concurrently.
    let (expected_sha_result, download_result) = tokio::join!(
        fetch_remote_sha256(&sha_url),
        download_file_parallel(&url, &tmp_archive),
    );

    let expected_sha = expected_sha_result
        .with_context(|| format!("fetching jackin-capsule SHA-256 manifest at {sha_url}"))?;

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

    // Verify the published SHA-256. The CI pipeline emits `<asset>.sha256`
    // alongside every archive; a mismatch means a tampered or partial release.
    let expected_sha = expected_sha;
    // Hashing a multi-MB archive parks the tokio worker; run it on the
    // blocking pool so concurrent launch / TUI tasks keep progressing.
    let archive_for_hash = tmp_archive.clone();
    let archive_for_context = archive_for_hash.clone();
    let actual_sha = tokio::task::spawn_blocking(move || hash_file_sha256(&archive_for_hash))
        .await
        .context("hash worker join")?
        .with_context(|| {
            format!(
                "hashing downloaded jackin-capsule archive at {}",
                archive_for_context.display()
            )
        })?;
    if !actual_sha.eq_ignore_ascii_case(&expected_sha) {
        let _ = std::fs::remove_file(&tmp_archive);
        anyhow::bail!(
            "jackin-capsule SHA-256 mismatch for {url}\n  expected {expected_sha}\n  actual   {actual_sha}\n\
             refusing to cache the binary; investigate network tampering and retry."
        );
    }

    extract_capsule_from_archive(&tmp_archive, &tmp)
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

    crate::debug_log!(
        "capsule_binary",
        "jackin-capsule {version} cached at {} (sha256 {})",
        dest.display(),
        &actual_sha[..16.min(actual_sha.len())]
    );
    Ok(())
}

/// Download `url` to `dest` using fast-down parallel chunks (work-stealing +
/// mmap writes). All GitHub release CDN endpoints support Range requests.
async fn download_file_parallel(url: &str, dest: &Path) -> Result<()> {
    let parsed = reqwest::Url::parse(url).with_context(|| format!("invalid URL {url}"))?;
    let headers = HeaderMap::new();
    let client = build_http_client(&headers, Proxy::System, false, false, None)
        .context("building HTTP client")?;
    let (info, _resp) = client
        .prefetch(parsed)
        .await
        .map_err(|(err, _)| anyhow::anyhow!("prefetch {url}: {err:?}"))?;
    anyhow::ensure!(
        info.fast_download,
        "server at {url} does not support Range requests; cannot download"
    );
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .truncate(true)
        .open(dest)
        .await
        .with_context(|| format!("creating {}", dest.display()))?;
    let puller = FastDownPuller::new(FastDownPullerOptions {
        url: info.final_url,
        headers: Arc::new(headers),
        proxy: Proxy::System,
        accept_invalid_certs: false,
        accept_invalid_hostnames: false,
        file_id: info.file_id,
        resp: None,
        available_ips: Arc::from([]),
    })
    .context("building parallel downloader")?;
    let pusher = MmapFilePusher::new(file, info.size, false)
        .await
        .context("creating memory-mapped file writer")?;
    let result = download_multi(
        puller,
        pusher,
        multi::DownloadOptions {
            download_chunks: std::iter::once(0..info.size),
            concurrent: 8,
            retry_gap: Duration::from_millis(500),
            push_queue_cap: 1024,
            pull_timeout: Duration::from_secs(30),
            min_chunk_size: 8 * 1024 * 1024,
            max_speculative: 3,
        },
    );
    while let Ok(event) = result.event_chain.recv().await {
        match event {
            Event::PullError(id, err) => {
                crate::debug_log!("capsule_binary", "worker {id} pull error: {err:?}");
            }
            Event::PushError(_, _, err) | Event::FlushError(err) => {
                crate::debug_log!("capsule_binary", "write error: {err}");
            }
            _ => {}
        }
    }
    result
        .join()
        .await
        .map_err(|e| anyhow::anyhow!("download task panicked for {url}: {e}"))
}

/// Fetch the published SHA-256 hex string for a release asset. The CI
/// workflow emits the file as one line of lowercase hex (no filename
/// suffix) so trim + lowercase is enough.
async fn fetch_remote_sha256(url: &str) -> Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    anyhow::ensure!(status.is_success(), "{url} failed: HTTP {status}");
    let text = resp
        .text()
        .await
        .context("sha256 manifest body is not UTF-8")?;
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
    if version.contains("-dev") || version.contains("-preview.") {
        let asset = format!("{ASSET_PREFIX}-{target}.tar.gz");
        format!("https://github.com/jackin-project/jackin/releases/download/preview/{asset}")
    } else {
        let asset = format!("{ASSET_PREFIX}-{version}-{target}.tar.gz");
        format!("https://github.com/jackin-project/jackin/releases/download/v{version}/{asset}")
    }
}

fn extract_capsule_from_archive(archive_path: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(archive_path)
        .with_context(|| format!("opening {}", archive_path.display()))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive
        .entries()
        .with_context(|| format!("reading entries from {}", archive_path.display()))?
    {
        let mut entry =
            entry.with_context(|| format!("reading entry from {}", archive_path.display()))?;
        let is_capsule = entry.path().context("reading archive entry path")?.as_ref()
            == Path::new("jackin-capsule");
        if is_capsule && entry.header().entry_type().is_file() {
            entry
                .unpack(dest)
                .with_context(|| format!("unpacking jackin-capsule to {}", dest.display()))?;
            return Ok(());
        }
    }
    anyhow::bail!(
        "{} does not contain a top-level jackin-capsule binary",
        archive_path.display()
    )
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

    #[test]
    fn extract_capsule_from_archive_writes_top_level_binary() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("jackin-capsule.tar.gz");
        let dest = temp.path().join("jackin-capsule");
        let bytes = b"#!/bin/sh\necho capsule\n";

        let archive_file = std::fs::File::create(&archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(archive_file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        archive
            .append_data(&mut header, "jackin-capsule", &bytes[..])
            .unwrap();
        let encoder = archive.into_inner().unwrap();
        encoder.finish().unwrap();

        extract_capsule_from_archive(&archive_path, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), bytes);
    }
}

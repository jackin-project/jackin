// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::ImageError;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use jackin_core::JackinPaths;

use crate::binary_artifact::{
    chmod_executable, container_arch, hash_file_sha256, is_executable_file, repair_executable_file,
};

pub const REQUIRED_VERSION: &str = env!("JACKIN_VERSION");

const ASSET_PREFIX: &str = "jackin-capsule";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReleaseChannel {
    Preview,
    LocalOnly,
}

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
        verify_version(&path, REQUIRED_VERSION)
            .await
            .context("JACKIN_CAPSULE_BIN version mismatch")?;
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
    let channel = release_channel(REQUIRED_VERSION);
    let cache_version = cache_key_version(REQUIRED_VERSION);
    let cached = resolved_cache_path(&paths.cache_dir);

    if is_executable_file(&cached) && verify_version(&cached, REQUIRED_VERSION).await.is_ok() {
        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::CapsuleBinary,
            jackin_telemetry::schema::enums::CacheResult::Hit,
        );
        return Ok(cached);
    }
    if repair_executable_file(&cached)? && verify_version(&cached, REQUIRED_VERSION).await.is_ok() {
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

    if let Some(packaged) = packaged_binary_path(REQUIRED_VERSION, arch).await {
        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::CapsuleBinary,
            jackin_telemetry::schema::enums::CacheResult::Reuse,
        );
        return Ok(packaged);
    }

    if channel == ReleaseChannel::LocalOnly {
        return Err(ImageError::msg(local_capsule_only_message(REQUIRED_VERSION)).into());
    }

    if let Some(parent) = cached.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache dir {}", parent.display()))?;
    }

    crate::telemetry_boundary::cache_decision(
        jackin_telemetry::schema::enums::CacheName::CapsuleBinary,
        jackin_telemetry::schema::enums::CacheResult::Miss,
    );
    download_and_cache(REQUIRED_VERSION, arch, channel, &cached).await?;
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
    version.replace('+', "_")
}

fn record(kind: &str, message: &str) {
    let _ = message;
    if let Some(run) = jackin_diagnostics::active_run() {
        run.compact(kind, kind);
    }
}

async fn packaged_binary_path(version: &str, arch: &str) -> Option<PathBuf> {
    for candidate in packaged_binary_candidates(arch) {
        if !is_executable_file(&candidate) {
            continue;
        }
        if verify_version(&candidate, version).await.is_ok() {
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

fn download_failure_message(version: &str, url: &str) -> String {
    format!(
        "jackin-capsule {version} download failed.\n\
             \n\
             Attempted URL: {url}\n\
             \n\
             The immutable release asset for jackin-capsule {version} may not\n\
             be published yet. Check the release tag:\n\
               https://github.com/jackin-project/jackin/releases/tag/v{version}\n\
             \n\
             Developing locally? Build and cache it first:\n\
               cargo run --bin build-jackin-capsule\n\
             or point JACKIN_CAPSULE_BIN at a local binary.\n\
             Then retry `jackin load`."
    )
}

async fn download_and_cache(
    version: &str,
    arch: &str,
    channel: ReleaseChannel,
    dest: &Path,
) -> Result<()> {
    let url = download_url(version, arch, channel);
    let tmp_archive = dest.with_extension("tar.gz.tmp");
    let tmp = dest.with_extension("tmp");

    let evidence = crate::release_evidence::verify_release(version, linux_target(arch))
        .await
        .context("authenticating complete capsule release evidence before installation")?;
    let expected_sha = evidence.sha256;
    let download_result = crate::telemetry_boundary::download_request(
        crate::telemetry_boundary::DownloadRoute::CapsuleArtifact,
        &url,
        jackin_docker::net::download_parallel(&url, &tmp_archive, Some(evidence.size)),
    )
    .await;
    if let Err(e) = download_result {
        remove_with_debug_log(&tmp_archive);
        return Err(e).context(download_failure_message(version, &url));
    }
    if std::fs::metadata(&tmp_archive)?.len() != evidence.size {
        remove_with_debug_log(&tmp_archive);
        anyhow::bail!("downloaded capsule archive size differs from authenticated release receipt");
    }

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

    if let Err(e) = archive::extract_capsule(&tmp_archive, &tmp) {
        remove_with_debug_log(&tmp_archive);
        remove_with_debug_log(&tmp);
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

    if let Err(e) = verify_version(&tmp, version).await {
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

fn release_channel(version: &str) -> ReleaseChannel {
    if is_preview_version(version) {
        ReleaseChannel::Preview
    } else {
        ReleaseChannel::LocalOnly
    }
}

fn is_preview_version(version: &str) -> bool {
    crate::release_evidence::preview_base(version).is_ok()
}

fn local_capsule_only_message(version: &str) -> String {
    format!(
        "jackin-capsule version {version:?} is not an immutable preview version. Remote capsule downloads support previews only.\n\
         No release download was attempted. Set JACKIN_CAPSULE_BIN to a local executable,\n\
         or build and cache a matching capsule with `cargo run --bin build-jackin-capsule`."
    )
}

fn download_url(version: &str, arch: &str, channel: ReleaseChannel) -> String {
    debug_assert_eq!(channel, ReleaseChannel::Preview);
    let target = linux_target(arch);
    let asset = format!("{ASSET_PREFIX}-{target}.tar.gz");
    format!("https://github.com/jackin-project/jackin/releases/download/v{version}/{asset}")
}

fn linux_target(arch: &str) -> &'static str {
    match arch {
        "arm64" => "aarch64-unknown-linux-gnu",
        _ => "x86_64-unknown-linux-gnu",
    }
}

pub fn install_test_stub(paths: &JackinPaths) -> Result<()> {
    let stub = resolved_cache_path(&paths.cache_dir);
    if let Some(parent) = stub.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache dir {}", parent.display()))?;
    }
    let script = format!(
        "#!/bin/sh\n[ \"$1\" = \"--version\" ] || exit 2\nprintf '%s\\n' 'jackin-capsule {REQUIRED_VERSION}'\n"
    );
    std::fs::write(&stub, script)
        .with_context(|| format!("writing test stub at {}", stub.display()))?;
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
async fn verify_version(binary: &Path, expected: &str) -> Result<()> {
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
        if stdout.trim() != format!("{ASSET_PREFIX} {expected}") {
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
        if !contains_exact_version(&bytes, expected.as_bytes()) {
            return Err(ImageError::msg(format!(
                "downloaded binary at {} does not contain expected version {expected}.\n\
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

#[cfg(not(target_os = "linux"))]
fn contains_exact_version(haystack: &[u8], version: &[u8]) -> bool {
    !version.is_empty()
        && haystack
            .windows(version.len())
            .enumerate()
            .any(|(index, candidate)| {
                let continues_version =
                    |byte: u8| byte.is_ascii_alphanumeric() || b".+-_".contains(&byte);
                candidate == version
                    && (index == 0 || !continues_version(haystack[index - 1]))
                    && haystack
                        .get(index + version.len())
                        .is_none_or(|byte| !continues_version(*byte))
            })
}

mod archive;

#[cfg(test)]
mod tests;

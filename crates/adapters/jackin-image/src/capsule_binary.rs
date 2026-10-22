// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::ImageError;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use jackin_core::JackinPaths;

use crate::binary_artifact::{chmod_executable, container_arch, is_executable_file};

pub const REQUIRED_VERSION: &str = env!("JACKIN_VERSION");

const ASSET_PREFIX: &str = "jackin-capsule";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReleaseChannel {
    Preview,
    Stable,
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
    let native = origin::compiled(REQUIRED_VERSION);
    let channel = release_channel(REQUIRED_VERSION);
    let cached = resolved_cache_path(&paths.cache_dir);

    if let Some(cached) = cache::lookup(&paths.cache_dir, REQUIRED_VERSION, arch, native).await? {
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
    let native = native.context("native release origin is required for a remote capsule")?;
    download_and_cache(REQUIRED_VERSION, arch, channel, native.source, &cached).await?;
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
    origin::cache_path(
        cache_dir,
        REQUIRED_VERSION,
        container_arch(),
        origin::compiled(REQUIRED_VERSION),
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

fn download_failure_message(version: &str, source: &str, url: &str) -> String {
    let tag = crate::release_evidence::release_tag(version, source)
        .unwrap_or_else(|_| "unavailable".into());
    format!(
        "jackin-capsule {version} download failed.\n\
             \n\
             Attempted URL: {url}\n\
             \n\
             The matching published asset for jackin-capsule {version} may not\n\
             be published yet. Check the release tag:\n\
               https://github.com/jackin-project/jackin/releases/tag/{tag}\n\
             \n\
             Developing locally? Build and cache it first:\n\
               cargo run --bin build-jackin-capsule\n\
             or point JACKIN_CAPSULE_BIN at a local binary.\n\
             Then retry `jackin load`."
    )
}

fn version_channel(version: &str) -> ReleaseChannel {
    match crate::release_evidence::release_channel(version) {
        Ok(velnor_actions_contract::package_release::PackageReleaseChannel::Preview) => {
            ReleaseChannel::Preview
        }
        Ok(velnor_actions_contract::package_release::PackageReleaseChannel::Stable) => {
            ReleaseChannel::Stable
        }
        Err(_) => ReleaseChannel::LocalOnly,
    }
}

fn release_channel(version: &str) -> ReleaseChannel {
    origin::compiled(version).map_or(ReleaseChannel::LocalOnly, |value| value.channel)
}

fn local_capsule_only_message(version: &str) -> String {
    format!(
        "jackin-capsule version {version:?} was not built with an exact native release origin. Remote capsule downloads require an admitted native channel, version and source.\n\
         No release download was attempted. Set JACKIN_CAPSULE_BIN to a local executable,\n\
         or build and cache a matching capsule with `cargo run --bin build-jackin-capsule`."
    )
}

/// Each preview source has its own immutable `preview-<full source SHA>` release.
fn download_url(
    version: &str,
    arch: &str,
    channel: ReleaseChannel,
    source: &str,
) -> Result<String> {
    anyhow::ensure!(
        version_channel(version) == channel && channel != ReleaseChannel::LocalOnly,
        "invalid remote capsule channel/version"
    );
    let target = linux_target(arch);
    let asset = format!("{ASSET_PREFIX}-{target}.tar.gz");
    let tag = match channel {
        ReleaseChannel::Preview | ReleaseChannel::Stable => {
            crate::release_evidence::release_tag(version, source)?
        }
        ReleaseChannel::LocalOnly => anyhow::bail!(local_capsule_only_message(version)),
    };
    Ok(format!(
        "https://github.com/jackin-project/jackin/releases/download/{tag}/{asset}"
    ))
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

// async retained for caller symmetry with the Linux exec path
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
mod cache;
mod download;
mod origin;
use download::download_and_cache;

#[cfg(test)]
mod tests;

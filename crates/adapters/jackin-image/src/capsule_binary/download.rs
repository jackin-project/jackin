// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{
    ImageError, ReleaseChannel, archive, chmod_executable, download_failure_message, download_url,
    linux_target, remove_with_debug_log, verify_version,
};
use crate::binary_artifact::hash_file_sha256;
use anyhow::{Context, Result};
use std::path::Path;

pub(super) async fn download_and_cache(
    version: &str,
    arch: &str,
    channel: ReleaseChannel,
    dest: &Path,
) -> Result<()> {
    let url = download_url(version, arch, channel)?;
    let tmp_archive = dest.with_extension("tar.gz.tmp");
    let tmp = dest.with_extension("tmp");
    let evidence = crate::release_evidence::verify_release(version, linux_target(arch))
        .await
        .context("authenticating complete capsule release evidence before installation")?;
    download_verified_archive(version, &url, &tmp_archive, &evidence).await?;
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

    if let Err(e) = crate::release_evidence::recheck_release(&evidence).await {
        remove_with_debug_log(&tmp);
        return Err(e).context("release changed before capsule execution");
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
    if let Err(e) = crate::release_evidence::recheck_release(&evidence).await {
        remove_with_debug_log(&tmp);
        return Err(e).context("release changed before capsule cache installation");
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

async fn download_verified_archive(
    version: &str,
    url: &str,
    tmp_archive: &Path,
    evidence: &crate::release_evidence::VerifiedCapsule,
) -> Result<()> {
    let expected_sha = &evidence.sha256;
    let download_result = crate::telemetry_boundary::download_request(
        crate::telemetry_boundary::DownloadRoute::CapsuleArtifact,
        url,
        jackin_docker::net::download_parallel(url, tmp_archive, Some(evidence.size)),
    )
    .await;
    if let Err(e) = download_result {
        remove_with_debug_log(tmp_archive);
        return Err(e).context(download_failure_message(version, url));
    }
    if std::fs::metadata(tmp_archive)?.len() != evidence.size {
        remove_with_debug_log(tmp_archive);
        anyhow::bail!("downloaded capsule archive size differs from authenticated release receipt");
    }

    let archive_for_hash = tmp_archive.to_owned();
    let hash_result =
        jackin_telemetry::spawn::joined_blocking(move || hash_file_sha256(&archive_for_hash)).await;
    let actual_sha = match hash_result {
        Err(e) => {
            remove_with_debug_log(tmp_archive);
            return Err(e).with_context(|| {
                format!(
                    "SHA-256 hash worker panicked or was cancelled for {}; \
                     delete the partial archive and retry",
                    tmp_archive.display()
                )
            });
        }
        Ok(Err(e)) => {
            remove_with_debug_log(tmp_archive);
            return Err(e).with_context(|| {
                format!(
                    "hashing downloaded jackin-capsule archive at {}",
                    tmp_archive.display()
                )
            });
        }
        Ok(Ok(sha)) => sha,
    };
    if !actual_sha.eq_ignore_ascii_case(expected_sha) {
        remove_with_debug_log(tmp_archive);
        return Err(ImageError::msg(format!(
            "jackin-capsule SHA-256 mismatch for {url}\n  expected {expected_sha}\n  actual   {actual_sha}\n\
             refusing to cache the binary; investigate network tampering and retry."
        ))
        .into());
    }

    Ok(())
}

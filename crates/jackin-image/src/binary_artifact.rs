// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Post-download artifact helpers shared by `agent_binary` and `capsule_binary`.
//!
//! Both modules fetch a binary (or a `.tar.gz` carrying one), verify its
//! SHA-256, extract it, and set the executable bit. These steps are identical
//! across the two callers, so they live here once rather than as drifting
//! copies. The network transfer itself lives in [`jackin_docker::net`]; this
//! module owns everything that happens to the bytes once they land on disk,
//! plus the host-derived [`container_arch`] both callers key their cache paths
//! on.

use crate::ImageError;
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::io::Read as _;
use std::path::Path;

/// Linux container arch for the current host, used to pick release assets and
/// build cache paths.
pub const fn container_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    }
}

/// Set the executable bit (0o755) on `path`.
///
/// No-op on non-Unix hosts, which cannot represent Unix mode bits — the binary
/// only ever runs inside the Linux container.
#[cfg(unix)]
pub fn chmod_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let meta =
        std::fs::metadata(path).with_context(|| format!("stating {} for chmod", path.display()))?;
    let mut perms = meta.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("chmod 0755 on {}", path.display()))
}

#[cfg(not(unix))]
pub fn chmod_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// True if `path` is a regular file with an executable bit set.
///
/// Every cached binary is [`chmod_executable`]'d after download. On non-Unix
/// hosts — which cannot represent the bit — this falls back to a file-exists
/// check, matching [`chmod_executable`]'s no-op there.
#[cfg(unix)]
pub fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.is_file()
        && path
            .metadata()
            .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
pub fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// Repair a regular cached binary that exists but has lost its executable bit.
///
/// Returns `true` only when the path is a regular file and is executable after
/// the repair. Missing paths and directories return `false` so callers can
/// continue to their normal fallback/download path.
pub fn repair_executable_file(path: &Path) -> Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    chmod_executable(path)?;
    Ok(is_executable_file(path))
}

/// SHA-256 of a file, returned as lowercase hex.
///
/// Synchronous: a multi-MB read parks the calling thread, so callers on an
/// async launch path wrap this in `tokio::task::spawn_blocking`.
/// Encode a SHA-256 digest as a lowercase 64-character hex string.
pub fn sha256_hex(digest: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;
    let bytes = digest.as_ref();
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _unused = write!(hex, "{byte:02x}");
    }
    hex
}

pub fn hash_file_sha256(path: &Path) -> Result<String> {
    #[expect(
        clippy::disallowed_methods,
        reason = "binary artifact hashing is called from image prep/offloaded launch work"
    )]
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
    Ok(sha256_hex(hasher.finalize()))
}

/// Parse the first whitespace-delimited token of a `.sha256` manifest as a
/// lowercase 64-char hex digest, erroring if it isn't one.
///
/// Publishers emit either a bare digest or `<digest>  <filename>`; both reduce
/// to the first token. A blank or malformed line is caught here rather than
/// surfacing later as a confusing "checksum mismatch against empty".
pub fn parse_sha256_hex(text: &str) -> Result<String> {
    let hex = text.split_whitespace().next().unwrap_or("").to_lowercase();
    if !(hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())) {
        return Err(ImageError::InvalidSha256Hex {
            got: hex.chars().take(80).collect(),
        }
        .into());
    }
    Ok(hex)
}

/// Extract the `.tar.gz` `archive`'s file entry whose file name is `member`,
/// writing it to `dest`.
///
/// Mode bits from the archive are applied by `unpack`; callers that need a
/// fixed mode call [`chmod_executable`] afterward.
pub fn extract_tar_gz_member(archive: &Path, member: &str, dest: &Path) -> Result<()> {
    #[expect(
        clippy::disallowed_methods,
        reason = "binary archive extraction is called from image prep/offloaded launch work"
    )]
    let file =
        std::fs::File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    let decoder = GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar
        .entries()
        .with_context(|| format!("reading entries from {}", archive.display()))?
    {
        let mut entry =
            entry.with_context(|| format!("reading entry from {}", archive.display()))?;
        let is_member = entry
            .path()
            .context("reading archive entry path")?
            .file_name()
            .and_then(|name| name.to_str())
            == Some(member);
        if is_member && entry.header().entry_type().is_file() {
            entry
                .unpack(dest)
                .with_context(|| format!("unpacking {member} to {}", dest.display()))?;
            return Ok(());
        }
    }
    Err(ImageError::ArchiveMemberMissing {
        archive: archive.to_path_buf(),
        member: member.to_owned(),
    }
    .into())
}

#[cfg(test)]
mod tests;

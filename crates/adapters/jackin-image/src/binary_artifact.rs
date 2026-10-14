// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::ImageError;
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256, Sha512};
use std::io::Read as _;
use std::path::Path;

pub const fn container_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    }
}

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

pub fn repair_executable_file(path: &Path) -> Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    chmod_executable(path)?;
    Ok(is_executable_file(path))
}

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

pub fn hash_file_sha512(path: &Path) -> Result<String> {
    #[expect(
        clippy::disallowed_methods,
        reason = "binary artifact hashing is called from image prep/offloaded launch work"
    )]
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("opening {} for hashing", path.display()))?;
    let mut hasher = Sha512::new();
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

pub fn parse_sha512_hex(text: &str) -> Result<String> {
    let hex = text.split_whitespace().next().unwrap_or("").to_lowercase();
    if !(hex.len() == 128 && hex.chars().all(|c| c.is_ascii_hexdigit())) {
        return Err(ImageError::InvalidSha512Hex {
            got: hex.chars().take(132).collect(),
        }
        .into());
    }
    Ok(hex)
}

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

//! Post-download artifact helpers shared by `agent_binary` and `capsule_binary`.
//!
//! Both modules fetch a binary (or a `.tar.gz` carrying one), verify its
//! SHA-256, extract it, and set the executable bit, all keyed off the same
//! host-derived container arch. These steps are identical across the two
//! callers, so they live here once rather than as drifting copies. The network
//! transfer itself lives in [`crate::net`]; this module owns everything that
//! happens to the bytes once they land on disk.

use std::path::Path;

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::io::Read as _;

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
/// Every cached binary is [`chmod_executable`]'d after download, so a cached
/// file without the bit is corrupt and must be re-fetched. On non-Unix hosts —
/// which cannot represent the bit — this falls back to a file-exists check,
/// matching [`chmod_executable`]'s no-op there.
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

/// SHA-256 of a file, returned as lowercase hex.
///
/// Synchronous: a multi-MB read parks the calling thread, so callers on an
/// async launch path wrap this in `tokio::task::spawn_blocking`.
pub fn hash_file_sha256(path: &Path) -> Result<String> {
    use std::fmt::Write as _;
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
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

/// Extract the `.tar.gz` `archive`'s file entry whose file name is `member`,
/// writing it to `dest`.
///
/// Mode bits from the archive are applied by `unpack`; callers that need a
/// fixed mode call [`chmod_executable`] afterward.
pub fn extract_tar_gz_member(archive: &Path, member: &str, dest: &Path) -> Result<()> {
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
    anyhow::bail!("{} is missing member {member}", archive.display())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn extract_tar_gz_member_writes_named_entry() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("bundle.tar.gz");
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

        extract_tar_gz_member(&archive_path, "jackin-capsule", &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), bytes);
    }
}

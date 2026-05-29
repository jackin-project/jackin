//! Post-download artifact helpers shared by `agent_binary` and `capsule_binary`.
//!
//! Both modules fetch a binary (or a `.tar.gz` carrying one), verify its
//! SHA-256, extract it, and set the executable bit. These steps are identical
//! across the two callers, so they live here once rather than as drifting
//! copies. The network transfer itself lives in [`crate::net`]; this module
//! owns everything that happens to the bytes once they land on disk, plus the
//! host-derived [`container_arch`] both callers key their cache paths on.

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

/// Parse the first whitespace-delimited token of a `.sha256` manifest as a
/// lowercase 64-char hex digest, erroring if it isn't one.
///
/// Publishers emit either a bare digest or `<digest>  <filename>`; both reduce
/// to the first token. A blank or malformed line is caught here rather than
/// surfacing later as a confusing "checksum mismatch against empty".
pub fn parse_sha256_hex(text: &str) -> Result<String> {
    let hex = text.split_whitespace().next().unwrap_or("").to_lowercase();
    anyhow::ensure!(
        hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()),
        "expected a 64-char hex sha256, got {:?}",
        hex.chars().take(80).collect::<String>()
    );
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

    #[cfg(unix)]
    #[test]
    fn is_executable_file_requires_exec_bit() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();

        let exec = dir.path().join("exec");
        std::fs::write(&exec, b"x").unwrap();
        std::fs::set_permissions(&exec, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_executable_file(&exec), "0o755 file should be executable");

        let plain = dir.path().join("plain");
        std::fs::write(&plain, b"x").unwrap();
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!is_executable_file(&plain), "0o644 file must be rejected");

        assert!(!is_executable_file(dir.path()), "a directory is not a file");
        assert!(!is_executable_file(&dir.path().join("missing")));
    }

    #[test]
    fn parse_sha256_hex_accepts_valid_and_rejects_garbage() {
        let digest = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(parse_sha256_hex(digest).unwrap(), digest);
        // Uppercase is normalized; a trailing filename token is ignored.
        assert_eq!(
            parse_sha256_hex(&format!("{}  some-asset.tar.gz", digest.to_uppercase())).unwrap(),
            digest
        );
        assert!(parse_sha256_hex("").is_err(), "empty");
        assert!(parse_sha256_hex("deadbeef").is_err(), "too short");
        assert!(
            parse_sha256_hex(&"z".repeat(64)).is_err(),
            "64 non-hex chars"
        );
    }
}

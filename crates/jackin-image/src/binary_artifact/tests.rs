//! Tests for `binary_artifact`.
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

#[cfg(unix)]
#[test]
fn repair_executable_file_sets_exec_bit_on_regular_file() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain");
    std::fs::write(&plain, b"x").unwrap();
    std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();

    assert!(repair_executable_file(&plain).unwrap());
    assert!(is_executable_file(&plain));
    assert!(!repair_executable_file(dir.path()).unwrap());
    assert!(!repair_executable_file(&dir.path().join("missing")).unwrap());
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
    parse_sha256_hex("").expect_err("empty");
    parse_sha256_hex("deadbeef").expect_err("too short");
    parse_sha256_hex(&"z".repeat(64)).expect_err("64 non-hex chars");
}

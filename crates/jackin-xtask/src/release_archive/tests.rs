// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn jackin_owns_every_release_target() {
    assert_eq!(targets(ArchivePackage::Jackin), &JACKIN_TARGETS);
    assert_eq!(binaries(ArchivePackage::Jackin), &["jackin", "jackin-role"]);
}

#[test]
fn capsule_owns_complete_linux_target_set() {
    assert_eq!(targets(ArchivePackage::JackinCapsule), &CAPSULE_TARGETS);
    assert_eq!(binaries(ArchivePackage::JackinCapsule), &["jackin-capsule"]);
}

#[test]
fn validator_owns_only_its_complete_linux_target_set() {
    assert_eq!(targets(ArchivePackage::JackinRole), &CAPSULE_TARGETS);
    assert_eq!(binaries(ArchivePackage::JackinRole), &["jackin-role"]);
}

#[test]
fn sidecars_preserve_archive_extension() {
    assert_eq!(
        sidecar(Path::new("dist/jackin-linux.tar.gz"), "sha256"),
        PathBuf::from("dist/jackin-linux.tar.gz.sha256")
    );
}

#[test]
fn checksum_sidecar_names_the_archive_for_sha256sum_check() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("jackin-role-linux.tar.gz");
    fs::write(&archive, b"validator archive").unwrap();

    write_checksum(&archive).unwrap();

    let sidecar = fs::read_to_string(sidecar(&archive, "sha256")).unwrap();
    assert!(sidecar.ends_with("  jackin-role-linux.tar.gz\n"));
}

#[test]
fn target_builds_share_runner_capacity() {
    assert_eq!(jobs_per_target_for(4, 4), 1);
    assert_eq!(jobs_per_target_for(8, 4), 2);
    assert_eq!(jobs_per_target_for(2, 4), 1);
}

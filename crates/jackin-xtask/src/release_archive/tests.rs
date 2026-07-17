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
fn sidecars_preserve_archive_extension() {
    assert_eq!(
        sidecar(Path::new("dist/jackin-linux.tar.gz"), "sha256"),
        PathBuf::from("dist/jackin-linux.tar.gz.sha256")
    );
}

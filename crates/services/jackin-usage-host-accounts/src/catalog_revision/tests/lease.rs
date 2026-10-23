use super::{TestRoot, assert_error, open_read_write_test_file};
use crate::catalog_revision::{
    CatalogAuthorityError, CatalogAuthorityStore, InitializationIntent, RevisionDomain,
};
use std::fs;
use std::os::unix::fs::FileExt;
use std::os::unix::fs::PermissionsExt;

#[test]
fn held_lease_revalidates_for_each_revision_and_keeps_domain_separation() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("explicit first-use decision initializes authority");
    let lease = authority
        .lease_current(&store)
        .expect("adopted authority produces one owned lease");

    lease
        .validate_current()
        .expect("durable names and retained descriptors agree");
    let first = lease
        .revision(
            RevisionDomain::HostSourceEvidenceV1,
            &[b"provider", b"source-1"],
        )
        .expect("bounded host-source evidence is accepted");
    let other_domain = lease
        .revision(
            RevisionDomain::CatalogEvidenceV1,
            &[b"provider", b"source-1"],
        )
        .expect("bounded catalog evidence is accepted");
    let other_fields = lease
        .revision(
            RevisionDomain::HostSourceEvidenceV1,
            &[b"provider-source-1"],
        )
        .expect("bounded single-field evidence is accepted");

    assert_ne!(first, other_domain);
    assert_ne!(first, other_fields);
    assert!(first.as_str().starts_with("hmac-sha256:"));
    assert_eq!(first.as_str().len(), 76);
    assert_error(
        lease.revision(
            RevisionDomain::HostSourceEvidenceV1,
            &[&vec![0; 1024 * 1024]],
        ),
        CatalogAuthorityError::InvalidEvidence,
    );
    lease
        .validate_current()
        .expect("rejected input leaves the retained lease valid");
}

#[test]
fn changed_key_bytes_are_detected_even_when_the_inode_is_unchanged() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("fixture authority initializes");
    let lease = authority
        .lease_current(&store)
        .expect("fixture lease holds the stable shared lock");
    let key_path = root.path().join("catalog-authority/key");
    let key =
        open_read_write_test_file(&key_path).expect("synthetic key can be opened for tampering");
    let mut original = [0_u8; 1];
    key.read_at(&mut original, 8)
        .expect("synthetic key byte can be observed");
    key.write_at(&[original[0] ^ 0x80], 8)
        .expect("one synthetic key byte is changed without changing length");

    assert_error(
        lease.validate_current(),
        CatalogAuthorityError::RecoveryRequired,
    );
    assert_error(
        store.load_existing(),
        CatalogAuthorityError::RecoveryRequired,
    );
}

#[test]
fn marker_epoch_and_each_stable_lock_identity_range_are_fenced() {
    for offset in [8_u64, 40, 55] {
        let root = TestRoot::new();
        let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
        store
            .initialize_explicit(InitializationIntent::FirstUse)
            .expect("fixture authority initializes");
        let marker_path = root.path().join("catalog-authority/epoch");
        let marker = open_read_write_test_file(&marker_path)
            .expect("synthetic marker can be opened for tampering");
        let mut original = [0_u8; 1];
        marker
            .read_at(&mut original, offset)
            .expect("synthetic marker byte can be observed");
        marker
            .write_at(&[original[0] ^ 0x80], offset)
            .expect("exact marker byte is changed without changing length");

        assert_eq!(
            store.load_existing().err(),
            Some(CatalogAuthorityError::RecoveryRequired),
            "marker offset {offset} belongs to epoch or stable-lock identity"
        );
    }
}

#[test]
fn key_epoch_bytes_cannot_drift_from_the_adopted_record() {
    for offset in [40_u64, 71] {
        let root = TestRoot::new();
        let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
        store
            .initialize_explicit(InitializationIntent::FirstUse)
            .expect("fixture authority initializes");
        let key_path = root.path().join("catalog-authority/key");
        let key = open_read_write_test_file(&key_path)
            .expect("synthetic key file can be opened for tampering");
        let mut original = [0_u8; 1];
        key.read_at(&mut original, offset)
            .expect("synthetic key epoch byte can be observed");
        key.write_at(&[original[0] ^ 0x80], offset)
            .expect("synthetic key epoch byte changes without changing length");

        assert_eq!(
            store.load_existing().err(),
            Some(CatalogAuthorityError::RecoveryRequired),
            "key epoch offset {offset} must agree with the adopted record"
        );
    }
}

#[test]
fn revision_rejects_adopted_state_and_marker_epoch_disagreement() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("fixture authority initializes");
    let lease = authority
        .lease_current(&store)
        .expect("fixture lease is initially valid");
    let marker_path = root.path().join("catalog-authority/epoch");
    let marker = open_read_write_test_file(&marker_path)
        .expect("synthetic marker can be opened for tampering");
    let mut original = [0_u8; 1];
    marker
        .read_at(&mut original, 8)
        .expect("synthetic marker epoch byte can be observed");
    marker
        .write_at(&[original[0] ^ 0x80], 8)
        .expect("synthetic marker epoch is changed");

    assert_error(
        lease.revision(RevisionDomain::CatalogEvidenceV1, &[b"must-not-escape"]),
        CatalogAuthorityError::RecoveryRequired,
    );
}

#[test]
fn byte_identical_marker_and_stable_lock_replacements_invalidate_the_lease() {
    for relative in [
        "catalog-authority/epoch",
        "catalog-authority.lock",
        "catalog-authority.state",
    ] {
        let root = TestRoot::new();
        let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
        let authority = store
            .initialize_explicit(InitializationIntent::FirstUse)
            .expect("fixture authority initializes");
        let lease = authority
            .lease_current(&store)
            .expect("fixture lease holds the stable shared lock");
        let original = root.path().join(relative);
        let previous = original.with_extension("previous");
        let replacement = original.with_extension("replacement");
        fs::copy(&original, &replacement).expect("replacement contains the same synthetic bytes");
        fs::set_permissions(&replacement, fs::Permissions::from_mode(0o600))
            .expect("replacement file mode is exact");
        fs::rename(&original, &previous).expect("original file name is moved aside");
        fs::rename(&replacement, &original)
            .expect("byte-identical replacement takes original name");

        assert_eq!(
            lease.validate_current().err(),
            Some(CatalogAuthorityError::RecoveryRequired),
            "replacement of {relative} must invalidate held authority"
        );
        let fresh_load = store.load_existing();
        if relative == "catalog-authority/epoch" {
            assert_error(fresh_load, CatalogAuthorityError::UnsafeStore);
        } else if relative == "catalog-authority.lock" {
            assert_error(fresh_load, CatalogAuthorityError::RecoveryRequired);
        } else {
            assert_eq!(relative, "catalog-authority.state");
            assert!(
                fresh_load.is_ok(),
                "fresh load accepts the byte-identical adopted record"
            );
        }
    }
}

#[test]
fn byte_identical_authority_directory_replacement_fails_the_directory_identity_fence() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("fixture authority initializes");
    let lease = authority
        .lease_current(&store)
        .expect("fixture lease holds the stable shared lock");
    let original = root.path().join("catalog-authority");
    let previous = root.path().join("catalog-authority.previous");
    let replacement = root.path().join("catalog-authority.replacement");
    fs::create_dir(&replacement).expect("replacement authority directory is created");
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o700))
        .expect("replacement directory mode is exact");
    for name in ["key", "epoch"] {
        fs::copy(original.join(name), replacement.join(name))
            .expect("replacement file contains the same synthetic bytes");
        fs::set_permissions(replacement.join(name), fs::Permissions::from_mode(0o600))
            .expect("replacement file mode is exact");
    }
    fs::rename(&original, &previous).expect("original authority directory is moved aside");
    fs::rename(&replacement, &original)
        .expect("byte-identical directory replacement takes original name");

    assert_error(
        lease.validate_current(),
        CatalogAuthorityError::RecoveryRequired,
    );
    assert_error(
        store.load_existing(),
        CatalogAuthorityError::RecoveryRequired,
    );
}

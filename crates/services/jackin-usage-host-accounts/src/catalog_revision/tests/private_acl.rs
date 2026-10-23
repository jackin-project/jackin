#![cfg(target_os = "macos")]

use super::{TestRoot, create_new_test_file, open_read_write_test_file, open_test_directory};
use crate::catalog_revision::private_acl::{
    FixtureAclKind, ensure_private_descriptor, set_test_acl,
};
use crate::catalog_revision::{
    CatalogAuthorityError, CatalogAuthorityStore, InitializationIntent, RevisionDomain,
};
use rustix::fs::{Mode, fchmod};
use std::ffi::OsString;
use std::fs::{self, File};
use std::os::fd::AsFd;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

#[test]
fn no_acl_store_initializes_reloads_and_keeps_a_stable_revision() {
    let root = TestRoot::new();
    let root_descriptor = open_test_directory(root.path())
        .expect("the fresh test root opens without following links");
    ensure_private_descriptor(root_descriptor.as_fd())
        .expect("the native absent-ACL result admits a private root");
    drop(root_descriptor);

    let store = CatalogAuthorityStore::open(root.path()).expect("fresh no-ACL root opens");
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("explicit first use initializes the no-ACL store");
    let first_lease = authority
        .lease_current(&store)
        .expect("initialized no-ACL authority produces a lease");
    let first_revision = first_lease
        .revision(
            RevisionDomain::HostSourceEvidenceV1,
            &[b"synthetic", b"stable"],
        )
        .expect("the synthetic evidence produces a revision");

    let reloaded = store
        .load_existing()
        .expect("the no-ACL authority reloads without creating or repairing files");
    let reloaded_lease = reloaded
        .lease_current(&store)
        .expect("reloaded no-ACL authority produces a lease");
    let reloaded_revision = reloaded_lease
        .revision(
            RevisionDomain::HostSourceEvidenceV1,
            &[b"synthetic", b"stable"],
        )
        .expect("the same synthetic evidence remains accepted");

    assert_eq!(first_revision, reloaded_revision);
}

#[test]
fn valid_empty_acl_and_absent_acl_are_both_admitted() {
    let root = TestRoot::new();
    let root_descriptor = open_test_directory(root.path())
        .expect("the fresh test root opens without following links");
    set_test_acl(root_descriptor.as_fd(), FixtureAclKind::Empty)
        .expect("the native fixture installs a valid empty extended ACL representation");
    ensure_private_descriptor(root_descriptor.as_fd())
        .expect("a validated native empty ACL has no first entry");

    let store = CatalogAuthorityStore::open(root.path())
        .expect("the valid empty ACL representation admits a private root");
    store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("explicit first use succeeds with no extended ACL entries");
}

#[test]
fn nonempty_acl_on_private_root_is_rejected_before_authority_creation() {
    let root = TestRoot::new();
    let entries_before = entry_names(root.path());
    let root_descriptor = open_test_directory(root.path())
        .expect("the fresh test root opens without following links");
    set_test_acl(root_descriptor.as_fd(), FixtureAclKind::InheritableAllow)
        .expect("the synthetic root receives a fixed inheritable allow ACE");

    assert_eq!(
        ensure_private_descriptor(root_descriptor.as_fd()),
        Err(CatalogAuthorityError::UnsafeStore)
    );
    assert_eq!(
        CatalogAuthorityStore::open(root.path()).err(),
        Some(CatalogAuthorityError::UnsafeStore)
    );
    assert_eq!(entry_names(root.path()), entries_before);
}

#[test]
fn inherited_acl_survives_private_mode_fchmod_and_blocks_admission() {
    let root = TestRoot::new();
    let parent = open_test_directory(root.path())
        .expect("the synthetic parent opens without following links");
    set_test_acl(parent.as_fd(), FixtureAclKind::InheritableAllow)
        .expect("the synthetic parent receives fixed file and directory inheritance flags");

    let child_path = root.path().join("inherited-directory");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&child_path)
        .expect("a private-mode child directory is created below the fixture parent");
    let child = open_test_directory(&child_path)
        .expect("the inherited child directory opens without following links");
    fchmod(&child, Mode::from_raw_mode(0o700))
        .expect("the child directory receives the production private-mode operation");
    assert_eq!(
        ensure_private_descriptor(child.as_fd()),
        Err(CatalogAuthorityError::UnsafeStore),
        "fchmod preserves the inherited ACL entry"
    );
    assert_eq!(
        CatalogAuthorityStore::open(&child_path).err(),
        Some(CatalogAuthorityError::UnsafeStore)
    );

    let child_file_path = root.path().join("inherited-file");
    let child_file = create_new_test_file(&child_file_path)
        .expect("a private-mode child file is created below the fixture parent");
    fchmod(&child_file, Mode::from_raw_mode(0o600))
        .expect("the child file receives the production private-mode operation");
    assert_eq!(
        ensure_private_descriptor(child_file.as_fd()),
        Err(CatalogAuthorityError::UnsafeStore),
        "file fchmod preserves the inherited ACL entry"
    );
}

#[derive(Clone, Copy, Debug)]
enum StoreObject {
    Root,
    AuthorityDirectory,
    Key,
    State,
    Epoch,
    Lock,
}

const STORE_OBJECTS: [StoreObject; 6] = [
    StoreObject::Root,
    StoreObject::AuthorityDirectory,
    StoreObject::Key,
    StoreObject::State,
    StoreObject::Epoch,
    StoreObject::Lock,
];

fn open_store_object(root: &Path, object: StoreObject) -> File {
    match object {
        StoreObject::Root => {
            open_test_directory(root).expect("synthetic root opens without following links")
        }
        StoreObject::AuthorityDirectory => open_test_directory(&root.join("catalog-authority"))
            .expect("synthetic authority directory opens without following links"),
        StoreObject::Key => open_read_write_test_file(&root.join("catalog-authority/key"))
            .expect("synthetic key opens without following links"),
        StoreObject::State => open_read_write_test_file(&root.join("catalog-authority.state"))
            .expect("synthetic state opens without following links"),
        StoreObject::Epoch => open_read_write_test_file(&root.join("catalog-authority/epoch"))
            .expect("synthetic epoch opens without following links"),
        StoreObject::Lock => open_read_write_test_file(&root.join("catalog-authority.lock"))
            .expect("synthetic lock opens without following links"),
    }
}

#[derive(PartialEq, Eq)]
struct StoreSnapshot {
    root_names: Vec<OsString>,
    authority_names: Vec<OsString>,
    bytes: Vec<(PathBuf, Vec<u8>)>,
}

fn store_snapshot(root: &Path) -> StoreSnapshot {
    let root_names = entry_names(root);
    let authority_names = entry_names(&root.join("catalog-authority"));
    let bytes = [
        "catalog-authority.lock",
        "catalog-authority.state",
        "catalog-authority/key",
        "catalog-authority/epoch",
    ]
    .into_iter()
    .map(|relative| {
        let path = PathBuf::from(relative);
        let bytes = fs::read(root.join(&path)).expect("synthetic authority bytes remain readable");
        (path, bytes)
    })
    .collect();
    StoreSnapshot {
        root_names,
        authority_names,
        bytes,
    }
}

#[test]
fn acl_on_each_anchored_store_object_blocks_fresh_and_held_use_without_byte_changes() {
    for object in STORE_OBJECTS {
        let root = TestRoot::new();
        let store = CatalogAuthorityStore::open(root.path())
            .expect("fresh synthetic root opens before ACL mutation");
        let authority = store
            .initialize_explicit(InitializationIntent::FirstUse)
            .expect("explicit first use initializes each synthetic store");
        let lease = authority
            .lease_current(&store)
            .expect("an initial lease is held before ACL mutation");

        let descriptor = open_store_object(root.path(), object);
        set_test_acl(descriptor.as_fd(), FixtureAclKind::Deny)
            .expect("the selected store descriptor receives a fixed deny ACE");
        drop(descriptor);
        let before = store_snapshot(root.path());

        assert_eq!(
            store.load_existing().err(),
            Some(CatalogAuthorityError::UnsafeStore),
            "fresh load rejects {object:?}"
        );
        assert_eq!(
            lease
                .revision(
                    RevisionDomain::CatalogEvidenceV1,
                    &[b"synthetic", b"denied"]
                )
                .err(),
            Some(CatalogAuthorityError::UnsafeStore),
            "held lease rejects {object:?} before returning a revision"
        );
        assert!(
            store_snapshot(root.path()) == before,
            "{object:?} entries and bytes are unchanged"
        );
    }
}

#[test]
fn descriptor_acl_query_stays_bound_after_rename_and_replacement() {
    let root = TestRoot::new();
    let original_path = root.path().join("bound-object");
    drop(create_new_test_file(&original_path).expect("the first synthetic file is created once"));
    let original = open_read_write_test_file(&original_path)
        .expect("the original descriptor opens without following links");
    set_test_acl(original.as_fd(), FixtureAclKind::Deny)
        .expect("the original descriptor receives the fixed deny ACE");

    let displaced_path = root.path().join("displaced-object");
    fs::rename(&original_path, &displaced_path)
        .expect("the ACL-bearing object is renamed while its descriptor stays open");
    drop(create_new_test_file(&original_path).expect("a clean replacement is created once"));
    let replacement = open_read_write_test_file(&original_path)
        .expect("the replacement descriptor opens without following links");

    assert_eq!(
        ensure_private_descriptor(original.as_fd()),
        Err(CatalogAuthorityError::UnsafeStore),
        "the retained descriptor still sees the ACL-bearing original"
    );
    ensure_private_descriptor(replacement.as_fd())
        .expect("the replacement descriptor has no ACL entries");
}

fn entry_names(directory: &Path) -> Vec<OsString> {
    let mut names = fs::read_dir(directory)
        .expect("the synthetic directory can be enumerated")
        .map(|entry| {
            entry
                .expect("synthetic directory entry metadata is readable")
                .file_name()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

use super::{
    ControlDir, TestRoot, assert_error, child_environment, child_expected_error,
    open_test_directory, open_write_test_file, read_child_value, spawn_test_child,
    wait_for_child_marker, wait_for_child_signal, wait_for_child_success, write_child_marker,
    write_child_value,
};
use crate::catalog_revision::storage::{
    FailurePoint, acquire_initialization_guard, fail_at_for_test, fail_random_for_test,
    pause_after_initial_state_published_for_test, validate_root_owner_for_test,
};
use crate::catalog_revision::{
    CatalogAuthorityError, CatalogAuthorityStore, InitializationIntent, RevisionDomain,
};
use std::fs;
use std::os::unix::fs::{FileExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::time::{Duration, Instant};

fn error_from_name(name: &str) -> Option<CatalogAuthorityError> {
    match name {
        "unsafe-store" => Some(CatalogAuthorityError::UnsafeStore),
        "recovery-required" => Some(CatalogAuthorityError::RecoveryRequired),
        _ => None,
    }
}

#[test]
fn process_child_dispatcher() -> Result<(), CatalogAuthorityError> {
    let Some(config) = child_environment() else {
        return Ok(());
    };
    let super::ChildEnvironment {
        mode,
        root,
        control,
        worker,
    } = config;
    match mode.as_str() {
        "paused_initializer" => child_paused_initializer(&root, &control),
        "race_contender" => child_race_contender(&root, &control),
        "stale_after_complete" => child_stale_after_complete(&root, &control),
        "complete_writer" => child_complete_writer(&root, &control),
        "concurrent_initializer" => child_concurrent_initializer(&root, &control, &worker),
        "lease_holder" => child_lease_holder(&root, &control),
        "lease_initializer" => child_lease_initializer(&root, &control),
        "cold_reload" => child_cold_reload(&root, &control),
        "expect_recovery" => child_expect_recovery(&root, &control),
        "gate_only" => child_gate_only(&root, &control),
        "special_probe" => {
            let Some(error_name) = child_expected_error() else {
                return Err(CatalogAuthorityError::Unsupported);
            };
            let Some(expected) = error_from_name(&error_name) else {
                return Err(CatalogAuthorityError::Unsupported);
            };
            child_special_probe(&root, &control, &worker, expected);
        }
        _ => return Err(CatalogAuthorityError::Unsupported),
    }
    Ok(())
}

fn child_paused_initializer(root: &Path, control: &Path) {
    let store = CatalogAuthorityStore::open(root).expect("child opens the private root");
    let _pause = pause_after_initial_state_published_for_test(control);
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("paused writer completes after parent release");
    let revision = expected_revision(&authority, &store);
    write_child_value(control, "INITIALIZER_REVISION", &revision);
    write_child_marker(control, "INITIALIZER_COMPLETE");
}

fn child_race_contender(root: &Path, control: &Path) {
    let store = CatalogAuthorityStore::open(root).expect("contender opens the private root");
    assert_error(store.load_existing(), CatalogAuthorityError::NotInitialized);
    write_child_marker(control, "B_OBSERVED_EMPTY");
    wait_for_child_signal(control, "TRY_B");
    let _random_must_remain_unused = fail_random_for_test();
    assert_error(
        store.initialize_explicit(InitializationIntent::FirstUse),
        CatalogAuthorityError::LockBusy,
    );
    write_child_marker(control, "B_BUSY");
    wait_for_child_signal(control, "RETRY_B");
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("explicit retry reuses the completed winner");
    let revision = expected_revision(&authority, &store);
    assert_eq!(
        revision,
        read_child_value(control, "INITIALIZER_REVISION"),
        "contender reuses the winner's sealed revision"
    );
    write_child_value(control, "B_REVISION", &revision);
    write_child_marker(control, "B_COMPLETE");
}

fn child_stale_after_complete(root: &Path, control: &Path) {
    let store = CatalogAuthorityStore::open(root).expect("stale observer opens the private root");
    assert_error(store.load_existing(), CatalogAuthorityError::NotInitialized);
    write_child_marker(control, "STALE_OBSERVED_EMPTY");
    wait_for_child_signal(control, "RELEASE_STALE_REUSE");
    let _random_must_remain_unused = fail_random_for_test();
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("stale observation reclassifies the complete authority");
    let revision = expected_revision(&authority, &store);
    assert_eq!(
        revision,
        read_child_value(control, "COMPLETE_WRITER_REVISION"),
        "stale observer reuses the completed writer's revision"
    );
    write_child_value(control, "STALE_REVISION", &revision);
    write_child_marker(control, "STALE_COMPLETE");
}

fn child_complete_writer(root: &Path, control: &Path) {
    let store = CatalogAuthorityStore::open(root).expect("writer opens the private root");
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("explicit writer completes one authority");
    let revision = expected_revision(&authority, &store);
    write_child_value(control, "COMPLETE_WRITER_REVISION", &revision);
    write_child_marker(control, "COMPLETE_WRITER_DONE");
}

fn child_concurrent_initializer(root: &Path, control: &Path, worker: &str) {
    let store = CatalogAuthorityStore::open(root).expect("concurrent child opens the root");
    write_child_marker(control, &format!("{worker}_READY"));
    wait_for_child_signal(control, "START_CONCURRENT");
    let initialized = match store.initialize_explicit(InitializationIntent::FirstUse) {
        Ok(authority) => Some(authority),
        Err(CatalogAuthorityError::LockBusy) => None,
        Err(error) => panic!("concurrent first-use initialization failed: {error}"),
    };
    write_child_marker(control, &format!("{worker}_INIT_FINISHED"));
    wait_for_child_signal(control, "RELEASE_CONCURRENT_REVISION");
    if let Some(authority) = initialized {
        let revision = expected_revision(&authority, &store);
        write_child_value(control, &format!("{worker}_RESULT"), &revision);
    } else {
        write_child_value(control, &format!("{worker}_RESULT"), "busy");
    }
    write_child_marker(control, &format!("{worker}_COMPLETE"));
}

fn child_lease_holder(root: &Path, control: &Path) {
    let store = CatalogAuthorityStore::open(root).expect("lease holder opens the root");
    let authority = store
        .load_existing()
        .expect("complete authority loads in holder");
    let lease = authority
        .lease_current(&store)
        .expect("holder acquires the shared stable lock");
    write_child_marker(control, "LEASE_HELD");
    wait_for_child_signal(control, "RELEASE_LEASE");
    lease
        .validate_current()
        .expect("authority remains valid while the lease is held");
    drop(lease);
    write_child_marker(control, "LEASE_RELEASED");
}

fn child_lease_initializer(root: &Path, control: &Path) {
    let store = CatalogAuthorityStore::open(root).expect("initializer opens the root");
    let _random_must_remain_unused = fail_random_for_test();
    assert_error(
        store.initialize_explicit(InitializationIntent::FirstUse),
        CatalogAuthorityError::LockBusy,
    );
    write_child_marker(control, "LEASE_INIT_BUSY");
    wait_for_child_signal(control, "RETRY_LEASE_INIT");
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("retry reuses the authority after lease release");
    let revision = expected_revision(&authority, &store);
    assert_eq!(
        revision,
        read_child_value(control, "LEASE_EXPECTED_REVISION"),
        "post-release initializer returns the same revision"
    );
    write_child_value(control, "LEASE_INIT_REVISION", &revision);
    write_child_marker(control, "LEASE_INIT_COMPLETE");
}

fn child_cold_reload(root: &Path, control: &Path) {
    let store = CatalogAuthorityStore::open(root).expect("fresh reader process opens the root");
    let authority = store
        .load_existing()
        .expect("fresh process reloads the authority");
    let revision = expected_revision(&authority, &store);
    assert_eq!(
        revision,
        read_child_value(control, "COLD_EXPECTED_REVISION"),
        "fresh reader derives the same sealed revision"
    );
    write_child_value(control, "COLD_RELOADED_REVISION", &revision);
    write_child_marker(control, "COLD_RELOAD_COMPLETE");
}

fn child_expect_recovery(root: &Path, control: &Path) {
    let store = CatalogAuthorityStore::open(root).expect("recovery checker opens the root");
    assert_error(
        store.load_existing(),
        CatalogAuthorityError::RecoveryRequired,
    );
    assert_error(
        store.initialize_explicit(InitializationIntent::FirstUse),
        CatalogAuthorityError::RecoveryRequired,
    );
    write_child_marker(control, "RECOVERY_DENIED");
}

fn child_gate_only(root: &Path, control: &Path) {
    let store = CatalogAuthorityStore::open(root).expect("gate-only child opens the root");
    let guard = acquire_initialization_guard(&store.root)
        .expect("fresh root gate is acquired without creating evidence");
    store
        .root
        .verify_path()
        .expect("root remains pinned under gate");
    write_child_marker(control, "GATE_ONLY_HELD");
    wait_for_child_signal(control, "RELEASE_GATE_ONLY");
    drop(guard);
    write_child_marker(control, "GATE_ONLY_RELEASED");
}

fn child_special_probe(root: &Path, control: &Path, worker: &str, expected: CatalogAuthorityError) {
    let store = CatalogAuthorityStore::open(root).expect("special-node checker opens the root");
    assert_error(store.load_existing(), expected);
    write_child_marker(control, &format!("{worker}_COMPLETE"));
}

fn legacy_intent() -> InitializationIntent {
    InitializationIntent::LegacyAdoption {
        evidence_record_id: [0x31; 32],
        evidence_digest: [0x52; 32],
    }
}

fn expected_revision(
    authority: &std::sync::Arc<crate::catalog_revision::CatalogRevisionAuthority>,
    store: &CatalogAuthorityStore,
) -> String {
    let lease = authority
        .lease_current(store)
        .expect("complete authority grants a current read lease");
    lease
        .revision(
            RevisionDomain::CatalogEvidenceV1,
            &[b"private-fixture", b"row-7"],
        )
        .expect("synthetic fixture evidence is bounded")
        .as_str()
        .to_owned()
}

#[test]
fn readonly_load_creates_nothing_and_explicit_first_use_reloads_idempotently() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");

    assert_error(store.load_existing(), CatalogAuthorityError::NotInitialized);
    assert_eq!(
        fs::read_dir(root.path())
            .expect("test root can be listed")
            .count(),
        0
    );

    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("explicit first-use decision initializes authority");
    let held = authority
        .lease_current(&store)
        .expect("authority obtains a held shared lease");
    assert_error(
        store.initialize_explicit(InitializationIntent::FirstUse),
        CatalogAuthorityError::LockBusy,
    );
    drop(held);

    let idempotent = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("complete matching authority is reused without rekeying");
    let loaded = store
        .load_existing()
        .expect("durable adopted authority reloads");
    assert_eq!(
        expected_revision(&authority, &store),
        expected_revision(&idempotent, &store)
    );
    assert_eq!(
        expected_revision(&authority, &store),
        expected_revision(&loaded, &store)
    );

    let private = fs::metadata(root.path().join("catalog-authority"))
        .expect("authority directory is present");
    assert_eq!(private.permissions().mode() & 0o7777, 0o700);
    for name in [
        "catalog-authority.lock",
        "catalog-authority.state",
        "catalog-authority/key",
        "catalog-authority/epoch",
    ] {
        let metadata = fs::metadata(root.path().join(name)).expect("private authority file exists");
        assert_eq!(metadata.permissions().mode() & 0o7777, 0o600);
        assert_eq!(metadata.nlink(), 1);
    }
}

#[test]
fn legacy_intent_is_recorded_and_conflicting_or_empty_evidence_is_rejected() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    let authority = store
        .initialize_explicit(legacy_intent())
        .expect("caller-provided legacy evidence identifiers are recorded");
    let loaded = store
        .load_existing()
        .expect("legacy adoption record survives reload");
    assert_eq!(
        expected_revision(&authority, &store),
        expected_revision(&loaded, &store)
    );
    assert_error(
        store.initialize_explicit(InitializationIntent::FirstUse),
        CatalogAuthorityError::ConflictingIntent,
    );

    let empty_root = TestRoot::new();
    let empty_store =
        CatalogAuthorityStore::open(empty_root.path()).expect("second existing private test root");
    let invalid = InitializationIntent::LegacyAdoption {
        evidence_record_id: [0; 32],
        evidence_digest: [0x52; 32],
    };
    assert_error(
        empty_store.initialize_explicit(invalid),
        CatalogAuthorityError::InvalidEvidence,
    );
    assert_eq!(
        fs::read_dir(empty_root.path())
            .expect("empty fixture root can be listed")
            .count(),
        0
    );
}

#[test]
fn random_failure_precedes_all_filesystem_mutation_and_has_no_fallback() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    let _failure = fail_random_for_test();

    assert_error(
        store.initialize_explicit(InitializationIntent::FirstUse),
        CatalogAuthorityError::RandomnessUnavailable,
    );
    assert_eq!(
        fs::read_dir(root.path())
            .expect("test root remains listable")
            .count(),
        0
    );
    assert_error(store.load_existing(), CatalogAuthorityError::NotInitialized);
}

#[test]
fn every_precommit_publication_failure_keeps_restart_fail_closed() {
    let points = [
        FailurePoint::LockTempCreated,
        FailurePoint::LockTempSynced,
        FailurePoint::LockLinked,
        FailurePoint::LockTempUnlinked,
        FailurePoint::LockRootSynced,
        FailurePoint::InitialStateTempCreated,
        FailurePoint::InitialStateWritten,
        FailurePoint::InitialStateSynced,
        FailurePoint::InitialStateLinked,
        FailurePoint::InitialStateUnlinked,
        FailurePoint::InitialStatePublished,
        FailurePoint::AuthorityDirectoryCreated,
        FailurePoint::KeyCreated,
        FailurePoint::KeyWritten,
        FailurePoint::KeySynced,
        FailurePoint::MarkerCreated,
        FailurePoint::MarkerWritten,
        FailurePoint::MarkerSynced,
        FailurePoint::AuthorityDirectorySynced,
        FailurePoint::AuthorityRootSynced,
        FailurePoint::AdoptedStateTempCreated,
        FailurePoint::AdoptedStateWritten,
        FailurePoint::AdoptedStateSynced,
    ];

    for point in points {
        let root = TestRoot::new();
        let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
        let _failure = fail_at_for_test(point);

        assert_eq!(
            store
                .initialize_explicit(InitializationIntent::FirstUse)
                .err(),
            Some(CatalogAuthorityError::StorageFailure),
            "failure point {point:?} is injected after its named operation"
        );
        let expected_restart_error = if point == FailurePoint::LockLinked {
            CatalogAuthorityError::UnsafeStore
        } else {
            CatalogAuthorityError::RecoveryRequired
        };
        let retained_lock_publication = if point == FailurePoint::LockLinked {
            let pending_path = root
                .path()
                .join(".catalog-authority.lock.initializing.pending");
            let stable_path = root.path().join("catalog-authority.lock");
            assert!(
                root_names(root.path())
                    == vec![
                        ".catalog-authority.lock.initializing.pending".to_owned(),
                        "catalog-authority.lock".to_owned(),
                    ],
                "LockLinked leaves only pending and stable lock names"
            );
            let pending_metadata = fs::symlink_metadata(&pending_path)
                .expect("LockLinked pending lock remains present");
            let stable_metadata =
                fs::symlink_metadata(&stable_path).expect("LockLinked stable lock remains present");
            assert!(
                pending_metadata.file_type().is_file(),
                "LockLinked pending name remains a regular file"
            );
            assert!(
                stable_metadata.file_type().is_file(),
                "LockLinked stable name remains a regular file"
            );
            assert!(
                pending_metadata.dev() == stable_metadata.dev()
                    && pending_metadata.ino() == stable_metadata.ino(),
                "LockLinked names retain the same inode"
            );
            assert_eq!(
                pending_metadata.nlink(),
                2,
                "pending lock retains two links"
            );
            assert_eq!(stable_metadata.nlink(), 2, "stable lock retains two links");
            let effective_uid = rustix::process::geteuid().as_raw();
            assert_eq!(
                pending_metadata.uid(),
                effective_uid,
                "pending lock remains owned by the effective UID"
            );
            assert_eq!(
                stable_metadata.uid(),
                effective_uid,
                "stable lock remains owned by the effective UID"
            );
            assert_eq!(
                pending_metadata.mode() & 0o7777,
                0o600,
                "pending lock remains private"
            );
            assert_eq!(
                stable_metadata.mode() & 0o7777,
                0o600,
                "stable lock remains private"
            );
            assert_eq!(pending_metadata.len(), 0, "pending lock remains empty");
            assert_eq!(stable_metadata.len(), 0, "stable lock remains empty");
            Some(root_snapshot(root.path()))
        } else {
            None
        };

        assert_eq!(
            store.load_existing().err(),
            Some(expected_restart_error),
            "failure point {point:?} refuses to yield authority from interrupted publication"
        );

        if let Some(retained) = retained_lock_publication {
            assert!(
                root_snapshot(root.path()) == retained,
                "same-handle read refusal leaves LockLinked evidence unchanged"
            );
            let reopened = CatalogAuthorityStore::open(root.path())
                .expect("fresh store handle opens the same private root");
            assert_eq!(
                reopened.load_existing().err(),
                Some(CatalogAuthorityError::UnsafeStore),
                "fresh-handle read refuses LockLinked evidence"
            );
            assert!(
                root_snapshot(root.path()) == retained,
                "fresh-handle read refusal leaves LockLinked evidence unchanged"
            );
            assert_eq!(
                reopened
                    .initialize_explicit(InitializationIntent::FirstUse)
                    .err(),
                Some(CatalogAuthorityError::UnsafeStore),
                "explicit FirstUse refuses LockLinked evidence"
            );
            assert!(
                root_snapshot(root.path()) == retained,
                "explicit FirstUse refusal leaves LockLinked evidence unchanged"
            );
        }
    }
}

#[test]
fn failures_after_atomic_adoption_publish_leave_only_a_complete_reloadable_authority() {
    for point in [
        FailurePoint::AdoptedStateRenamed,
        FailurePoint::AdoptedStateRootSynced,
    ] {
        let root = TestRoot::new();
        let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
        let _failure = fail_at_for_test(point);

        assert_error(
            store.initialize_explicit(InitializationIntent::FirstUse),
            CatalogAuthorityError::StorageFailure,
        );
        let loaded = store
            .load_existing()
            .expect("published state is complete even when the caller saw a late error");
        let revision = loaded
            .lease_current(&store)
            .expect("complete publication can be leased")
            .revision(
                RevisionDomain::HostSourceEvidenceV1,
                &[b"restart", b"same-key"],
            )
            .expect("bounded post-restart evidence is accepted");
        assert_eq!(revision.as_str().len(), 76);
    }
}

#[test]
fn missing_adopted_objects_deny_existing_handles_and_fresh_loads() {
    for missing in ["lock", "state", "directory", "key", "epoch"] {
        let root = TestRoot::new();
        let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
        let authority = store
            .initialize_explicit(InitializationIntent::FirstUse)
            .expect("fixture authority initializes");
        let lease = authority
            .lease_current(&store)
            .expect("fixture lease holds shared lock");
        let path = match missing {
            "lock" => root.path().join("catalog-authority.lock"),
            "state" => root.path().join("catalog-authority.state"),
            "directory" => root.path().join("catalog-authority"),
            "key" => root.path().join("catalog-authority/key"),
            "epoch" => root.path().join("catalog-authority/epoch"),
            _ => root.path().join("unexpected-object"),
        };
        if missing == "directory" {
            fs::remove_dir_all(&path).expect("fixture authority directory can be removed");
        } else {
            fs::remove_file(&path).expect("fixture adopted object can be removed");
        }

        assert_eq!(
            lease.validate_current().err(),
            Some(CatalogAuthorityError::RecoveryRequired),
            "existing lease denies after deleting {missing}"
        );
        assert_eq!(
            store.load_existing().err(),
            Some(CatalogAuthorityError::RecoveryRequired),
            "fresh read-only load denies after deleting {missing}"
        );
        drop(lease);
        assert_error(
            store.initialize_explicit(InitializationIntent::FirstUse),
            CatalogAuthorityError::RecoveryRequired,
        );
    }
}

#[test]
fn byte_identical_key_replacement_fails_the_pinned_inode_fence() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("fixture authority initializes");
    let lease = authority
        .lease_current(&store)
        .expect("fixture lease holds shared lock");
    let directory = root.path().join("catalog-authority");
    let key = directory.join("key");
    let old_key = directory.join("key.previous");
    let replacement = directory.join("key.replacement");
    fs::copy(&key, &replacement).expect("replacement contains the same synthetic bytes");
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o600))
        .expect("replacement fixture mode is exact");
    fs::rename(&key, &old_key).expect("original key name is moved aside");
    fs::rename(&replacement, &key).expect("byte-identical replacement takes original name");

    assert_error(
        lease.validate_current(),
        CatalogAuthorityError::RecoveryRequired,
    );
}

#[test]
fn root_symlink_wrong_root_and_root_replacement_are_rejected() {
    use std::os::unix::fs::symlink;

    let root = TestRoot::new();
    let link = root.path().join("root-link");
    symlink(root.path(), &link).expect("symlink fixture is created");
    assert_error(
        CatalogAuthorityStore::open(&link),
        CatalogAuthorityError::UnsafeStore,
    );
    fs::remove_file(&link).expect("symlink fixture is removed");

    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("fixture authority initializes");
    let other_root = TestRoot::new();
    let other_store =
        CatalogAuthorityStore::open(other_root.path()).expect("second existing private test root");
    assert_error(
        authority.lease_current(&other_store),
        CatalogAuthorityError::UnsafeStore,
    );

    let lease = authority
        .lease_current(&store)
        .expect("fixture authority leases its own root");
    let moved_root = root.path().with_extension("root-moved");
    fs::rename(root.path(), &moved_root).expect("root fixture moves without copying");
    assert_error(lease.validate_current(), CatalogAuthorityError::UnsafeStore);
    fs::rename(&moved_root, root.path()).expect("root fixture identity is restored");
}

#[test]
fn symlink_in_an_ancestor_component_is_rejected_before_store_open() {
    use std::os::unix::fs::symlink;

    let root = TestRoot::new();
    let real_parent = root.path().join("real-parent");
    let child = real_parent.join("authority-root");
    let parent_link = root.path().join("parent-link");
    fs::create_dir(&real_parent).expect("real parent fixture is created");
    fs::create_dir(&child).expect("private-root fixture is created");
    fs::set_permissions(&real_parent, fs::Permissions::from_mode(0o700))
        .expect("fixture ancestor mode is explicit");
    fs::set_permissions(&child, fs::Permissions::from_mode(0o700))
        .expect("fixture root mode is explicit");
    symlink(&real_parent, &parent_link).expect("ancestor symlink fixture is created");
    let symlinked_child = parent_link.join("authority-root");

    assert_error(
        CatalogAuthorityStore::open(&symlinked_child),
        CatalogAuthorityError::UnsafeStore,
    );
}

#[test]
fn a_directory_in_a_regular_file_position_and_root_mode_are_not_repaired() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("fixture authority initializes");
    let key = root.path().join("catalog-authority/key");
    fs::remove_file(&key).expect("synthetic key is removed");
    fs::create_dir(&key).expect("directory is placed in the regular-file slot");
    fs::set_permissions(&key, fs::Permissions::from_mode(0o700))
        .expect("replacement directory mode is explicit");
    assert_error(store.load_existing(), CatalogAuthorityError::UnsafeStore);

    let unsafe_root = TestRoot::new();
    fs::set_permissions(unsafe_root.path(), fs::Permissions::from_mode(0o755))
        .expect("root fixture mode is made deliberately unsafe");
    assert_error(
        CatalogAuthorityStore::open(unsafe_root.path()),
        CatalogAuthorityError::UnsafeStore,
    );
    assert_eq!(
        fs::metadata(unsafe_root.path())
            .expect("unsafe root remains present")
            .permissions()
            .mode()
            & 0o7777,
        0o755
    );
}

#[test]
fn truncated_and_oversized_persistent_records_are_not_salvaged() {
    let truncated_root = TestRoot::new();
    let truncated_store =
        CatalogAuthorityStore::open(truncated_root.path()).expect("existing private test root");
    truncated_store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("fixture authority initializes");
    let key_path = truncated_root.path().join("catalog-authority/key");
    open_write_test_file(&key_path)
        .expect("synthetic key can be opened for truncation")
        .set_len(71)
        .expect("synthetic key is truncated by one byte");
    assert_error(
        truncated_store.load_existing(),
        CatalogAuthorityError::UnsafeStore,
    );
    assert_eq!(
        fs::metadata(&key_path)
            .expect("truncated key remains for explicit recovery")
            .len(),
        71
    );

    let oversized_root = TestRoot::new();
    let oversized_store = CatalogAuthorityStore::open(oversized_root.path())
        .expect("second existing private test root");
    oversized_store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("second fixture authority initializes");
    let state_path = oversized_root.path().join("catalog-authority.state");
    open_write_test_file(&state_path)
        .expect("synthetic state record can be opened for extension")
        .set_len(225)
        .expect("synthetic state record is extended by one byte");
    assert_error(
        oversized_store.load_existing(),
        CatalogAuthorityError::UnsafeStore,
    );
    assert_eq!(
        fs::metadata(&state_path)
            .expect("oversized state remains for explicit recovery")
            .len(),
        225
    );
}

#[test]
fn unknown_persistent_state_schema_is_denied_without_repair() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("fixture authority initializes");
    let state_path = root.path().join("catalog-authority.state");
    let state = open_write_test_file(&state_path)
        .expect("synthetic state record can be opened for schema mutation");
    assert_eq!(
        state
            .write_at(&[0xff, 0xff], 8)
            .expect("schema bytes can be changed in place"),
        2
    );
    state
        .sync_all()
        .expect("synthetic schema mutation is durable");
    let preserved = root_snapshot(root.path());

    assert_error(
        store.load_existing(),
        CatalogAuthorityError::RecoveryRequired,
    );
    assert_error(
        store.initialize_explicit(InitializationIntent::FirstUse),
        CatalogAuthorityError::RecoveryRequired,
    );
    assert!(
        root_snapshot(root.path()) == preserved,
        "unknown schema remains byte-for-byte intact after read and init refusal"
    );
}

type RootEntrySnapshot = (String, u64, u64, u64, u32, u64, Option<Vec<u8>>);

fn root_snapshot(root: &Path) -> Vec<RootEntrySnapshot> {
    let mut entries = fs::read_dir(root)
        .expect("private test root can be listed")
        .map(|entry| {
            let entry = entry.expect("root entry metadata is available");
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).expect("root entry metadata remains available");
            let contents = metadata
                .file_type()
                .is_file()
                .then(|| fs::read(&path).expect("regular root entry can be snapshotted privately"));
            (
                entry
                    .file_name()
                    .into_string()
                    .expect("test root entry names are UTF-8"),
                metadata.dev(),
                metadata.ino(),
                metadata.len(),
                metadata.mode(),
                metadata.nlink(),
                contents,
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

fn root_names(root: &Path) -> Vec<String> {
    let mut names = fs::read_dir(root)
        .expect("private test root can be listed")
        .map(|entry| {
            entry
                .expect("root entry can be read")
                .file_name()
                .into_string()
                .expect("test root entry names are UTF-8")
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn assert_no_pending_names(root: &Path) {
    for name in [
        ".catalog-authority.lock.initializing.pending",
        ".catalog-authority.state.initializing.pending",
        ".catalog-authority.state.adopted.pending",
    ] {
        assert!(
            !root.join(name).exists(),
            "completed authority has no {name}"
        );
    }
}

#[test]
fn concurrent_initializers_never_accept_distinct_first_use_authorities() {
    let root = TestRoot::new();
    let control = ControlDir::new();
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut first = spawn_test_child(&root, &control, "concurrent_initializer", "FIRST", None);
    let mut second = spawn_test_child(&root, &control, "concurrent_initializer", "SECOND", None);

    wait_for_child_marker(&mut first, &control, "FIRST_READY", deadline);
    wait_for_child_marker(&mut second, &control, "SECOND_READY", deadline);
    control.signal("START_CONCURRENT");
    wait_for_child_marker(&mut first, &control, "FIRST_INIT_FINISHED", deadline);
    wait_for_child_marker(&mut second, &control, "SECOND_INIT_FINISHED", deadline);
    control.signal("RELEASE_CONCURRENT_REVISION");
    wait_for_child_marker(&mut first, &control, "FIRST_COMPLETE", deadline);
    wait_for_child_marker(&mut second, &control, "SECOND_COMPLETE", deadline);
    wait_for_child_success(&mut first, deadline);
    wait_for_child_success(&mut second, deadline);

    let first_result = control.read_value("FIRST_RESULT");
    let second_result = control.read_value("SECOND_RESULT");
    let successful = [first_result.as_str(), second_result.as_str()]
        .into_iter()
        .filter(|value| *value != "busy")
        .collect::<Vec<_>>();
    assert!(
        !successful.is_empty(),
        "one process initializes the authority"
    );
    let store = CatalogAuthorityStore::open(root.path()).expect("parent opens completed root");
    let loaded = store
        .load_existing()
        .expect("one complete authority is durable");
    let expected = expected_revision(&loaded, &store);
    for revision in successful {
        assert_eq!(
            revision,
            expected.as_str(),
            "all successful writers share one authority"
        );
    }
    assert_no_pending_names(root.path());
}

#[test]
fn root_gate_blocks_the_stale_pending_inode_race_and_reuses_after_completion() {
    let root = TestRoot::new();
    let control = ControlDir::new();
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut contender = spawn_test_child(&root, &control, "race_contender", "B", None);
    wait_for_child_marker(&mut contender, &control, "B_OBSERVED_EMPTY", deadline);
    let mut winner = spawn_test_child(&root, &control, "paused_initializer", "A", None);
    wait_for_child_marker(&mut winner, &control, "INITIAL_STATE_READY", deadline);

    let initializing_snapshot = root_snapshot(root.path());
    assert_eq!(
        root_names(root.path()),
        vec![
            "catalog-authority.lock".to_owned(),
            "catalog-authority.state".to_owned(),
        ]
    );
    control.signal("TRY_B");
    wait_for_child_marker(&mut contender, &control, "B_BUSY", deadline);
    assert!(
        root_snapshot(root.path()) == initializing_snapshot,
        "busy contender does not add a pending name or alter initializing state"
    );

    control.signal("RELEASE_INITIALIZER");
    wait_for_child_marker(&mut winner, &control, "INITIALIZER_COMPLETE", deadline);
    wait_for_child_success(&mut winner, deadline);
    let completed_snapshot = root_snapshot(root.path());
    control.signal("RETRY_B");
    wait_for_child_marker(&mut contender, &control, "B_COMPLETE", deadline);
    wait_for_child_success(&mut contender, deadline);
    assert_eq!(
        control.read_value("B_REVISION"),
        control.read_value("INITIALIZER_REVISION")
    );
    assert!(
        root_snapshot(root.path()) == completed_snapshot,
        "explicit retry reuses the completed authority without mutation"
    );
    assert_no_pending_names(root.path());

    let stale_root = TestRoot::new();
    let mut stale = spawn_test_child(&stale_root, &control, "stale_after_complete", "STALE", None);
    wait_for_child_marker(&mut stale, &control, "STALE_OBSERVED_EMPTY", deadline);
    let mut complete_writer =
        spawn_test_child(&stale_root, &control, "complete_writer", "WRITER", None);
    wait_for_child_marker(
        &mut complete_writer,
        &control,
        "COMPLETE_WRITER_DONE",
        deadline,
    );
    wait_for_child_success(&mut complete_writer, deadline);
    let stale_complete_snapshot = root_snapshot(stale_root.path());
    control.signal("RELEASE_STALE_REUSE");
    wait_for_child_marker(&mut stale, &control, "STALE_COMPLETE", deadline);
    wait_for_child_success(&mut stale, deadline);
    assert_eq!(
        control.read_value("STALE_REVISION"),
        control.read_value("COMPLETE_WRITER_REVISION")
    );
    assert!(
        root_snapshot(stale_root.path()) == stale_complete_snapshot,
        "stale empty observation reclassifies a completed authority without mutation"
    );
    assert_no_pending_names(stale_root.path());
}

#[test]
fn root_gate_uses_a_fresh_open_description_and_releases_only_with_its_owner() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    let second_store =
        CatalogAuthorityStore::open(root.path()).expect("second store pins the same root");
    let guard = acquire_initialization_guard(&store.root)
        .expect("root gate is acquired through an independent directory open");

    store
        .root
        .verify_path()
        .expect("root verification works under the gate");
    assert_error(
        store.initialize_explicit(InitializationIntent::FirstUse),
        CatalogAuthorityError::LockBusy,
    );
    assert_error(
        second_store.initialize_explicit(InitializationIntent::FirstUse),
        CatalogAuthorityError::LockBusy,
    );

    let independent = open_test_directory(root.path())
        .expect("another independent descriptor opens the same directory");
    let independent_metadata = independent
        .metadata()
        .expect("independent descriptor metadata is available");
    let pinned_metadata = fs::metadata(root.path()).expect("root metadata is available");
    assert_eq!(independent_metadata.dev(), pinned_metadata.dev());
    assert_eq!(independent_metadata.ino(), pinned_metadata.ino());
    drop(independent);
    assert_error(
        second_store.initialize_explicit(InitializationIntent::FirstUse),
        CatalogAuthorityError::LockBusy,
    );

    let unrelated_root = TestRoot::new();
    let unrelated_store = CatalogAuthorityStore::open(unrelated_root.path())
        .expect("independent private root opens while this root is gated");
    unrelated_store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("different roots do not contend on one another's gate");
    assert!(root_names(root.path()).is_empty());

    drop(guard);
    store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("dropping the gate releases first use without persistent bootstrap state");
}

#[test]
fn root_gate_and_initializing_crashes_release_only_ephemeral_locks() {
    let control = ControlDir::new();
    let deadline = Instant::now() + Duration::from_secs(45);

    let gate_root = TestRoot::new();
    let mut gate_child = spawn_test_child(&gate_root, &control, "gate_only", "GATE", None);
    wait_for_child_marker(&mut gate_child, &control, "GATE_ONLY_HELD", deadline);
    assert!(root_names(gate_root.path()).is_empty());
    let gate_status = gate_child
        .kill_and_reap()
        .expect("owned gate-only child status is confirmed reaped");
    assert!(
        gate_status.code().is_none(),
        "parent kills the gate-only child"
    );
    assert!(root_names(gate_root.path()).is_empty());
    let gate_store = CatalogAuthorityStore::open(gate_root.path())
        .expect("parent reopens the root after gate-only child exit");
    assert_error(
        gate_store.load_existing(),
        CatalogAuthorityError::NotInitialized,
    );
    gate_store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("absence after gate-only exit still requires explicit FirstUse");

    let interrupted_root = TestRoot::new();
    let mut interrupted = spawn_test_child(
        &interrupted_root,
        &control,
        "paused_initializer",
        "INTERRUPTED",
        None,
    );
    wait_for_child_marker(&mut interrupted, &control, "INITIAL_STATE_READY", deadline);
    let before_crash = root_snapshot(interrupted_root.path());
    assert_eq!(
        root_names(interrupted_root.path()),
        vec![
            "catalog-authority.lock".to_owned(),
            "catalog-authority.state".to_owned(),
        ]
    );
    let interrupted_status = interrupted
        .kill_and_reap()
        .expect("owned interrupted writer status is confirmed reaped");
    assert!(
        interrupted_status.code().is_none(),
        "parent kills the writer at durable Initializing"
    );
    assert!(
        root_snapshot(interrupted_root.path()) == before_crash,
        "process termination preserves the durable Initializing evidence"
    );

    let mut recovery = spawn_test_child(
        &interrupted_root,
        &control,
        "expect_recovery",
        "RECOVERY",
        None,
    );
    wait_for_child_marker(&mut recovery, &control, "RECOVERY_DENIED", deadline);
    wait_for_child_success(&mut recovery, deadline);
    assert!(
        root_snapshot(interrupted_root.path()) == before_crash,
        "fresh process denial does not mutate incomplete state"
    );
}

#[test]
fn cross_process_lease_blocks_writer_then_cold_reader_reloads_same_revision() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    let authority = store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("explicit first use initializes an authority");
    let expected = expected_revision(&authority, &store);
    drop(authority);

    let control = ControlDir::new();
    control.write_value("LEASE_EXPECTED_REVISION", &expected);
    control.write_value("COLD_EXPECTED_REVISION", &expected);
    let deadline = Instant::now() + Duration::from_secs(45);
    let before = root_snapshot(root.path());
    let mut holder = spawn_test_child(&root, &control, "lease_holder", "HOLDER", None);
    wait_for_child_marker(&mut holder, &control, "LEASE_HELD", deadline);
    let mut initializer =
        spawn_test_child(&root, &control, "lease_initializer", "LEASE_INIT", None);
    wait_for_child_marker(&mut initializer, &control, "LEASE_INIT_BUSY", deadline);
    assert!(
        root_snapshot(root.path()) == before,
        "exclusive writer blocked by the held shared lease leaves no file changes"
    );

    control.signal("RELEASE_LEASE");
    wait_for_child_marker(&mut holder, &control, "LEASE_RELEASED", deadline);
    wait_for_child_success(&mut holder, deadline);
    control.signal("RETRY_LEASE_INIT");
    wait_for_child_marker(&mut initializer, &control, "LEASE_INIT_COMPLETE", deadline);
    wait_for_child_success(&mut initializer, deadline);
    assert_eq!(control.read_value("LEASE_INIT_REVISION"), expected);
    assert_eq!(root_snapshot(root.path()), before);

    let mut cold_reader = spawn_test_child(&root, &control, "cold_reload", "COLD", None);
    wait_for_child_marker(&mut cold_reader, &control, "COLD_RELOAD_COMPLETE", deadline);
    wait_for_child_success(&mut cold_reader, deadline);
    assert_eq!(control.read_value("COLD_RELOADED_REVISION"), expected);
    assert_eq!(root_snapshot(root.path()), before);
}

fn install_special_node(root: &Path, name: &str, kind: &str) -> Option<UnixListener> {
    match kind {
        #[cfg(target_os = "linux")]
        "fifo" => {
            let directory = open_test_directory(root)
                .expect("private root directory can be opened for FIFO fixture");
            rustix::fs::mkfifoat(&directory, name, rustix::fs::Mode::from_raw_mode(0o600))
                .expect("synthetic FIFO is created without opening it");
            None
        }
        "socket" => Some(
            UnixListener::bind(root.join(name))
                .expect("synthetic Unix socket is created at the authority name"),
        ),
        _ => None,
    }
}

#[test]
fn fifo_and_socket_nodes_in_regular_file_positions_fail_without_block_or_repair() {
    let control = ControlDir::new();
    let deadline = Instant::now() + Duration::from_secs(45);
    // rustix exposes safe mkfifoat on Linux; Apple still runs the socket cases.
    #[cfg(target_os = "linux")]
    let special_kinds = ["fifo", "socket"];
    #[cfg(target_os = "macos")]
    let special_kinds = ["socket"];
    for kind in special_kinds {
        let lock_root = if kind == "socket" {
            TestRoot::new_for_socket()
        } else {
            TestRoot::new()
        };
        let lock_listener = install_special_node(lock_root.path(), "catalog-authority.lock", kind);
        let lock_before = root_snapshot(lock_root.path());
        let worker = format!("LOCK_{kind}");
        let mut lock_probe = spawn_test_child(
            &lock_root,
            &control,
            "special_probe",
            &worker,
            Some("unsafe-store"),
        );
        wait_for_child_marker(
            &mut lock_probe,
            &control,
            &format!("{worker}_COMPLETE"),
            deadline,
        );
        wait_for_child_success(&mut lock_probe, deadline);
        assert_eq!(root_snapshot(lock_root.path()), lock_before);
        drop(lock_listener);

        let state_root = if kind == "socket" {
            TestRoot::new_for_socket()
        } else {
            TestRoot::new()
        };
        let state_store =
            CatalogAuthorityStore::open(state_root.path()).expect("private state root opens");
        state_store
            .initialize_explicit(InitializationIntent::FirstUse)
            .expect("fixture authority initializes before special state replacement");
        let state_path = state_root.path().join("catalog-authority.state");
        fs::remove_file(&state_path).expect("synthetic adopted state is removed for fixture");
        let state_listener =
            install_special_node(state_root.path(), "catalog-authority.state", kind);
        let state_before = root_snapshot(state_root.path());
        let worker = format!("STATE_{kind}");
        let mut state_probe = spawn_test_child(
            &state_root,
            &control,
            "special_probe",
            &worker,
            Some("unsafe-store"),
        );
        wait_for_child_marker(
            &mut state_probe,
            &control,
            &format!("{worker}_COMPLETE"),
            deadline,
        );
        wait_for_child_success(&mut state_probe, deadline);
        assert_eq!(root_snapshot(state_root.path()), state_before);
        drop(state_listener);
    }
}

#[test]
fn wrong_owner_metadata_is_rejected_without_privileged_fixture_changes() {
    let root = TestRoot::new();
    let store = CatalogAuthorityStore::open(root.path()).expect("existing private test root");
    let current_uid = fs::metadata(root.path())
        .expect("root owner metadata is available")
        .uid();
    let wrong_uid = current_uid.wrapping_add(1);
    assert_error(
        validate_root_owner_for_test(&store.root, wrong_uid),
        CatalogAuthorityError::UnsafeStore,
    );
}

#[test]
fn hardlink_and_nonprivate_modes_fail_closed_without_repair() {
    let hardlink_root = TestRoot::new();
    let hardlink_store =
        CatalogAuthorityStore::open(hardlink_root.path()).expect("existing private test root");
    hardlink_store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("fixture authority initializes");
    let key = hardlink_root.path().join("catalog-authority/key");
    let extra_link = hardlink_root.path().join("second-link");
    fs::hard_link(&key, &extra_link).expect("synthetic key obtains a second hard link");
    assert_error(
        hardlink_store.load_existing(),
        CatalogAuthorityError::UnsafeStore,
    );
    assert_eq!(
        fs::metadata(&key)
            .expect("hardlinked synthetic key remains present")
            .nlink(),
        2
    );

    let mode_root = TestRoot::new();
    let mode_store = CatalogAuthorityStore::open(mode_root.path()).expect("existing private root");
    mode_store
        .initialize_explicit(InitializationIntent::FirstUse)
        .expect("fixture authority initializes");
    let key = mode_root.path().join("catalog-authority/key");
    fs::set_permissions(&key, fs::Permissions::from_mode(0o640))
        .expect("synthetic key mode is made deliberately unsafe");
    assert_error(
        mode_store.load_existing(),
        CatalogAuthorityError::UnsafeStore,
    );
    assert_eq!(
        fs::metadata(&key)
            .expect("unsafe file remains for explicit recovery")
            .permissions()
            .mode()
            & 0o7777,
        0o640
    );
}

#[test]
fn error_and_intent_debug_output_do_not_contain_path_or_evidence() {
    let secret_path = "/fixture/opaque-root-9911";
    let intent = InitializationIntent::LegacyAdoption {
        evidence_record_id: [0x99; 32],
        evidence_digest: [0x11; 32],
    };
    let debug = format!("{intent:?}");
    let error = CatalogAuthorityError::StorageFailure;
    let display = error.to_string();
    assert!(!debug.contains("99"));
    assert!(!debug.contains("11"));
    assert!(!debug.contains(secret_path));
    assert!(!display.contains(secret_path));
}

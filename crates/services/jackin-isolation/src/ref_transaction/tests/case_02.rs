// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

const LOCK_NAME: &str = "HEAD.lock";
const OWNER_BYTES: &[u8] = b"jackin-ref-transaction-v1\n";
const PAYLOAD_NAME: &str = "unrelated-payload";
const PAYLOAD_BYTES: &[u8] = b"retain older recovery metadata\n";
const PARENT_SYNC_ERROR: &str = "injected parent directory sync failure";
const JOURNAL_SYNC_ERROR: &str = "injected journal directory sync failure";

struct LockFixture {
    _dir: tempfile::TempDir,
    root: OwnedFd,
    journal: OwnedFd,
    journal_name: String,
    candidate_name: String,
    marker: Vec<u8>,
    candidate_identity: FileStat,
    parent_identity: FileStat,
    journal_identity: FileStat,
    owner_identity: FileStat,
    lease_identity: FileStat,
    payload_identity: FileStat,
}

fn create_lock_fixture() -> (LockFixture, RecoveryDir) {
    let (dir, root) = fixture();
    let mut recovery = RecoveryDir::create(root.as_fd()).expect("recovery journal");
    let journal_name = recovery.name.clone();
    let journal = recovery
        .fd
        .as_fd()
        .try_clone_to_owned()
        .expect("clone journal fd");
    let parent_identity = fstat(root.as_fd()).expect("stat lock parent");
    let journal_identity = fstat(journal.as_fd()).expect("stat recovery journal");
    let (candidate_name, candidate_file, candidate_identity, marker) = recovery
        .create_lock_candidate(root.as_fd(), LOCK_NAME)
        .expect("create lock candidate");
    linkat(
        &recovery.fd,
        candidate_name.as_str(),
        root.as_fd(),
        LOCK_NAME,
        AtFlags::empty(),
    )
    .expect("link candidate as conventional lock");
    sync_directory(root.as_fd()).expect("sync lock parent setup");

    let mut payload: File = openat(
        &recovery.fd,
        PAYLOAD_NAME,
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::S_IRUSR | Mode::S_IWUSR,
    )
    .expect("create unrelated payload")
    .into();
    payload
        .write_all(PAYLOAD_BYTES)
        .expect("write unrelated payload");
    payload.sync_all().expect("sync unrelated payload");
    recovery.register_file(PAYLOAD_NAME);
    sync_directory(recovery.fd.as_fd()).expect("sync journal setup");
    drop(payload);
    drop(candidate_file);

    let owner = read_regular(recovery.fd.as_fd(), "owner")
        .expect("read owner")
        .expect("owner exists");
    assert_eq!(owner.bytes, OWNER_BYTES);
    let lease = read_regular(recovery.fd.as_fd(), "lease")
        .expect("read lease")
        .expect("lease exists");
    let payload = read_regular(recovery.fd.as_fd(), PAYLOAD_NAME)
        .expect("read unrelated payload")
        .expect("payload exists");
    assert_eq!(payload.bytes, PAYLOAD_BYTES);

    (
        LockFixture {
            _dir: dir,
            root,
            journal,
            journal_name,
            candidate_name,
            marker,
            candidate_identity,
            parent_identity,
            journal_identity,
            owner_identity: owner.identity,
            lease_identity: lease.identity,
            payload_identity: payload.identity,
        },
        recovery,
    )
}

fn abandoned_lock_fixture() -> LockFixture {
    let (fixture, recovery) = create_lock_fixture();
    recovery.preserve();
    drop(recovery);
    fixture
}

fn assert_journal_metadata(fixture: &LockFixture) {
    let journal_path = open_directory(fixture.root.as_fd(), &fixture.journal_name, false)
        .expect("recovery journal path remains");
    assert!(same_file(
        &fstat(journal_path.as_fd()).expect("stat journal path"),
        &fixture.journal_identity
    ));

    let owner = read_regular(fixture.journal.as_fd(), "owner")
        .expect("read owner")
        .expect("owner remains");
    assert!(same_file(&owner.identity, &fixture.owner_identity));
    assert_eq!(owner.bytes, OWNER_BYTES);

    let lease = read_regular(fixture.journal.as_fd(), "lease")
        .expect("read lease")
        .expect("lease remains");
    assert!(same_file(&lease.identity, &fixture.lease_identity));
    assert!(lease.bytes.is_empty());

    let payload = read_regular(fixture.journal.as_fd(), PAYLOAD_NAME)
        .expect("read unrelated payload")
        .expect("payload remains");
    assert!(same_file(&payload.identity, &fixture.payload_identity));
    assert_eq!(payload.bytes, PAYLOAD_BYTES);
}

fn assert_lock_present(fixture: &LockFixture) {
    let lock = read_regular(fixture.root.as_fd(), LOCK_NAME)
        .expect("read conventional lock")
        .expect("conventional lock remains");
    assert!(same_file(&lock.identity, &fixture.candidate_identity));
    assert_eq!(lock.bytes, fixture.marker);
}

fn assert_lock_absent(fixture: &LockFixture) {
    assert!(
        read_regular(fixture.root.as_fd(), LOCK_NAME)
            .expect("read conventional lock")
            .is_none()
    );
}

fn assert_candidate_present(fixture: &LockFixture) {
    let candidate = read_regular(fixture.journal.as_fd(), &fixture.candidate_name)
        .expect("read recovery candidate")
        .expect("recovery candidate remains");
    assert!(same_file(&candidate.identity, &fixture.candidate_identity));
    assert_eq!(candidate.bytes, fixture.marker);
}

fn assert_candidate_absent(fixture: &LockFixture) {
    assert!(
        read_regular(fixture.journal.as_fd(), &fixture.candidate_name)
            .expect("read recovery candidate")
            .is_none()
    );
}

#[test]
fn parent_sync_failure_keeps_candidate_witness_and_propagates() {
    let fixture = abandoned_lock_fixture();
    let mut callbacks = 0;

    let result = reclaim_abandoned_lock_with_sync(
        fixture.root.as_fd(),
        fixture.root.as_fd(),
        LOCK_NAME,
        |directory| {
            callbacks += 1;
            let identity = fstat(directory)?;
            assert!(same_file(&identity, &fixture.parent_identity));
            assert_lock_absent(&fixture);
            assert_candidate_present(&fixture);
            Err(anyhow::anyhow!(PARENT_SYNC_ERROR))
        },
    );

    let error = result.expect_err("parent sync failure must propagate");
    assert_eq!(error.to_string(), PARENT_SYNC_ERROR);
    assert_eq!(callbacks, 1);
    assert_lock_absent(&fixture);
    assert_candidate_present(&fixture);
    assert_journal_metadata(&fixture);
}

#[test]
fn successful_reclaim_syncs_parent_before_removing_candidate() {
    let fixture = abandoned_lock_fixture();
    let mut sync_order = Vec::new();

    let reclaimed = reclaim_abandoned_lock_with_sync(
        fixture.root.as_fd(),
        fixture.root.as_fd(),
        LOCK_NAME,
        |directory| {
            let identity = fstat(directory)?;
            if same_file(&identity, &fixture.parent_identity) {
                sync_order.push("parent");
                assert_lock_absent(&fixture);
                assert_candidate_present(&fixture);
            } else {
                assert!(same_file(&identity, &fixture.journal_identity));
                sync_order.push("journal");
                assert_lock_absent(&fixture);
                assert_candidate_absent(&fixture);
            }
            sync_directory(directory)
        },
    )
    .expect("reclaim with both directory syncs");

    assert!(reclaimed, "successful reclaim must remove an existing lock");
    assert_eq!(sync_order, ["parent", "journal"]);
    assert_lock_absent(&fixture);
    assert_candidate_absent(&fixture);
    assert_journal_metadata(&fixture);
}

#[test]
fn journal_sync_failure_propagates_after_parent_sync_and_candidate_unlink() {
    let fixture = abandoned_lock_fixture();
    let mut sync_order = Vec::new();

    let result = reclaim_abandoned_lock_with_sync(
        fixture.root.as_fd(),
        fixture.root.as_fd(),
        LOCK_NAME,
        |directory| {
            let identity = fstat(directory)?;
            if same_file(&identity, &fixture.parent_identity) {
                sync_order.push("parent");
                assert_lock_absent(&fixture);
                assert_candidate_present(&fixture);
                sync_directory(directory)
            } else {
                assert!(same_file(&identity, &fixture.journal_identity));
                sync_order.push("journal");
                assert_lock_absent(&fixture);
                assert_candidate_absent(&fixture);
                Err(anyhow::anyhow!(JOURNAL_SYNC_ERROR))
            }
        },
    );

    let error = result.expect_err("journal sync failure must propagate");
    assert_eq!(error.to_string(), JOURNAL_SYNC_ERROR);
    assert_eq!(sync_order, ["parent", "journal"]);
    assert_lock_absent(&fixture);
    assert_candidate_absent(&fixture);
    assert_journal_metadata(&fixture);
}

#[test]
fn missing_conventional_lock_returns_false_without_syncing_or_removing_candidate() {
    let fixture = abandoned_lock_fixture();
    unlinkat(fixture.root.as_fd(), LOCK_NAME, UnlinkatFlags::NoRemoveDir)
        .expect("remove conventional lock before retry");
    let mut callbacks = 0;

    let reclaimed = reclaim_abandoned_lock_with_sync(
        fixture.root.as_fd(),
        fixture.root.as_fd(),
        LOCK_NAME,
        |directory| {
            callbacks += 1;
            sync_directory(directory)
        },
    )
    .expect("missing lock is an idempotent false result");

    assert!(!reclaimed);
    assert_eq!(callbacks, 0);
    assert_lock_absent(&fixture);
    assert_candidate_present(&fixture);
    assert_journal_metadata(&fixture);
}

#[test]
fn live_lease_refuses_reclaim_without_mutation_or_sync() {
    let (fixture, _live_recovery) = create_lock_fixture();
    let mut callbacks = 0;

    let error = reclaim_abandoned_lock_with_sync(
        fixture.root.as_fd(),
        fixture.root.as_fd(),
        LOCK_NAME,
        |directory| {
            callbacks += 1;
            sync_directory(directory)
        },
    )
    .expect_err("live recovery lease must refuse reclaim");

    assert!(
        error
            .to_string()
            .contains("live Jackin cleanup transaction")
    );
    assert_eq!(callbacks, 0);
    assert_lock_present(&fixture);
    assert_candidate_present(&fixture);
    assert_journal_metadata(&fixture);
}

#[test]
fn foreign_lock_is_untouched_without_syncing() {
    let (dir, root) = fixture();
    let foreign_bytes = b"foreign-lock-v1\tjournal\tlock-1\tHEAD.lock\t0\t0\n";
    std::fs::write(dir.path().join(LOCK_NAME), foreign_bytes).expect("foreign lock");
    let before = read_regular(root.as_fd(), LOCK_NAME)
        .expect("read foreign lock")
        .expect("foreign lock exists");
    let mut callbacks = 0;

    let reclaimed =
        reclaim_abandoned_lock_with_sync(root.as_fd(), root.as_fd(), LOCK_NAME, |directory| {
            callbacks += 1;
            sync_directory(directory)
        })
        .expect("foreign lock is not a Jackin recovery candidate");

    assert!(!reclaimed);
    assert_eq!(callbacks, 0);
    let after = read_regular(root.as_fd(), LOCK_NAME)
        .expect("read foreign lock after reclaim attempt")
        .expect("foreign lock remains");
    assert!(same_file(&after.identity, &before.identity));
    assert_eq!(after.bytes, before.bytes);
}

#[test]
fn candidate_identity_mismatch_refuses_reclaim_without_mutation_or_sync() {
    let fixture = abandoned_lock_fixture();
    unlinkat(
        fixture.journal.as_fd(),
        fixture.candidate_name.as_str(),
        UnlinkatFlags::NoRemoveDir,
    )
    .expect("remove original candidate link");
    let mut replacement: File = openat(
        fixture.journal.as_fd(),
        fixture.candidate_name.as_str(),
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::S_IRUSR | Mode::S_IWUSR,
    )
    .expect("create replacement candidate")
    .into();
    replacement
        .write_all(&fixture.marker)
        .expect("write replacement candidate");
    replacement.sync_all().expect("sync replacement candidate");
    let replacement_identity = fstat(&replacement).expect("stat replacement candidate");
    assert!(!same_file(
        &replacement_identity,
        &fixture.candidate_identity
    ));
    sync_directory(fixture.journal.as_fd()).expect("sync replacement journal entry");
    drop(replacement);
    let mut callbacks = 0;

    let error = reclaim_abandoned_lock_with_sync(
        fixture.root.as_fd(),
        fixture.root.as_fd(),
        LOCK_NAME,
        |directory| {
            callbacks += 1;
            sync_directory(directory)
        },
    )
    .expect_err("candidate identity mismatch must refuse reclaim");

    assert!(
        error
            .to_string()
            .contains("not the journaled Jackin lock candidate")
    );
    assert_eq!(callbacks, 0);
    assert_lock_present(&fixture);
    let candidate = read_regular(fixture.journal.as_fd(), &fixture.candidate_name)
        .expect("read replacement candidate after refusal")
        .expect("replacement candidate remains");
    assert!(same_file(&candidate.identity, &replacement_identity));
    assert_eq!(candidate.bytes, fixture.marker);
    assert_journal_metadata(&fixture);
}

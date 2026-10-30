// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn state_read_is_noncreating_and_publish_reopens_private_bounded_leaf() {
    use std::io::Write as _;
    use std::os::unix::fs::MetadataExt as _;

    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let directory = universe_dir(&paths).unwrap();
    let key = "apple-usage-relay-first";

    assert_eq!(
        read_state_bytes_in_namespace(&directory, key, 64).unwrap(),
        None
    );
    assert!(!paths.home_dir.exists(), "read must not create state roots");

    let lock = open_in_namespace(&directory, key).unwrap();
    lock.try_lock().unwrap();
    publish_state_bytes_in_namespace(&directory, key, b"complete-state", 64).unwrap();

    assert_eq!(
        read_state_bytes_in_namespace(&directory, key, 64).unwrap(),
        Some(b"complete-state".to_vec())
    );
    let leaf = directory.join(format!("{key}.json"));
    let metadata = std::fs::symlink_metadata(&leaf).unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.uid(), nix::unistd::geteuid().as_raw());
    assert_eq!(metadata.mode() & 0o777, 0o600);
    assert_eq!(metadata.nlink(), 1);

    assert!(matches!(
        publish_state_bytes_in_namespace(&directory, "oversized", b"012345678", 8),
        Err(StatePublishError::NotPublished(_))
    ));
    assert!(!directory.join("oversized.json").exists());
    let mut oversized = open_state_in_namespace(&directory, "oversized.json", true).unwrap();
    oversized.write_all(b"012345678").unwrap();
    assert_eq!(
        read_state_bytes_in_namespace(&directory, "oversized", 8)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn oversized_publish_does_not_create_an_absent_coordination_namespace() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.home_dir).unwrap();
    let namespace = paths.home_dir.join(".jackin-coordination");
    assert!(!namespace.exists());
    let directory = universe_dir(&paths).unwrap();

    let result = publish_state_bytes_in_namespace(&directory, "oversized-absent", b"012345678", 8);
    assert!(!namespace.exists(), "rejection must leave namespace absent");
    assert!(
        !directory.exists(),
        "rejection must leave the universe absent"
    );
    let error = result.expect_err("oversized state must be rejected");
    let StatePublishError::NotPublished(error) = error else {
        panic!("oversized state must not be published: {error:?}");
    };
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert_eq!(
        error.to_string(),
        "coordination state exceeds the configured size limit"
    );
}

#[test]
fn pre_rename_failure_preserves_old_inode_bytes_and_removes_only_its_temp() {
    use std::io::{Read as _, Write as _};
    use std::os::unix::fs::MetadataExt as _;

    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let directory = universe_dir(&paths).unwrap();
    let key = "apple-usage-relay-pre-rename";
    let lock = open_in_namespace(&directory, key).unwrap();
    lock.try_lock().unwrap();
    publish_state_bytes_in_namespace(&directory, key, b"old-complete", 64).unwrap();
    let leaf = directory.join(format!("{key}.json"));
    let before = std::fs::symlink_metadata(&leaf).unwrap();
    let sentinel_name = ".jackin-state-unrelated-sentinel";
    let sentinel_path = directory.join(sentinel_name);
    let mut sentinel = open_state_in_namespace(&directory, sentinel_name, true).unwrap();
    sentinel.write_all(b"unrelated staged sentinel").unwrap();
    sentinel.sync_all().unwrap();
    let sentinel_metadata = sentinel.metadata().unwrap();
    let sentinel_identity = (sentinel_metadata.dev(), sentinel_metadata.ino());
    drop(sentinel);

    let result = publish_state_bytes_with_faults(
        &directory,
        key,
        b"new-complete",
        64,
        PublishTestFaults {
            failure: Some(PublishTestFailure::TempSync),
        },
    );
    assert!(matches!(result, Err(StatePublishError::NotPublished(_))));

    let after = std::fs::symlink_metadata(&leaf).unwrap();
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    assert_eq!(
        read_state_bytes_in_namespace(&directory, key, 64).unwrap(),
        Some(b"old-complete".to_vec())
    );
    let staged_names = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".jackin-state-"))
        .collect::<Vec<_>>();
    assert_eq!(
        staged_names,
        vec![sentinel_name.to_owned()],
        "only this publication's staged inode must be cleaned"
    );
    let sentinel_after = std::fs::symlink_metadata(&sentinel_path).unwrap();
    assert_eq!(
        (sentinel_after.dev(), sentinel_after.ino()),
        sentinel_identity,
        "unrelated staged inode must remain unchanged"
    );
    assert_eq!(
        std::fs::read(&sentinel_path).unwrap(),
        b"unrelated staged sentinel"
    );

    let mut file = open_state_in_namespace(&directory, &format!("{key}.json"), false).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"old-complete");
}

#[test]
fn post_rename_failures_report_uncertainty_and_leave_complete_new_bytes_visible() {
    use std::os::unix::fs::MetadataExt as _;

    for (key, faults) in [
        (
            "apple-usage-relay-directory-sync",
            PublishTestFaults {
                failure: Some(PublishTestFailure::DirectorySync),
            },
        ),
        (
            "apple-usage-relay-final-readback",
            PublishTestFaults {
                failure: Some(PublishTestFailure::FinalReadback),
            },
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let directory = universe_dir(&paths).unwrap();
        let lock = open_in_namespace(&directory, key).unwrap();
        lock.try_lock().unwrap();
        publish_state_bytes_in_namespace(&directory, key, b"old-complete", 64).unwrap();
        let leaf = directory.join(format!("{key}.json"));
        let old_file = open_state_in_namespace(&directory, &format!("{key}.json"), false).unwrap();
        let old = std::fs::symlink_metadata(&leaf).unwrap();

        let result = publish_state_bytes_with_faults(&directory, key, b"new-complete", 64, faults);
        assert!(matches!(
            result,
            Err(StatePublishError::VisibleButUncertain(_))
        ));
        let new = std::fs::symlink_metadata(&leaf).unwrap();
        assert_ne!((old.dev(), old.ino()), (new.dev(), new.ino()));
        assert_eq!(
            read_state_bytes_in_namespace(&directory, key, 64).unwrap(),
            Some(b"new-complete".to_vec()),
            "post-rename uncertainty must not roll back or delete the complete new leaf"
        );
        drop(old_file);
    }
}

#[test]
fn replacing_json_keeps_the_independent_lock_inode_exclusive() {
    use std::os::unix::fs::MetadataExt as _;

    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let directory = universe_dir(&paths).unwrap();
    let key = "apple-usage-relay-stable-lock";
    let first_lock = open_in_namespace(&directory, key).unwrap();
    let second_lock = open_in_namespace(&directory, key).unwrap();
    first_lock.try_lock().unwrap();
    assert!(matches!(
        second_lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    let identity = first_lock.metadata().unwrap();
    assert_eq!(
        (identity.dev(), identity.ino()),
        (
            second_lock.metadata().unwrap().dev(),
            second_lock.metadata().unwrap().ino()
        )
    );

    publish_state_bytes_in_namespace(&directory, key, b"first", 64).unwrap();
    let first_json = std::fs::symlink_metadata(directory.join(format!("{key}.json"))).unwrap();
    let first_json_file =
        open_state_in_namespace(&directory, &format!("{key}.json"), false).unwrap();
    publish_state_bytes_in_namespace(&directory, key, b"second", 64).unwrap();
    let second_json = std::fs::symlink_metadata(directory.join(format!("{key}.json"))).unwrap();
    assert_ne!(
        (first_json.dev(), first_json.ino()),
        (second_json.dev(), second_json.ino())
    );

    let lock_after = std::fs::symlink_metadata(directory.join(format!("{key}.lock"))).unwrap();
    assert_eq!(
        (identity.dev(), identity.ino()),
        (lock_after.dev(), lock_after.ino())
    );
    assert!(
        matches!(
            second_lock.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ),
        "JSON replacement must not release the independent stable lock"
    );
    first_lock.unlock().unwrap();
    second_lock.try_lock().unwrap();
    second_lock.unlock().unwrap();
    drop(first_json_file);
}

#[test]
fn unsafe_destination_inodes_are_preserved_and_refused() {
    use std::io::Write as _;
    use std::os::unix::fs::{PermissionsExt as _, symlink};

    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let directory = universe_dir(&paths).unwrap();
    let target = temp.path().join("target.json");
    std::fs::write(&target, b"keep this target").unwrap();
    let symlink_lock = open_in_namespace(&directory, "apple-usage-relay-symlink").unwrap();
    symlink_lock.try_lock().unwrap();
    let symlink_leaf = directory.join("apple-usage-relay-symlink.json");
    symlink(&target, &symlink_leaf).unwrap();
    assert!(matches!(
        publish_state_bytes_in_namespace(
            &directory,
            "apple-usage-relay-symlink",
            b"replacement",
            64
        ),
        Err(StatePublishError::NotPublished(_))
    ));
    assert!(
        std::fs::symlink_metadata(&symlink_leaf)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"keep this target");

    let linked_target = temp.path().join("linked-target.json");
    std::fs::write(&linked_target, b"keep hardlink target").unwrap();
    let hardlink_lock = open_in_namespace(&directory, "apple-usage-relay-hardlink").unwrap();
    hardlink_lock.try_lock().unwrap();
    let hardlink_leaf = directory.join("apple-usage-relay-hardlink.json");
    std::fs::hard_link(&linked_target, &hardlink_leaf).unwrap();
    assert!(matches!(
        publish_state_bytes_in_namespace(
            &directory,
            "apple-usage-relay-hardlink",
            b"replacement",
            64
        ),
        Err(StatePublishError::NotPublished(_))
    ));
    assert_eq!(
        std::fs::read(&linked_target).unwrap(),
        b"keep hardlink target"
    );
    assert_eq!(
        std::fs::read(&hardlink_leaf).unwrap(),
        b"keep hardlink target"
    );

    let loose_key = "apple-usage-relay-loose-mode";
    let loose_lock = open_in_namespace(&directory, loose_key).unwrap();
    loose_lock.try_lock().unwrap();
    let mut loose =
        open_state_in_namespace(&directory, &format!("{loose_key}.json"), true).unwrap();
    loose.write_all(b"private old").unwrap();
    drop(loose);
    std::fs::set_permissions(
        directory.join(format!("{loose_key}.json")),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(matches!(
        publish_state_bytes_in_namespace(&directory, loose_key, b"replacement", 64),
        Err(StatePublishError::NotPublished(_))
    ));
    assert_eq!(
        std::fs::read(directory.join(format!("{loose_key}.json"))).unwrap(),
        b"private old"
    );

    let fifo_lock = open_in_namespace(&directory, "apple-usage-relay-fifo").unwrap();
    fifo_lock.try_lock().unwrap();
    let fifo_leaf = directory.join("apple-usage-relay-fifo.json");
    nix::unistd::mkfifo(&fifo_leaf, nix::sys::stat::Mode::from_bits_truncate(0o600)).unwrap();
    assert!(matches!(
        publish_state_bytes_in_namespace(&directory, "apple-usage-relay-fifo", b"replacement", 64),
        Err(StatePublishError::NotPublished(_))
    ));
    assert!(fifo_leaf.exists(), "unsafe FIFO must not be replaced");
}

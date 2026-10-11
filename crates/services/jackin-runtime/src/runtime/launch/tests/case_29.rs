// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Name-lock ownership suite (relocated from
//! `launch_slot/tests/case_01` by S7 split 77).

#![cfg(unix)]

use super::*;

use std::io::{BufRead as _, Write as _};

use std::os::unix::fs::MetadataExt as _;

#[test]
#[ignore = "child fixture invoked explicitly by the parent scenario"]
fn name_lock_child() {
    let root = std::env::var_os(CHILD_ROOT).expect("child fixture requires isolated root");
    let paths = jackin_core::JackinPaths::for_tests(std::path::Path::new(&root));
    let lock_path = crate::runtime::coordination::root(&paths)
        .unwrap()
        .join(format!("name-{NAME}.lock"));
    let mut held = None;
    for command in std::io::stdin().lock().lines() {
        assert_eq!(command.unwrap(), "attempt");
        assert!(
            held.is_none(),
            "an owner must not attempt a second acquisition"
        );
        let acquired = match try_acquire_name_lock(&paths, NAME) {
            Ok(lock) => {
                held = Some(lock);
                true
            }
            Err(error) => {
                assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
                false
            }
        };
        let metadata = held
            .as_ref()
            .map(|lock| lock.metadata().unwrap())
            .or_else(|| std::fs::metadata(&lock_path).ok());
        let (device, inode) = metadata.map_or((0, 0), |meta| (meta.dev(), meta.ino()));
        println!(
            "{REPORT}{} {} {device} {inode}",
            std::process::id(),
            if acquired { "acquired" } else { "blocked" },
        );
        std::io::stdout().flush().unwrap();
    }
    // Dropping ownership must release flock without deleting the pathname.
    drop(held);
}

#[test]
fn three_process_contenders_preserve_lock_inode_until_and_after_owner_exit() {
    let temp = canonical_tempdir().unwrap();
    let paths = jackin_core::JackinPaths::for_tests(temp.path());
    let lock_path = crate::runtime::coordination::root(&paths)
        .unwrap()
        .join(format!("name-{NAME}.lock"));
    let mut owner = Contender::spawn(temp.path());
    let first = owner.attempt();
    assert!(first.acquired, "first process must own the slot");
    let mut failed = Contender::spawn(temp.path());
    let second = failed.attempt();
    assert!(
        !second.acquired,
        "second process must encounter owner contention"
    );
    let mut third = Contender::spawn(temp.path());
    let blocked = third.attempt();
    failed.exit();
    assert!(
        !blocked.acquired,
        "third process acquired a replacement inode while the first owner still held flock: {first:?}, {second:?}, {blocked:?}"
    );
    assert_ne!(first.pid, second.pid);
    assert_ne!(first.pid, blocked.pid);
    assert_ne!(second.pid, blocked.pid);
    assert_eq!(
        second.inode, first.inode,
        "failed contender must preserve owner inode"
    );
    assert_eq!(
        blocked.inode, first.inode,
        "third contender must observe owner inode"
    );
    let metadata = std::fs::metadata(&lock_path).unwrap();
    assert_eq!((metadata.dev(), metadata.ino()), first.inode);
    owner.exit();
    let reclaimed = third.attempt();
    assert!(reclaimed.acquired, "owner exit must release flock");
    assert_eq!(reclaimed.pid, blocked.pid);
    assert_eq!(
        reclaimed.inode, first.inode,
        "reclaim must lock the original inode"
    );
    third.exit();
    let metadata = std::fs::metadata(lock_path).expect("owner drop must preserve lock file");
    assert_eq!((metadata.dev(), metadata.ino()), first.inode);
}

#[tokio::test]
async fn prune_paths_preserve_three_process_name_lock_ownership() {
    use jackin_test_support::{FakeDockerClient, FakeRunner};
    for operation in ["container", "instances", "all-instances", "home"] {
        let temp = canonical_tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(temp.path());
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let lock_path = crate::runtime::coordination::root(&paths)
            .unwrap()
            .join(format!("name-{NAME}.lock"));
        let mut owner = Contender::spawn(temp.path());
        let first = owner.attempt();
        assert!(first.acquired);
        let mut failed = Contender::spawn(temp.path());
        let second = failed.attempt();
        assert!(!second.acquired);
        let docker = FakeDockerClient::default();
        let mut runner = FakeRunner::default();
        match operation {
            "container" => {
                crate::runtime::cleanup::purge_container_state(&paths, NAME, &docker, &mut runner)
                    .await
                    .unwrap()
            }
            "instances" => {
                crate::runtime::cleanup::prune_instances(&paths, &docker, &mut runner)
                    .await
                    .unwrap();
            }
            "all-instances" => {
                crate::runtime::cleanup::prune_all_instances(&paths, &docker, &mut runner)
                    .await
                    .unwrap();
            }
            "home" => crate::runtime::cleanup::prune_jackin_home(&paths).unwrap(),
            _ => unreachable!(),
        }
        let mut third = Contender::spawn(temp.path());
        let blocked = third.attempt();
        failed.exit();
        assert!(
            !blocked.acquired,
            "{operation} split the inode while owner still held flock"
        );
        assert_ne!(first.pid, second.pid);
        assert_ne!(first.pid, blocked.pid);
        assert_ne!(second.pid, blocked.pid);
        assert_eq!(second.inode, first.inode);
        assert_eq!(
            blocked.inode, first.inode,
            "{operation} changed coordination identity"
        );
        owner.exit();
        let reclaimed = third.attempt();
        assert!(reclaimed.acquired);
        assert_eq!(reclaimed.inode, first.inode);
        third.exit();
        let metadata = std::fs::metadata(lock_path).unwrap();
        assert_eq!((metadata.dev(), metadata.ino()), first.inode);
    }
}

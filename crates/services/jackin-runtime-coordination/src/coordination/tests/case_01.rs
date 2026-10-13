// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn resolution_accepts_real_directory_created_after_canonicalization_miss() {
    for missing_tail in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let raced = temp.path().join("created-between-syscalls");
        let selected = if missing_tail {
            raced.join("still-missing")
        } else {
            raced.clone()
        };
        let mut retry_calls = 0;
        let resolved = resolve_with(&selected, |ancestor| {
            canonicalize_with_appearing_directory(ancestor, &raced, &mut retry_calls)
        })
        .unwrap();
        let expected = std::fs::canonicalize(&raced).unwrap();
        assert_eq!(
            resolved,
            if missing_tail {
                expected.join("still-missing")
            } else {
                expected
            }
        );
        assert_eq!(
            retry_calls, 2,
            "one bounded retry for the appeared real inode"
        );
        assert!(!raced.join("still-missing").exists());
    }
}

#[test]
fn resolution_rejects_dangling_symlink_without_retry_or_creation() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("absent-target");
    let link = temp.path().join("dangling-link");
    symlink(&target, &link).unwrap();
    let mut calls = 0;
    let error = resolve_with(&link, |ancestor| {
        calls += 1;
        std::fs::canonicalize(ancestor)
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert_eq!(calls, 1);
    assert!(!target.exists());
    assert!(
        std::fs::symlink_metadata(link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn concurrent_first_lock_opens_all_share_one_inode() {
    use std::sync::{Arc, Barrier};
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let directory = universe_dir(&paths).unwrap();
    drop(open_directory_in_namespace(&directory, true).unwrap());
    for round in 0..40 {
        let barrier = Arc::new(Barrier::new(8));
        let key = format!("concurrent-first-{round}");
        let files = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..8)
                .map(|_| {
                    let barrier = Arc::clone(&barrier);
                    let directory = &directory;
                    let key = &key;
                    scope.spawn(move || open_after_barrier(&barrier, directory, key))
                })
                .collect();
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap().unwrap())
                .collect::<Vec<_>>()
        });
        let first = files[0].metadata().unwrap();
        for file in &files {
            let metadata = file.metadata().unwrap();
            assert_eq!((metadata.dev(), metadata.ino()), (first.dev(), first.ino()));
        }
        let pathname = directory.join(format!("{key}.lock"));
        let metadata = std::fs::symlink_metadata(pathname).unwrap();
        assert_eq!((metadata.dev(), metadata.ino()), (first.dev(), first.ino()));
    }
}

#[test]
fn existing_only_namespace_and_state_opens_create_nothing_and_never_truncate() {
    use std::io::{Read as _, Write as _};
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let directory = universe_dir(&paths).unwrap();
    assert_eq!(
        open_directory_in_namespace(&directory, false)
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(
        open_state_in_namespace(&directory, "generation", true)
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    assert!(
        !paths.home_dir.exists(),
        "existing-only parent open must not create fixture state"
    );
    drop(open_in_namespace(&directory, "universe-lock").unwrap());
    let mut file = open_state_in_namespace(&directory, "generation", true).unwrap();
    file.write_all(b"retained generation").unwrap();
    let first = file.metadata().unwrap();
    drop(file);
    let mut reopened = open_state_in_namespace(&directory, "generation", false).unwrap();
    let second = reopened.metadata().unwrap();
    let mut contents = String::new();
    reopened.read_to_string(&mut contents).unwrap();
    assert_eq!(
        contents, "retained generation",
        "opening validated state must not truncate it"
    );
    assert_eq!((first.dev(), first.ino()), (second.dev(), second.ino()));
    assert_eq!(
        open_state_in_namespace(&directory, "missing", false)
            .unwrap_err()
            .kind(),
        io::ErrorKind::NotFound
    );
    assert!(!directory.join("missing").exists());
}

#[test]
fn literal_auxiliary_state_keys_refuse_symlinks_before_any_write() {
    use std::io::Write as _;
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let directory = universe_dir(&paths).unwrap();
    drop(open_in_namespace(&directory, "universe-lock").unwrap());
    let outside = temp.path().join("outside-state");
    std::fs::write(&outside, b"must stay intact").unwrap();
    for key in ["universe-generation", "universe-since"] {
        symlink(&outside, directory.join(key)).unwrap();
        for create in [false, true] {
            assert_eq!(
                open_state_in_namespace(&directory, key, create)
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::PermissionDenied
            );
        }
    }
    assert_eq!(std::fs::read(&outside).unwrap(), b"must stay intact");
    let mut regular = open_state_in_namespace(&directory, "private-state", true).unwrap();
    regular.write_all(b"approved state").unwrap();
    assert_eq!(regular.metadata().unwrap().mode() & 0o077, 0);
    for key in ["", ".", "..", "../outside-state", "nested/state"] {
        assert_eq!(
            open_state_in_namespace(&directory, key, true)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}

#[test]
fn universe_identity_survives_deleted_data_and_isolates_distinct_roots() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let missing = universe_dir(&paths).unwrap();
    assert!(
        !paths.home_dir.exists(),
        "path derivation must not create state"
    );
    std::fs::create_dir_all(&paths.data_dir).unwrap();
    assert_eq!(universe_dir(&paths).unwrap(), missing);
    std::fs::remove_dir_all(&paths.data_dir).unwrap();
    assert_eq!(universe_dir(&paths).unwrap(), missing);
    let mut other = paths.clone();
    other.data_dir = paths.home_dir.join("other-data");
    assert_ne!(universe_dir(&other).unwrap(), missing);
}

#[test]
fn permanent_lock_rejects_symlinks_and_escaping_keys() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let file = open_lock(&paths, "original").unwrap();
    let identity = file.metadata().unwrap();
    drop(file);
    let file = open_lock(&paths, "original").unwrap();
    let reopened = file.metadata().unwrap();
    assert_eq!(
        (identity.dev(), identity.ino()),
        (reopened.dev(), reopened.ino())
    );
    for key in ["", ".", "..", "../outside", "nested/key"] {
        assert_eq!(
            open_lock(&paths, key).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    let outside = temp.path().join("outside");
    std::fs::write(&outside, b"retain").unwrap();
    symlink(&outside, root(&paths).unwrap().join("symlink.lock")).unwrap();
    open_lock(&paths, "symlink").unwrap_err();
    assert_eq!(std::fs::read(outside).unwrap(), b"retain");
}

#[test]
fn prune_overlap_rejects_namespace_descendants_and_custom_runtime_layouts() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    drop(open_lock(&paths, "retained").unwrap());
    let namespace = root(&paths).unwrap();
    for target in [
        namespace.clone(),
        namespace.join("universes"),
        namespace.join("universes/custom-root"),
    ] {
        assert_eq!(
            ensure_prunable(&paths, &target).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied,
            "namespace descendant cannot be a deletion target"
        );
    }
    for field in ["data", "home", "roles", "cache"] {
        let mut custom = paths.clone();
        let target = namespace.join(field);
        match field {
            "data" => custom.data_dir = target,
            "home" => custom.jackin_home = target,
            "roles" => custom.roles_dir = target,
            "cache" => custom.cache_dir = target,
            _ => unreachable!(),
        }
        assert_eq!(
            root(&custom).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied,
            "custom {field} layout must not enter the coordination namespace"
        );
    }
    assert!(namespace.join("retained.lock").exists());
}

#[test]
fn prune_overlap_rejects_canonical_aliases_and_ambiguous_missing_tails() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.home_dir).unwrap();
    let alias = temp.path().join("home-alias");
    symlink(&paths.home_dir, &alias).unwrap();
    assert_eq!(
        ensure_prunable(&paths, &alias).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    let dangling = temp.path().join("dangling");
    symlink(temp.path().join("absent"), &dangling).unwrap();
    ensure_prunable(&paths, &dangling).unwrap_err();
    ensure_prunable(&paths, &temp.path().join("missing/../home")).unwrap_err();
    ensure_prunable(&paths, &paths.jackin_home).unwrap();
}

#[test]
fn namespace_open_rejects_writable_ancestors_and_fifo_before_opening() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let scope = universe_dir(&paths).unwrap();
    drop(open_in_namespace(&scope, "owner").unwrap());
    let universe_parent = scope.parent().unwrap();
    std::fs::set_permissions(universe_parent, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert_eq!(
        open_in_namespace(&scope, "owner").unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    std::fs::set_permissions(universe_parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    let namespace = root(&paths).unwrap();
    std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert_eq!(
        open_in_namespace(&scope, "owner").unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o700)).unwrap();
    let fifo = namespace.join("fifo.lock");
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::from_bits_truncate(0o600)).unwrap();
    assert_eq!(
        open_lock(&paths, "fifo").unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert!(
        fifo.exists(),
        "failed preflight must preserve the rejected inode"
    );
}

#[test]
fn namespace_boundary_ignores_matching_basename_above_operator_home() {
    let temp = tempfile::tempdir().unwrap();
    let ancestor = temp.path().join(DIRECTORY);
    std::fs::create_dir_all(&ancestor).unwrap();
    std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o777)).unwrap();
    let mut paths = JackinPaths::for_tests(temp.path());
    paths.home_dir = ancestor.join("users/operator-home");
    let file = open_lock(&paths, "owner").unwrap();
    assert!(file.metadata().unwrap().is_file());
    assert!(
        root(&paths)
            .unwrap()
            .starts_with(resolve(&paths.home_dir).unwrap())
    );
}

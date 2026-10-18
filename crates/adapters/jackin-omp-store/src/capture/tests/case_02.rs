// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::directory::{open_source_directory_with_hook, same_object_and_security};
use super::*;
use nix::sys::stat::{SFlag, fstat, stat};
use std::os::unix::fs::{PermissionsExt, symlink};

fn fixture() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    Connection,
) {
    let temp = tempdir().unwrap();
    let ancestor = temp.path().join("ancestor");
    fs::create_dir(&ancestor).unwrap();
    let (source, writer) = create_source(&ancestor, true);
    let ancestor = fs::canonicalize(ancestor).unwrap();
    let source = fs::canonicalize(source).unwrap();
    (temp, ancestor, source, writer)
}

#[test]
fn ancestor_sibling_churn_preserves_production_capture() {
    let (_temp, ancestor, source, _writer) = fixture();
    let mut changed = false;
    let root = open_source_directory_with_hook(&source, deadline(), |path| {
        if path == ancestor {
            let before = stat(path).unwrap();
            fs::create_dir(path.join("unrelated-sibling")).unwrap();
            let after = stat(path).unwrap();
            assert_ne!(
                super::super::FileIdentity::from_stat(&before),
                super::super::FileIdentity::from_stat(&after)
            );
            assert!(same_object_and_security(&before, &after));
            changed = true;
        }
    })
    .unwrap()
    .unwrap();
    assert!(changed);
    let mut snapshot = OmpSnapshot::capture_from_root(&root).unwrap().unwrap();
    let selected = snapshot
        .select(Some("openai"), Some(&selector(41)))
        .unwrap();
    let mut output = zeroize::Zeroizing::new(Vec::new());
    selected.write_standalone_database(&mut *output).unwrap();
    assert!(
        output
            .windows(b"selected-old-canary".len())
            .any(|bytes| bytes == b"selected-old-canary")
    );
}

#[test]
fn ancestor_replacement_symlinks_and_permission_changes_are_rejected() {
    for mutation in ["permissions", "replacement", "symlink"] {
        let (_temp, ancestor, source, _writer) = fixture();
        let mut changed = false;
        let result = open_source_directory_with_hook(&source, deadline(), |path| {
            if path != ancestor {
                return;
            }
            changed = true;
            if mutation == "permissions" {
                let mode = fs::metadata(path).unwrap().permissions().mode();
                fs::set_permissions(path, fs::Permissions::from_mode(mode ^ 0o010)).unwrap();
                return;
            }
            let original = path.with_file_name("original-ancestor");
            fs::rename(path, &original).unwrap();
            if mutation == "symlink" {
                symlink(original, path).unwrap();
            } else {
                fs::create_dir(path).unwrap();
            }
        });
        assert!(changed);
        assert!(matches!(result, Err(OmpError::Unavailable)));
    }
}

#[test]
fn source_root_child_mutation_still_rejects_full_identity_change() {
    let (_temp, _ancestor, source, _writer) = fixture();
    let mut changed = false;
    let result = open_source_directory_with_hook(&source, deadline(), |path| {
        if path == source {
            fs::create_dir(path.join("unexpected-source-child")).unwrap();
            changed = true;
        }
    });
    assert!(changed);
    assert!(matches!(result, Err(OmpError::Unavailable)));
}

#[test]
fn stable_directory_identity_rejects_every_object_and_security_field_change() {
    let (_temp, _ancestor, source, _writer) = fixture();
    let root = open_source_directory_with_hook(&source, deadline(), |_| {})
        .unwrap()
        .unwrap();
    let expected = fstat(&root).unwrap();
    for field in 0..6 {
        let mut actual = expected;
        match field {
            0 => actual.st_dev ^= 1,
            1 => actual.st_ino ^= 1,
            2 => actual.st_uid ^= 1,
            3 => actual.st_gid ^= 1,
            4 => actual.st_mode ^= 0o010,
            _ => actual.st_mode = (actual.st_mode & !SFlag::S_IFMT.bits()) | SFlag::S_IFREG.bits(),
        }
        assert!(!same_object_and_security(&expected, &actual));
    }
}

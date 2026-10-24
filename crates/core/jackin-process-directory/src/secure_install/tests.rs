// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(any(target_os = "linux", target_os = "macos"))]

use super::*;
use rustix::fs::{Mode, OFlags, open};
use rustix::io::{Errno, FdFlags, fcntl_getfd};
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;

#[cfg(target_os = "macos")]
use super::darwin_acl::{
    ACL_MAX_ENTRIES, EntryStatus, FixtureAcl, classify_entry_result, classify_getter_result,
    classify_zero_result, set_test_acl, validate_acl_entry, validate_maximal_mask,
};
#[cfg(target_os = "linux")]
use super::linux_acl::{XattrProbe, classify_probe, set_extended_acl_fixture};
#[cfg(target_os = "linux")]
use rustix::fs::mkfifoat;

const FIXTURE_DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

fn fixture_root() -> (tempfile::TempDir, CheckedInstallRoot) {
    let temporary = tempfile::tempdir().unwrap();
    set_mode(temporary.path(), 0o700);
    let descriptor = open(temporary.path(), FIXTURE_DIRECTORY_FLAGS, Mode::empty()).unwrap();
    let root = CheckedInstallRoot::from_pinned_directory(File::from(descriptor)).unwrap();
    (temporary, root)
}

fn open_fixture_directory(path: &Path) -> File {
    File::from(open(path, FIXTURE_DIRECTORY_FLAGS, Mode::empty()).unwrap())
}

fn create_private_directory(path: &Path) {
    fs::create_dir(path).unwrap();
    set_mode(path, 0o700);
}

fn write_fixture_file(path: &Path, bytes: &[u8], mode: u32) {
    fs::write(path, bytes).unwrap();
    set_mode(path, mode);
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

fn read_checked(file: &CheckedInstallFile) -> Vec<u8> {
    let mut bytes = Vec::new();
    file.file().read_to_end(&mut bytes).unwrap();
    bytes
}

fn result_error_kind<T>(result: io::Result<T>) -> ErrorKind {
    match result {
        Ok(_) => panic!("operation unexpectedly succeeded"),
        Err(error) => error.kind(),
    }
}

fn assert_checked_descriptor(file: &CheckedInstallFile) {
    assert!(
        file.file().as_raw_fd() > 2,
        "descriptor must not alias stdio"
    );
    assert!(
        fcntl_getfd(file.file()).unwrap().contains(FdFlags::CLOEXEC),
        "checked file descriptor must be close-on-exec",
    );
}

#[test]
fn owned_root_relative_leaf_is_checked_and_pinned() {
    let (temporary, root) = fixture_root();
    assert!(root.directory.as_raw_fd() > 2);
    assert!(
        fcntl_getfd(&root.directory)
            .unwrap()
            .contains(FdFlags::CLOEXEC)
    );
    for directory in ["bin", "receipts", "scripts", "sealed"] {
        create_private_directory(&temporary.path().join(directory));
    }
    let fixtures = [
        (
            "bin/owner",
            InstallFileKind::Executable,
            0o755,
            b"executable".as_slice(),
        ),
        (
            "receipts/build",
            InstallFileKind::Receipt,
            0o600,
            b"receipt".as_slice(),
        ),
        (
            "scripts/install",
            InstallFileKind::Script,
            0o400,
            b"script".as_slice(),
        ),
        (
            "sealed/image",
            InstallFileKind::SealedCopy,
            0o500,
            b"sealed".as_slice(),
        ),
    ];

    for (relative, kind, mode, expected) in fixtures {
        write_fixture_file(&temporary.path().join(relative), expected, mode);
        let checked = root.open_file(Path::new(relative), kind).unwrap();
        assert_checked_descriptor(&checked);
        assert_eq!(read_checked(&checked), expected);
        checked.recheck().unwrap();
    }
}

#[test]
fn lexical_escape_and_symlink_components_are_rejected() {
    let (temporary, root) = fixture_root();
    create_private_directory(&temporary.path().join("real"));
    write_fixture_file(&temporary.path().join("real/owner"), b"fixed", 0o755);
    symlink("real", temporary.path().join("linked-directory")).unwrap();
    symlink("real/owner", temporary.path().join("linked-file")).unwrap();

    assert_eq!(
        result_error_kind(root.open_file(Path::new("../outside"), InstallFileKind::Executable)),
        ErrorKind::InvalidInput,
    );
    assert_eq!(
        result_error_kind(root.open_file(Path::new(""), InstallFileKind::Executable)),
        ErrorKind::InvalidInput,
    );
    assert_eq!(
        result_error_kind(root.open_file(temporary.path(), InstallFileKind::Executable)),
        ErrorKind::InvalidInput,
    );
    assert_eq!(
        result_error_kind(root.open_file(
            Path::new("linked-directory/owner"),
            InstallFileKind::Executable,
        )),
        ErrorKind::PermissionDenied,
    );
    assert_eq!(
        result_error_kind(root.open_file(Path::new("linked-file"), InstallFileKind::Executable)),
        ErrorKind::PermissionDenied,
    );
    assert!(
        fs::symlink_metadata(temporary.path().join("linked-file"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let original = root
        .open_file(Path::new("real/owner"), InstallFileKind::Executable)
        .unwrap();
    assert_eq!(read_checked(&original), b"fixed");
}

#[test]
fn unsafe_owner_mode_metadata_is_rejected() {
    let (temporary, _) = fixture_root();
    set_mode(temporary.path(), 0o750);
    let descriptor = open_fixture_directory(temporary.path());
    assert_eq!(
        result_error_kind(CheckedInstallRoot::from_pinned_directory(descriptor)),
        ErrorKind::PermissionDenied,
    );

    let (temporary, root) = fixture_root();
    let writable_parent = temporary.path().join("writable");
    create_private_directory(&writable_parent);
    set_mode(&writable_parent, 0o770);
    write_fixture_file(&writable_parent.join("owner"), b"fixed", 0o755);
    assert_eq!(
        result_error_kind(
            root.open_file(Path::new("writable/owner"), InstallFileKind::Executable,)
        ),
        ErrorKind::PermissionDenied,
    );
    write_fixture_file(&temporary.path().join("wrong-mode"), b"fixed", 0o644);
    assert_eq!(
        result_error_kind(root.open_file(Path::new("wrong-mode"), InstallFileKind::Executable)),
        ErrorKind::PermissionDenied,
    );

    let effective_uid = geteuid().as_raw();
    let foreign_uid = u32::from(effective_uid == 0);
    let foreign_owner = MetadataFacts {
        file_type: FileType::Directory,
        mode: Mode::from_bits_truncate(0o700),
        uid: foreign_uid,
        single_link: true,
    };
    assert_eq!(
        classify_metadata(foreign_owner, effective_uid, NodeRole::Root),
        Err(MetadataProblem::WrongOwner),
    );
    for mode in [0o720, 0o702] {
        let writable_directory = MetadataFacts {
            uid: effective_uid,
            mode: Mode::from_bits_truncate(mode),
            ..foreign_owner
        };
        assert_eq!(
            classify_metadata(writable_directory, effective_uid, NodeRole::Directory),
            Err(MetadataProblem::WritableByGroupOrOther),
        );
    }
}

#[test]
fn hardlink_and_nonregular_leaf_are_rejected() {
    let (temporary, root) = fixture_root();
    assert!(LEAF_OPEN_FLAGS.contains(OFlags::NONBLOCK));
    assert!(DIRECTORY_OPEN_FLAGS.contains(OFlags::NONBLOCK));
    write_fixture_file(&temporary.path().join("linked"), b"fixed", 0o755);
    fs::hard_link(
        temporary.path().join("linked"),
        temporary.path().join("alias"),
    )
    .unwrap();
    assert_eq!(
        result_error_kind(root.open_file(Path::new("linked"), InstallFileKind::Executable)),
        ErrorKind::PermissionDenied,
    );
    create_private_directory(&temporary.path().join("directory-leaf"));
    assert_eq!(
        result_error_kind(
            root.open_file(Path::new("directory-leaf"), InstallFileKind::Executable,)
        ),
        ErrorKind::PermissionDenied,
    );
    #[cfg(target_os = "linux")]
    {
        mkfifoat(
            &root.directory,
            "fifo-leaf",
            Mode::from_bits_truncate(0o600),
        )
        .unwrap();
        assert_eq!(
            result_error_kind(root.open_file(Path::new("fifo-leaf"), InstallFileKind::Executable)),
            ErrorKind::PermissionDenied,
        );
    }
}

#[test]
fn pinned_parent_rebind_keeps_original_namespace() {
    let (temporary, root) = fixture_root();
    let selected = temporary.path().join("selected");
    create_private_directory(&selected);
    write_fixture_file(&selected.join("owner"), b"original-parent", 0o755);
    let lookup = open_parent_directory(&root.directory, Path::new("selected/owner")).unwrap();

    fs::rename(&selected, temporary.path().join("moved")).unwrap();
    create_private_directory(&selected);
    write_fixture_file(&selected.join("owner"), b"replacement-parent", 0o755);
    let checked = open_checked_leaf(
        lookup.parent,
        lookup.role,
        lookup.leaf_name,
        InstallFileKind::Executable,
    )
    .unwrap();
    assert_eq!(read_checked(&checked), b"original-parent");
    checked.recheck().unwrap();
}

#[test]
fn recheck_detects_leaf_name_replacement() {
    let (temporary, root) = fixture_root();
    write_fixture_file(&temporary.path().join("owner"), b"original-object", 0o755);
    let checked = root
        .open_file(Path::new("owner"), InstallFileKind::Executable)
        .unwrap();
    fs::rename(
        temporary.path().join("owner"),
        temporary.path().join("moved-owner"),
    )
    .unwrap();
    write_fixture_file(
        &temporary.path().join("owner"),
        b"replacement-object",
        0o755,
    );

    assert_eq!(
        result_error_kind(checked.recheck()),
        ErrorKind::PermissionDenied
    );
    assert_eq!(read_checked(&checked), b"original-object");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_accepts_absent_extended_acl() {
    let (temporary, root) = fixture_root();
    write_fixture_file(&temporary.path().join("owner"), b"fixed", 0o755);
    let checked = root
        .open_file(Path::new("owner"), InstallFileKind::Executable)
        .unwrap();
    linux_acl::ensure_no_extended_acl(checked.file(), false).unwrap();
    assert_eq!(read_checked(&checked), b"fixed");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_rejects_access_acl_even_when_mode_is_0755() {
    let (temporary, root) = fixture_root();
    let path = temporary.path().join("owner");
    write_fixture_file(&path, b"fixed", 0o755);
    let descriptor = open(&path, LEAF_OPEN_FLAGS, Mode::empty()).unwrap();
    set_extended_acl_fixture(&descriptor, false).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o755
    );

    assert_eq!(
        result_error_kind(root.open_file(Path::new("owner"), InstallFileKind::Executable)),
        ErrorKind::PermissionDenied,
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_rejects_default_acl_and_classifies_xattr_results() {
    assert_eq!(classify_probe(Ok(0)), XattrProbe::Present);
    assert_eq!(classify_probe(Ok(1)), XattrProbe::Present);
    assert_eq!(classify_probe(Err(Errno::NODATA)), XattrProbe::Absent);
    assert_eq!(classify_probe(Err(Errno::RANGE)), XattrProbe::Present);
    assert_eq!(classify_probe(Err(Errno::NOTSUP)), XattrProbe::Unsupported);
    assert_eq!(
        classify_probe(Err(Errno::IO)),
        XattrProbe::Failed(Errno::IO)
    );

    let (temporary, root) = fixture_root();
    let directory = temporary.path().join("with-default-acl");
    create_private_directory(&directory);
    write_fixture_file(&directory.join("owner"), b"fixed", 0o755);
    let descriptor = open(&directory, FIXTURE_DIRECTORY_FLAGS, Mode::empty()).unwrap();
    set_extended_acl_fixture(&descriptor, true).unwrap();
    assert_eq!(
        fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        result_error_kind(root.open_file(
            Path::new("with-default-acl/owner"),
            InstallFileKind::Executable,
        )),
        ErrorKind::PermissionDenied,
    );
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_accepts_empty_deny_and_read_only_acls() {
    let (temporary, root) = fixture_root();
    write_fixture_file(&temporary.path().join("empty"), b"empty", 0o755);
    let empty = root
        .open_file(Path::new("empty"), InstallFileKind::Executable)
        .unwrap();
    assert_eq!(read_checked(&empty), b"empty");

    for (name, acl) in [
        ("deny", FixtureAcl::DenyRead),
        ("read", FixtureAcl::AllowRead),
    ] {
        let path = temporary.path().join(name);
        write_fixture_file(&path, name.as_bytes(), 0o755);
        let descriptor = open(&path, LEAF_OPEN_FLAGS, Mode::empty()).unwrap();
        set_test_acl(&descriptor, acl).unwrap();
        let checked = root
            .open_file(Path::new(name), InstallFileKind::Executable)
            .unwrap();
        assert_eq!(read_checked(&checked), name.as_bytes());
    }
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_rejects_direct_inheritable_and_delete_child_mutation() {
    for (name, acl, directory) in [
        ("direct", FixtureAcl::AllowMutation, false),
        ("inheritable", FixtureAcl::InheritableMutation, true),
        ("delete-child", FixtureAcl::DeleteChild, true),
    ] {
        let (temporary, root) = fixture_root();
        let entry = temporary.path().join(name);
        if directory {
            create_private_directory(&entry);
        } else {
            write_fixture_file(&entry, b"fixed", 0o755);
        }
        let descriptor = open(
            &entry,
            if directory {
                FIXTURE_DIRECTORY_FLAGS
            } else {
                LEAF_OPEN_FLAGS
            },
            Mode::empty(),
        )
        .unwrap();
        set_test_acl(&descriptor, acl).unwrap();
        let mode = fs::metadata(&entry).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode,
            if directory { 0o700 } else { 0o755 },
            "ACL fixture must preserve the mode policy",
        );
        if directory {
            write_fixture_file(&entry.join("owner"), b"fixed", 0o755);
        }
        let relative = if directory {
            Path::new(name).join("owner")
        } else {
            Path::new(name).to_path_buf()
        };
        assert_eq!(
            result_error_kind(root.open_file(&relative, InstallFileKind::Executable)),
            ErrorKind::PermissionDenied,
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_classifies_acl_api_failures_and_entry_limits() {
    assert!(!classify_getter_result(false, Some(Errno::NOENT)).unwrap());
    assert!(classify_getter_result(true, None).unwrap());
    assert_eq!(
        classify_getter_result(false, Some(Errno::IO))
            .unwrap_err()
            .raw_os_error(),
        Some(Errno::IO.raw_os_error()),
    );
    assert_eq!(
        result_error_kind(classify_getter_result(false, None)),
        ErrorKind::InvalidData,
    );
    assert_eq!(
        result_error_kind(classify_getter_result(false, Some(Errno::OPNOTSUPP))),
        ErrorKind::Unsupported,
    );
    classify_zero_result(0, None, "test operation").unwrap();
    assert_eq!(
        classify_zero_result(-1, Some(Errno::IO), "test operation")
            .unwrap_err()
            .raw_os_error(),
        Some(Errno::IO.raw_os_error()),
    );
    assert_eq!(
        result_error_kind(classify_zero_result(1, None, "test operation")),
        ErrorKind::InvalidData,
    );
    assert_eq!(
        classify_entry_result(0, None, true, ACL_MAX_ENTRIES - 1).unwrap(),
        EntryStatus::Entry,
    );
    assert_eq!(
        classify_entry_result(-1, Some(Errno::INVAL), false, ACL_MAX_ENTRIES).unwrap(),
        EntryStatus::End,
    );
    assert_eq!(
        result_error_kind(classify_entry_result(0, None, true, ACL_MAX_ENTRIES)),
        ErrorKind::PermissionDenied,
    );
    assert_eq!(
        classify_entry_result(-1, Some(Errno::IO), false, 0)
            .unwrap_err()
            .raw_os_error(),
        Some(Errno::IO.raw_os_error()),
    );
    assert_eq!(
        result_error_kind(classify_entry_result(0, None, false, 0)),
        ErrorKind::InvalidData,
    );
    const EXPECTED_PERMISSION_MASK: u64 = (1 << 1)
        | (1 << 2)
        | (1 << 3)
        | (1 << 4)
        | (1 << 5)
        | (1 << 6)
        | (1 << 7)
        | (1 << 8)
        | (1 << 9)
        | (1 << 10)
        | (1 << 11)
        | (1 << 12)
        | (1 << 13)
        | (1 << 20);
    validate_maximal_mask(EXPECTED_PERMISSION_MASK).unwrap();
    assert_eq!(
        result_error_kind(validate_maximal_mask(0)),
        ErrorKind::Unsupported,
    );
    validate_acl_entry(2, 0).unwrap();
    validate_acl_entry(1, 1 << 1).unwrap();
    assert_eq!(
        result_error_kind(validate_acl_entry(99, 0)),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        result_error_kind(validate_acl_entry(1, 1 << 2)),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        result_error_kind(validate_acl_entry(2, 1 << 30)),
        ErrorKind::PermissionDenied,
    );
    for mutation in [
        1 << 2,
        1 << 4,
        1 << 5,
        1 << 6,
        1 << 8,
        1 << 10,
        1 << 12,
        1 << 13,
    ] {
        assert_eq!(
            result_error_kind(validate_acl_entry(1, mutation)),
            ErrorKind::PermissionDenied,
            "mutating ACL permission {mutation:#x} must fail closed",
        );
    }
}

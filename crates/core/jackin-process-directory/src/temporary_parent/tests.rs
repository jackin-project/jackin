// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::validate_temporary_parent_facts;
use rustix::fs::FileType;
#[cfg(target_os = "linux")]
use rustix::fs::{Mode, OFlags, open};
#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

#[cfg(target_os = "linux")]
const FIXTURE_DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

#[test]
fn temporary_parent_facts_accept_private_directories_and_root_sticky_tmp() {
    validate_temporary_parent_facts(FileType::Directory, 501, 0o700, 501)
        .expect("private effective-user parent");
    validate_temporary_parent_facts(FileType::Directory, 0, 0o1777, 501)
        .expect("root-owned sticky standard temporary parent");
    validate_temporary_parent_facts(FileType::Directory, 501, 0o1777, 501)
        .expect("effective-user-owned sticky temporary parent");
}

#[test]
fn direct_validation_requires_canonical_absolute_paths() {
    let cwd = std::env::current_dir().expect("current directory");
    let fixture = tempfile::tempdir_in(&cwd).expect("fixture beneath current directory");
    let physical = fixture.path().join("private-parent");
    fs::create_dir(&physical).expect("create private parent");
    fs::set_permissions(&physical, fs::Permissions::from_mode(0o700))
        .expect("protect private parent");
    let canonical = fs::canonicalize(&physical).expect("canonical private parent");
    super::validate_temporary_parent(&canonical).expect("canonical path is admitted");

    let relative = physical
        .strip_prefix(&cwd)
        .expect("fixture uses current directory as its base");
    let relative_error =
        super::validate_temporary_parent(relative).expect_err("relative path is not admitted");
    assert_eq!(relative_error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(
        relative_error
            .to_string()
            .contains("canonical absolute path")
    );

    let alias = fixture.path().join("parent-alias");
    symlink(&physical, &alias).expect("create symlink alias");
    let alias_error =
        super::validate_temporary_parent(&alias).expect_err("symlink path is not admitted");
    assert_eq!(alias_error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(alias_error.to_string().contains("canonical absolute path"));
}

#[test]
fn filesystem_validation_accepts_effective_user_owned_sticky_parent() {
    let fixture = tempfile::tempdir().expect("fixture directory");
    let sticky = fixture.path().join("sticky-parent");
    fs::create_dir(&sticky).expect("create effective-user parent");
    fs::set_permissions(&sticky, fs::Permissions::from_mode(0o1777))
        .expect("set exact sticky mode");
    let sticky = fs::canonicalize(sticky).expect("canonical sticky parent");
    super::validate_temporary_parent(&sticky)
        .expect("effective-user-owned sticky parent is an accepted temporary parent");
}

#[test]
fn temporary_parent_facts_reject_foreign_owners_and_nonsticky_writable_ancestors() {
    assert!(
        validate_temporary_parent_facts(FileType::Directory, 502, 0o755, 501).is_err(),
        "foreign owner must fail closed"
    );
    assert!(
        validate_temporary_parent_facts(FileType::Directory, 501, 0o777, 501).is_err(),
        "effective-user parent remains unsafe when foreign users can rename children"
    );
    assert!(
        validate_temporary_parent_facts(FileType::RegularFile, 501, 0o700, 501).is_err(),
        "non-directory path component must fail closed"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn filesystem_validation_rejects_an_inherited_default_acl() {
    let fixture = tempfile::tempdir().expect("fixture directory");
    let parent = fixture.path().join("acl-parent");
    fs::create_dir(&parent).expect("create ACL parent");
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o700))
        .expect("protect ACL parent mode");
    let descriptor =
        File::from(open(&parent, FIXTURE_DIRECTORY_FLAGS, Mode::empty()).expect("open ACL parent"));
    super::super::secure_install::set_source_linux_acl_fixture(&descriptor, true)
        .expect("install default ACL fixture");

    let parent = fs::canonicalize(parent).expect("canonical ACL parent");
    let error = super::validate_temporary_parent(Path::new(&parent))
        .expect_err("inherited ACL must be refused");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
}

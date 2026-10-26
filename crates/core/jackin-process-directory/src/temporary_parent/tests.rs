// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::validate_temporary_parent_facts;
use rustix::fs::FileType;

#[test]
fn temporary_parent_facts_accept_private_directories_and_root_sticky_tmp() {
    validate_temporary_parent_facts(FileType::Directory, 501, 0o700, 501)
        .expect("private effective-user parent");
    validate_temporary_parent_facts(FileType::Directory, 0, 0o1777, 501)
        .expect("root-owned sticky standard temporary parent");
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

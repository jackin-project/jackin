// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::branch::branch_name;

#[test]
fn plain_selector_gets_scratch_branch() {
    assert_eq!(branch_name("payments", None), "jackin/scratch/payments");
}

#[test]
fn suffix_appends_after_namespaced_leaf() {
    assert_eq!(
        branch_name("team/role", Some("b2")),
        "jackin/scratch/team/role-b2"
    );
    assert_eq!(branch_name("role", Some("b2")), "jackin/scratch/role-b2");
}

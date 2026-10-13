// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::audit_arguments;

#[test]
fn restored_database_disables_network_fetch() {
    assert_eq!(
        audit_arguments(true),
        ["audit", "--no-yanked", "--no-fetch", "--stale"]
    );
}

#[test]
fn missing_database_allows_the_initial_fetch() {
    assert_eq!(audit_arguments(false), ["audit", "--no-yanked"]);
}

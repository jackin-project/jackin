// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn encodes_instance_name_as_hex() {
    assert_eq!(account_credentials_filename("main"), "acct-6d61696e.json");
}

#[test]
fn empty_instance_keeps_affixes() {
    assert_eq!(account_credentials_filename(""), "acct-.json");
}

#[test]
fn container_path_prefixes_credentials_dir() {
    assert_eq!(
        account_credentials_container_path("main"),
        format!("{ACCOUNT_CREDENTIALS_DIR}/acct-6d61696e.json")
    );
}

#[test]
fn distinct_instances_stay_distinct() {
    assert_ne!(
        account_credentials_filename("main"),
        account_credentials_filename("worker")
    );
}

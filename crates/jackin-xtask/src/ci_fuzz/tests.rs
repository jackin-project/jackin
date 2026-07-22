// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::contract_for;

#[test]
fn maps_each_fuzzing_crate_to_its_complete_contract() {
    let cases = [
        (
            "jackin-config",
            "crates/jackin-config",
            &["config_migrate", "workspace_migrate"][..],
        ),
        ("jackin-env", "crates/jackin-env", &["env_resolve"][..]),
        (
            "jackin-manifest",
            "crates/jackin-manifest",
            &["manifest_migrate", "manifest_validate"][..],
        ),
        (
            "jackin-protocol",
            "crates/jackin-protocol",
            &["decode_frames"][..],
        ),
        (
            "jackin-term",
            "crates/jackin-term",
            &["damage_grid_process"][..],
        ),
    ];

    for (package, directory, targets) in cases {
        let contract = contract_for(package).unwrap();
        assert_eq!(contract.directory, directory);
        assert_eq!(contract.targets, targets);
    }
}

#[test]
fn rejects_crates_without_a_fuzz_contract() {
    assert!(contract_for("jackin-xtask").is_none());
}

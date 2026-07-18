// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::collections::BTreeMap;

#[test]
fn auth_modes_are_complete_bounded_and_allowlisted() {
    let valid = CapsuleConfig {
        workdir: "/workspace".to_owned(),
        agents: vec!["codex".to_owned()],
        auth_modes: BTreeMap::from([("codex".to_owned(), "api_key".to_owned())]),
        ..CapsuleConfig::default()
    };
    validate(&valid).unwrap();

    let mut invalid = valid.clone();
    invalid
        .auth_modes
        .insert("codex".to_owned(), "private-mode".to_owned());
    assert!(validate(&invalid).is_err());
    invalid.auth_modes = BTreeMap::from([("claude".to_owned(), "sync".to_owned())]);
    assert!(validate(&invalid).is_err());
}
